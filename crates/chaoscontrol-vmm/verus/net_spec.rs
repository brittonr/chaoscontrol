// Verus specification for network pure functions
verus! {

/// Local unicast check is precise
proof fn local_unicast_precise(mac: [u8; 6])
    ensures
        is_local_unicast_mac(&mac) <==> (mac[0] & 0x03 == 0x02),
{ }

/// Standard locally-administered MAC passes
proof fn standard_local_mac()
    ensures is_local_unicast_mac(&[0x02, 0, 0, 0, 0, 1]),
{ }

}
