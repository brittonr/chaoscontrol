const MAC_ADDRESS_BYTES: usize = 6;

pub(crate) fn bring_up(name: &str) -> bool {
    unsafe {
        let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if socket < 0 {
            return false;
        }
        let mut request: libc::ifreq = std::mem::zeroed();
        let name_bytes = name.as_bytes();
        let copy_length = name_bytes.len().min(libc::IFNAMSIZ - 1);
        for (index, &byte) in name_bytes.iter().enumerate().take(copy_length) {
            request.ifr_name[index] = byte as libc::c_char;
        }
        if libc::ioctl(socket, libc::SIOCGIFFLAGS as libc::Ioctl, &mut request) < 0 {
            libc::close(socket);
            return false;
        }
        request.ifr_ifru.ifru_flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
        if libc::ioctl(socket, libc::SIOCSIFFLAGS as libc::Ioctl, &request) < 0 {
            libc::close(socket);
            return false;
        }
        libc::close(socket);
        true
    }
}

pub(crate) fn read_mac(name: &str) -> Option<[u8; MAC_ADDRESS_BYTES]> {
    unsafe {
        let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if socket < 0 {
            return None;
        }
        let mut request: libc::ifreq = std::mem::zeroed();
        let name_bytes = name.as_bytes();
        let copy_length = name_bytes.len().min(libc::IFNAMSIZ - 1);
        std::ptr::copy_nonoverlapping(
            name_bytes.as_ptr(),
            request.ifr_name.as_mut_ptr().cast(),
            copy_length,
        );
        if libc::ioctl(socket, libc::SIOCGIFHWADDR as libc::Ioctl, &mut request) < 0 {
            libc::close(socket);
            return None;
        }
        libc::close(socket);
        let mut address = [0_u8; MAC_ADDRESS_BYTES];
        for (destination, source) in address
            .iter_mut()
            .zip(request.ifr_ifru.ifru_hwaddr.sa_data.iter())
        {
            *destination = *source as u8;
        }
        Some(address)
    }
}

pub fn parse_cmdline_param(key: &str) -> Option<usize> {
    let command_line = std::fs::read_to_string("/proc/cmdline").ok()?;
    let prefix = format!("{key}=");
    command_line
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&prefix))?
        .parse()
        .ok()
}

pub fn vm_id() -> usize {
    parse_cmdline_param("vm_id").unwrap_or(0)
}

pub fn vm_ip(id: usize) -> smoltcp::wire::Ipv4Address {
    smoltcp::wire::Ipv4Address::new(
        crate::SUBNET_PREFIX[0],
        crate::SUBNET_PREFIX[1],
        crate::SUBNET_PREFIX[2],
        (id + 1) as u8,
    )
}
