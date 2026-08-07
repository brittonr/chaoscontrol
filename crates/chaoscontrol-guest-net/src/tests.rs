#[test]
fn vm_ip_assignment_is_stable() {
    const LAST_VM_ID: usize = 254;
    const LAST_VM_ADDRESS: u8 = 255;

    for (vm_id, host) in [(0, 1), (1, 2), (2, 3), (LAST_VM_ID, LAST_VM_ADDRESS)] {
        assert_eq!(
            crate::interface::vm_ip(vm_id),
            smoltcp::wire::Ipv4Address::new(
                crate::SUBNET_PREFIX[0],
                crate::SUBNET_PREFIX[1],
                crate::SUBNET_PREFIX[2],
                host,
            )
        );
    }
}

#[test]
fn network_clock_is_positive() {
    assert!(crate::runtime::now().total_millis() > 0);
}
