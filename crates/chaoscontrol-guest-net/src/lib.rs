//! TCP/IP networking for ChaosControl guest programs.
//!
//! Provides a [`GuestNetwork`] type that sets up a smoltcp TCP/IP stack
//! over the VM's virtio-net interface (`eth0`).  Guest programs use this
//! instead of the kernel's network stack so they have full control over
//! IP assignment and TCP connections without needing busybox or iproute2.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │              Guest binary                │
//! │   GuestNetwork::poll() + tcp sockets     │
//! ├──────────────────────────────────────────┤
//! │   smoltcp (TCP/IP + ARP + Ethernet)      │
//! ├──────────────────────────────────────────┤
//! │   AF_PACKET raw socket on eth0           │
//! ├──────────────────────────────────────────┤
//! │   Linux virtio_net driver                │
//! ├──────────────────────────────────────────┤
//! │   VMM virtio-net device ↔ NetworkFabric  │
//! └──────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,no_run
//! use chaoscontrol_guest_net::GuestNetwork;
//! use smoltcp::wire::IpAddress;
//!
//! let mut net = GuestNetwork::init(0, 3); // VM 0 of 3
//! let server = net.tcp_listen(8080);
//!
//! loop {
//!     net.poll();
//!     // ... use net.tcp_recv / net.tcp_send ...
//! }
//! ```

use log::{debug, info, warn};
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Medium, RawSocket};
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address};

// Re-export types that guest programs need.
pub use smoltcp::iface::SocketHandle as TcpHandle;
pub use smoltcp::socket::tcp::State as TcpState;
pub use smoltcp::wire::Ipv4Address as Ipv4Addr;

// ═══════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════

/// Network interface name (Linux virtio-net driver creates this).
const IFACE_NAME: &str = "eth0";

/// Subnet for inter-VM communication: 10.0.0.0/24
/// VM `i` gets IP 10.0.0.{i+1}.
const SUBNET_PREFIX: [u8; 3] = [10, 0, 0];
const SUBNET_MASK: u8 = 24;

/// Maximum number of TCP sockets per guest.
const MAX_SOCKETS: usize = 16;

/// TCP socket buffer sizes.
const TCP_RX_BUF_SIZE: usize = 16384;
const TCP_TX_BUF_SIZE: usize = 16384;

// ═══════════════════════════════════════════════════════════════════════
//  Interface setup helpers (ioctl)
// ═══════════════════════════════════════════════════════════════════════

/// Bring up a network interface using ioctl.
///
/// Sets IFF_UP | IFF_RUNNING on the named interface.
fn bring_up_interface(name: &str) -> bool {
    unsafe {
        let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if sock < 0 {
            return false;
        }

        let mut ifr: libc::ifreq = std::mem::zeroed();
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(libc::IFNAMSIZ - 1);
        for i in 0..copy_len {
            ifr.ifr_name[i] = name_bytes[i] as libc::c_char;
        }

        if libc::ioctl(sock, libc::SIOCGIFFLAGS as libc::Ioctl, &mut ifr) < 0 {
            libc::close(sock);
            return false;
        }

        ifr.ifr_ifru.ifru_flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
        if libc::ioctl(sock, libc::SIOCSIFFLAGS as libc::Ioctl, &ifr) < 0 {
            libc::close(sock);
            return false;
        }

        libc::close(sock);
        true
    }
}

/// Read the MAC address of a network interface.
fn read_mac(name: &str) -> Option<[u8; 6]> {
    unsafe {
        let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if sock < 0 {
            return None;
        }

        let mut ifr: libc::ifreq = std::mem::zeroed();
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(libc::IFNAMSIZ - 1);
        std::ptr::copy_nonoverlapping(
            name_bytes.as_ptr(),
            ifr.ifr_name.as_mut_ptr() as *mut u8,
            copy_len,
        );

        if libc::ioctl(sock, libc::SIOCGIFHWADDR as libc::Ioctl, &mut ifr) < 0 {
            libc::close(sock);
            return None;
        }

        libc::close(sock);

        let mut mac = [0u8; 6];
        mac.copy_from_slice(
            &ifr.ifr_ifru.ifru_hwaddr.sa_data[..6]
                .iter()
                .map(|&b| b as u8)
                .collect::<Vec<u8>>(),
        );
        Some(mac)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Kernel cmdline helpers
// ═══════════════════════════════════════════════════════════════════════

/// Parse a `key=value` integer parameter from `/proc/cmdline`.
pub fn parse_cmdline_param(key: &str) -> Option<usize> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").ok()?;
    let prefix = format!("{}=", key);
    for token in cmdline.split_whitespace() {
        if let Some(val) = token.strip_prefix(&prefix) {
            return val.parse().ok();
        }
    }
    None
}

/// Get the VM ID from the kernel command line (`vm_id=N`).
pub fn vm_id() -> usize {
    parse_cmdline_param("vm_id").unwrap_or(0)
}

/// Compute the IPv4 address for a given VM ID.
pub fn vm_ip(id: usize) -> Ipv4Address {
    Ipv4Address::new(
        SUBNET_PREFIX[0],
        SUBNET_PREFIX[1],
        SUBNET_PREFIX[2],
        (id + 1) as u8,
    )
}

// ═══════════════════════════════════════════════════════════════════════
//  GuestNetwork
// ═══════════════════════════════════════════════════════════════════════

/// TCP/IP network stack for a ChaosControl guest VM.
///
/// Wraps smoltcp's interface + socket set over a raw AF_PACKET socket
/// on the VM's `eth0` virtio-net interface.
pub struct GuestNetwork {
    /// smoltcp network interface.
    iface: Interface,
    /// Raw socket device (AF_PACKET on eth0).
    device: RawSocket,
    /// Socket set (TCP sockets live here).
    sockets: SocketSet<'static>,
    /// This VM's ID.
    pub vm_id: usize,
    /// Total number of VMs in the simulation.
    pub num_vms: usize,
    /// This VM's IPv4 address.
    pub ip: Ipv4Address,
}

impl GuestNetwork {
    /// Initialize networking for this VM.
    ///
    /// 1. Mounts `/proc` (for cmdline parsing) and `/sys` (if needed)
    /// 2. Brings up `eth0`
    /// 3. Creates smoltcp interface with IP `10.0.0.{vm_id+1}/24`
    ///
    /// # Panics
    ///
    /// Panics if `eth0` cannot be brought up or the raw socket fails.
    pub fn init(vm_id: usize, num_vms: usize) -> Self {
        // Mount /proc for cmdline access and /sys for interface info
        mount_pseudo_fs();

        // Bring up eth0 — retry a few times since the virtio-net
        // driver may not have finished probing yet
        let mut retries = 0;
        loop {
            if bring_up_interface(IFACE_NAME) {
                break;
            }
            retries += 1;
            if retries >= 50 {
                panic!(
                    "Failed to bring up {} after {} retries. \
                     Is CONFIG_VIRTIO_NET=y in the kernel?",
                    IFACE_NAME, retries
                );
            }
            // Brief sleep to let the kernel finish device probing
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Read MAC address
        let mac = read_mac(IFACE_NAME).expect("Failed to read MAC address");
        info!(
            "VM{}: MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            vm_id, mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );

        // Create raw socket on eth0
        let mut device = RawSocket::new(IFACE_NAME, Medium::Ethernet)
            .expect("Failed to create raw socket on eth0");

        // Configure smoltcp interface
        let hw_addr = HardwareAddress::Ethernet(EthernetAddress(mac));
        let config = Config::new(hw_addr);
        let mut iface = Interface::new(config, &mut device, smoltcp_now());

        // Assign IP address
        let ip = vm_ip(vm_id);
        iface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(IpAddress::Ipv4(ip), SUBNET_MASK))
                .expect("Failed to add IP address");
        });

        info!("VM{}: IP {}", vm_id, ip);

        // Pre-allocate socket storage
        let sockets = SocketSet::new(Vec::with_capacity(MAX_SOCKETS));

        Self {
            iface,
            device,
            sockets,
            vm_id,
            num_vms,
            ip,
        }
    }

    /// Poll the network interface — process incoming/outgoing packets.
    ///
    /// Must be called regularly in the guest's main loop.
    pub fn poll(&mut self) {
        self.iface
            .poll(smoltcp_now(), &mut self.device, &mut self.sockets);
    }

    /// Get the IPv4 address of another VM by its ID.
    pub fn peer_ip(&self, peer_id: usize) -> Ipv4Address {
        vm_ip(peer_id)
    }

    // ─── TCP socket management ───────────────────────────────────

    /// Create a TCP socket and start listening on the given port.
    ///
    /// Returns a socket handle for use with [`tcp_recv`] and [`tcp_send`].
    pub fn tcp_listen(&mut self, port: u16) -> SocketHandle {
        let rx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_RX_BUF_SIZE]);
        let tx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_TX_BUF_SIZE]);
        let mut socket = tcp::Socket::new(rx_buf, tx_buf);
        socket.listen(port).expect("TCP listen failed");
        info!("VM{}: listening on port {}", self.vm_id, port);
        self.sockets.add(socket)
    }

    /// Create a TCP socket and connect to a remote address.
    ///
    /// Returns a socket handle. The connection is non-blocking — poll
    /// until [`tcp_is_active`] returns `true`.
    pub fn tcp_connect(&mut self, addr: Ipv4Address, port: u16) -> SocketHandle {
        let rx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_RX_BUF_SIZE]);
        let tx_buf = tcp::SocketBuffer::new(vec![0u8; TCP_TX_BUF_SIZE]);
        let mut socket = tcp::Socket::new(rx_buf, tx_buf);

        // Use ephemeral port based on vm_id to avoid collisions
        let local_port = 49152 + (self.vm_id as u16 * 100) + (port % 100);
        let local = (self.ip, local_port);
        let remote = (IpAddress::Ipv4(addr), port);

        socket
            .connect(self.iface.context(), remote, local)
            .expect("TCP connect failed");

        info!(
            "VM{}: connecting to {}:{} from port {}",
            self.vm_id, addr, port, local_port
        );
        self.sockets.add(socket)
    }

    /// Check if a TCP socket has an active connection.
    pub fn tcp_is_active(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.is_active()
    }

    /// Check if a TCP socket can receive data.
    pub fn tcp_can_recv(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.can_recv()
    }

    /// Check if a TCP socket can send data.
    pub fn tcp_can_send(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.can_send()
    }

    /// Receive data from a TCP socket.
    ///
    /// Returns the number of bytes read, or 0 if no data available.
    pub fn tcp_recv(&mut self, handle: SocketHandle, buf: &mut [u8]) -> usize {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        match socket.recv_slice(buf) {
            Ok(n) => {
                if n > 0 {
                    debug!("VM{}: recv {} bytes", self.vm_id, n);
                }
                n
            }
            Err(_) => 0,
        }
    }

    /// Send data through a TCP socket.
    ///
    /// Returns the number of bytes actually enqueued (may be less than
    /// `data.len()` if the TX buffer is full).
    pub fn tcp_send(&mut self, handle: SocketHandle, data: &[u8]) -> usize {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        match socket.send_slice(data) {
            Ok(n) => {
                if n > 0 {
                    debug!("VM{}: sent {} bytes", self.vm_id, n);
                }
                n
            }
            Err(_) => 0,
        }
    }

    /// Close a TCP socket gracefully (sends FIN).
    pub fn tcp_close(&mut self, handle: SocketHandle) {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.close();
        info!("VM{}: TCP socket closed", self.vm_id);
    }

    /// Get the TCP socket state (for debugging).
    pub fn tcp_state(&self, handle: SocketHandle) -> tcp::State {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.state()
    }

    /// Remove a closed socket from the set, freeing its slot.
    pub fn tcp_remove(&mut self, handle: SocketHandle) {
        self.sockets.remove(handle);
    }

    /// Create a fresh TCP socket for re-accepting after a connection closes.
    ///
    /// This removes the old socket and creates a new one listening on
    /// the same port.
    pub fn tcp_re_listen(&mut self, old_handle: SocketHandle, port: u16) -> SocketHandle {
        self.sockets.remove(old_handle);
        self.tcp_listen(port)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════

/// Get current time as a smoltcp Instant.
fn smoltcp_now() -> Instant {
    Instant::from_millis(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
    )
}

/// Mount /proc and /sys pseudo-filesystems.
fn mount_pseudo_fs() {
    unsafe {
        libc::mkdir(c"/proc".as_ptr().cast(), 0o555);
        libc::mount(
            c"proc".as_ptr().cast(),
            c"/proc".as_ptr().cast(),
            c"proc".as_ptr().cast(),
            0,
            std::ptr::null(),
        );

        libc::mkdir(c"/sys".as_ptr().cast(), 0o555);
        libc::mount(
            c"sysfs".as_ptr().cast(),
            c"/sys".as_ptr().cast(),
            c"sysfs".as_ptr().cast(),
            0,
            std::ptr::null(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_ip_assignment() {
        assert_eq!(vm_ip(0), Ipv4Address::new(10, 0, 0, 1));
        assert_eq!(vm_ip(1), Ipv4Address::new(10, 0, 0, 2));
        assert_eq!(vm_ip(2), Ipv4Address::new(10, 0, 0, 3));
        assert_eq!(vm_ip(254), Ipv4Address::new(10, 0, 0, 255));
    }

    #[test]
    fn test_smoltcp_now_is_positive() {
        let now = smoltcp_now();
        assert!(now.total_millis() > 0);
    }
}
