//! Verified pure functions for the deterministic network device.
//!
//! The network device is primarily stateful queue management (imperative shell).
//! This module documents the invariants that would be verified with Verus.

/// Validate that a MAC address is a locally-administered unicast address.
///
/// Bit 0 of the first octet: 0 = unicast, 1 = multicast
/// Bit 1 of the first octet: 0 = globally unique, 1 = locally administered
///
/// For deterministic simulation, we always use locally-administered
/// unicast addresses (bit 1 set, bit 0 clear).
///
/// # Examples
///
/// ```
/// use chaoscontrol_vmm::verified::net::is_local_unicast_mac;
///
/// assert!(is_local_unicast_mac(&[0x02, 0, 0, 0, 0, 1]));
/// assert!(!is_local_unicast_mac(&[0x00, 0, 0, 0, 0, 1])); // globally unique
/// assert!(!is_local_unicast_mac(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])); // broadcast
/// ```
pub fn is_local_unicast_mac(mac: &[u8; 6]) -> bool {
    let result = (mac[0] & 0x01) == 0 && (mac[0] & 0x02) != 0;
    
    // Tiger Style: positive and negative assertions
    debug_assert!(result == ((mac[0] & 0x03) == 0x02));
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_unicast_mac() {
        assert!(is_local_unicast_mac(&[0x02, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn global_unicast_mac() {
        assert!(!is_local_unicast_mac(&[0x00, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn multicast_mac() {
        assert!(!is_local_unicast_mac(&[0x03, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn broadcast_mac() {
        assert!(!is_local_unicast_mac(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]));
    }

    #[test]
    fn locally_administered_multicast() {
        // Both bits set = locally administered multicast (not valid for our use)
        assert!(!is_local_unicast_mac(&[0x03, 0, 0, 0, 0, 1]));
    }

    #[test]
    fn various_local_unicast_macs() {
        assert!(is_local_unicast_mac(&[0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]));
        assert!(is_local_unicast_mac(&[0x06, 0, 0, 0, 0, 1]));
        assert!(is_local_unicast_mac(&[0x0A, 0, 0, 0, 0, 1]));
        assert!(is_local_unicast_mac(&[0x0E, 0, 0, 0, 0, 1]));
    }
}
