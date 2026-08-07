mod socket;

const INTERFACE_RETRY_LIMIT: usize = 50;
const INTERFACE_RETRY_DELAY_MILLISECONDS: u64 = 100;

/// TCP/IP network stack for a ChaosControl guest VM.
pub struct GuestNetwork {
    iface: smoltcp::iface::Interface,
    device: smoltcp::phy::RawSocket,
    sockets: smoltcp::iface::SocketSet<'static>,
    pub vm_id: usize,
    pub num_vms: usize,
    pub ip: smoltcp::wire::Ipv4Address,
}

impl GuestNetwork {
    /// Initialize networking for this VM.
    pub fn init(vm_id: usize, num_vms: usize) -> Self {
        crate::runtime::mount_pseudo_filesystems();
        let mut retries = 0;
        loop {
            if crate::interface::bring_up(crate::IFACE_NAME) {
                break;
            }
            retries += 1;
            assert!(
                retries < INTERFACE_RETRY_LIMIT,
                "failed to bring up {} after {} retries; check CONFIG_VIRTIO_NET",
                crate::IFACE_NAME,
                retries
            );
            std::thread::sleep(std::time::Duration::from_millis(
                INTERFACE_RETRY_DELAY_MILLISECONDS,
            ));
        }

        let mac = crate::interface::read_mac(crate::IFACE_NAME)
            .expect("failed to read network interface MAC address");
        let [first, second, third, fourth, fifth, sixth] = mac;
        log::info!(
            "VM{vm_id}: MAC {first:02x}:{second:02x}:{third:02x}:{fourth:02x}:{fifth:02x}:{sixth:02x}"
        );
        let mut device =
            smoltcp::phy::RawSocket::new(crate::IFACE_NAME, smoltcp::phy::Medium::Ethernet)
                .expect("failed to create raw socket on network interface");
        let hardware =
            smoltcp::wire::HardwareAddress::Ethernet(smoltcp::wire::EthernetAddress(mac));
        let config = smoltcp::iface::Config::new(hardware);
        let mut iface = smoltcp::iface::Interface::new(config, &mut device, crate::runtime::now());
        let ip = crate::interface::vm_ip(vm_id);
        iface.update_ip_addrs(|addresses| {
            addresses
                .push(smoltcp::wire::IpCidr::new(
                    smoltcp::wire::IpAddress::Ipv4(ip),
                    crate::SUBNET_MASK,
                ))
                .expect("failed to add guest IP address");
        });
        log::info!("VM{vm_id}: IP {ip}");
        let sockets = smoltcp::iface::SocketSet::new(Vec::with_capacity(crate::MAX_SOCKETS));
        Self {
            iface,
            device,
            sockets,
            vm_id,
            num_vms,
            ip,
        }
    }

    /// Poll incoming and outgoing network packets.
    pub fn poll(&mut self) {
        self.iface
            .poll(crate::runtime::now(), &mut self.device, &mut self.sockets);
    }

    /// Get another VM's IPv4 address.
    pub fn peer_ip(&self, peer_id: usize) -> smoltcp::wire::Ipv4Address {
        crate::interface::vm_ip(peer_id)
    }
}
