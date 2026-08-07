#![allow(unknown_lints)]

//! TCP/IP networking for ChaosControl guest programs.
//!
//! [`GuestNetwork`] wraps a smoltcp stack over the VM's `eth0` virtio-net
//! interface. Guest programs can poll TCP sockets without a separate userspace
//! network configuration tool.
//!
//! ```rust,no_run
//! use chaoscontrol_guest_net::GuestNetwork;
//!
//! let mut network = GuestNetwork::init(0, 3);
//! let server = network.tcp_listen(8080);
//! loop {
//!     network.poll();
//!     let _ = server;
//! }
//! ```

mod interface;
mod runtime;
mod stack;

pub use interface::{parse_cmdline_param, vm_id, vm_ip};
pub use stack::GuestNetwork;

pub type TcpHandle = smoltcp::iface::SocketHandle;
pub type TcpState = smoltcp::socket::tcp::State;
pub type Ipv4Addr = smoltcp::wire::Ipv4Address;

pub(crate) const IFACE_NAME: &str = "eth0";
pub(crate) const SUBNET_PREFIX: [u8; 3] = [10, 0, 0];
pub(crate) const SUBNET_MASK: u8 = 24;
pub(crate) const MAX_SOCKETS: usize = 16;
pub(crate) const TCP_RX_BUF_SIZE: usize = 16_384;
pub(crate) const TCP_TX_BUF_SIZE: usize = 16_384;

#[cfg(test)]
mod tests;
