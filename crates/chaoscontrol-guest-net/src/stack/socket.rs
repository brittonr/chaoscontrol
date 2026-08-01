const EPHEMERAL_PORT_START: u16 = 49_152;
const EPHEMERAL_PORT_SPAN: u16 = 100;

impl super::GuestNetwork {
    /// Create a TCP socket and listen on a port.
    pub fn tcp_listen(&mut self, port: u16) -> smoltcp::iface::SocketHandle {
        let receive = smoltcp::socket::tcp::SocketBuffer::new(vec![0_u8; crate::TCP_RX_BUF_SIZE]);
        let transmit = smoltcp::socket::tcp::SocketBuffer::new(vec![0_u8; crate::TCP_TX_BUF_SIZE]);
        let mut socket = smoltcp::socket::tcp::Socket::new(receive, transmit);
        socket.listen(port).expect("TCP listen failed");
        log::info!("VM{}: listening on port {port}", self.vm_id);
        self.sockets.add(socket)
    }

    /// Create a TCP socket and connect to a remote address.
    pub fn tcp_connect(
        &mut self,
        address: smoltcp::wire::Ipv4Address,
        port: u16,
    ) -> smoltcp::iface::SocketHandle {
        let receive = smoltcp::socket::tcp::SocketBuffer::new(vec![0_u8; crate::TCP_RX_BUF_SIZE]);
        let transmit = smoltcp::socket::tcp::SocketBuffer::new(vec![0_u8; crate::TCP_TX_BUF_SIZE]);
        let mut socket = smoltcp::socket::tcp::Socket::new(receive, transmit);
        let local_port = EPHEMERAL_PORT_START
            + (self.vm_id as u16 * EPHEMERAL_PORT_SPAN)
            + (port % EPHEMERAL_PORT_SPAN);
        socket
            .connect(
                self.iface.context(),
                (smoltcp::wire::IpAddress::Ipv4(address), port),
                (self.ip, local_port),
            )
            .expect("TCP connect failed");
        log::info!(
            "VM{}: connecting to {address}:{port} from port {local_port}",
            self.vm_id
        );
        self.sockets.add(socket)
    }

    pub fn tcp_is_active(&self, handle: smoltcp::iface::SocketHandle) -> bool {
        self.sockets
            .get::<smoltcp::socket::tcp::Socket>(handle)
            .is_active()
    }

    pub fn tcp_can_recv(&self, handle: smoltcp::iface::SocketHandle) -> bool {
        self.sockets
            .get::<smoltcp::socket::tcp::Socket>(handle)
            .can_recv()
    }

    pub fn tcp_can_send(&self, handle: smoltcp::iface::SocketHandle) -> bool {
        self.sockets
            .get::<smoltcp::socket::tcp::Socket>(handle)
            .can_send()
    }

    pub fn tcp_recv(&mut self, handle: smoltcp::iface::SocketHandle, buffer: &mut [u8]) -> usize {
        match self
            .sockets
            .get_mut::<smoltcp::socket::tcp::Socket>(handle)
            .recv_slice(buffer)
        {
            Ok(length) => {
                if length > 0 {
                    log::debug!("VM{}: received {length} bytes", self.vm_id);
                }
                length
            }
            Err(_) => 0,
        }
    }

    pub fn tcp_send(&mut self, handle: smoltcp::iface::SocketHandle, data: &[u8]) -> usize {
        match self
            .sockets
            .get_mut::<smoltcp::socket::tcp::Socket>(handle)
            .send_slice(data)
        {
            Ok(length) => {
                if length > 0 {
                    log::debug!("VM{}: sent {length} bytes", self.vm_id);
                }
                length
            }
            Err(_) => 0,
        }
    }

    pub fn tcp_close(&mut self, handle: smoltcp::iface::SocketHandle) {
        self.sockets
            .get_mut::<smoltcp::socket::tcp::Socket>(handle)
            .close();
        log::info!("VM{}: TCP socket closed", self.vm_id);
    }

    pub fn tcp_state(&self, handle: smoltcp::iface::SocketHandle) -> smoltcp::socket::tcp::State {
        self.sockets
            .get::<smoltcp::socket::tcp::Socket>(handle)
            .state()
    }

    pub fn tcp_remove(&mut self, handle: smoltcp::iface::SocketHandle) {
        self.sockets.remove(handle);
    }

    pub fn tcp_re_listen(
        &mut self,
        old_handle: smoltcp::iface::SocketHandle,
        port: u16,
    ) -> smoltcp::iface::SocketHandle {
        self.sockets.remove(old_handle);
        self.tcp_listen(port)
    }
}
