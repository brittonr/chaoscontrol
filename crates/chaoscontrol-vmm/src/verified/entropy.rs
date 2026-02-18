//! Verified pure functions for the deterministic entropy device.
//!
//! The entropy device uses a ChaCha20-based PRNG (imperative shell) initialized
//! with a seed expansion function (pure functional core).

/// Expand a u64 seed into a 32-byte array for ChaCha20.
///
/// Places the seed's little-endian bytes in the first 8 bytes of a zeroed array.
/// This is a pure function suitable for formal verification with Verus.
///
/// # Examples
///
/// ```
/// use chaoscontrol_vmm::verified::entropy::expand_seed;
///
/// let seed = expand_seed(0x0102030405060708);
/// assert_eq!(&seed[..8], &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
/// assert_eq!(&seed[8..], &[0u8; 24]);
/// ```
pub fn expand_seed(seed: u64) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&seed.to_le_bytes());

    // Tiger Style: Positive assertion - first 8 bytes match seed
    debug_assert_eq!(&key[..8], &seed.to_le_bytes());

    // Tiger Style: Negative assertion - remaining bytes are zero
    debug_assert!(key[8..].iter().all(|&b| b == 0));

    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_seed_places_bytes_correctly() {
        let seed = 0x0102030405060708u64;
        let expanded = expand_seed(seed);
        // Little-endian: LSB first
        assert_eq!(expanded[0], 0x08);
        assert_eq!(expanded[1], 0x07);
        assert_eq!(expanded[2], 0x06);
        assert_eq!(expanded[3], 0x05);
        assert_eq!(expanded[4], 0x04);
        assert_eq!(expanded[5], 0x03);
        assert_eq!(expanded[6], 0x02);
        assert_eq!(expanded[7], 0x01);
    }

    #[test]
    fn expand_seed_zeroes_remaining() {
        let expanded = expand_seed(0xFFFFFFFFFFFFFFFF);
        for i in 8..32 {
            assert_eq!(expanded[i], 0);
        }
    }

    #[test]
    fn expand_seed_deterministic() {
        let seed = 42;
        let a = expand_seed(seed);
        let b = expand_seed(seed);
        assert_eq!(a, b);
    }

    #[test]
    fn expand_seed_injective() {
        let a = expand_seed(1);
        let b = expand_seed(2);
        assert_ne!(a, b);
    }

    #[test]
    fn expand_zero_seed() {
        let expanded = expand_seed(0);
        assert_eq!(expanded, [0u8; 32]);
    }

    #[test]
    fn expand_max_seed() {
        let seed = u64::MAX;
        let expanded = expand_seed(seed);
        assert_eq!(&expanded[..8], &[0xFF; 8]);
        assert_eq!(&expanded[8..], &[0u8; 24]);
    }
}
