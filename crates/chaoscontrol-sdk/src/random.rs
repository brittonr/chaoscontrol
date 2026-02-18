//! Guided randomness — the VMM controls these values for exploration.
//!
//! Instead of using `rand` or hardware RNG, guest code should use these
//! functions for any random decisions that affect test behavior.  The VMM
//! can then vary these values across runs to explore different execution
//! paths, and replay them exactly for reproduction.
//!
//! When running outside a ChaosControl VM, these functions return
//! deterministic values based on the VMM's seeded PRNG.

use chaoscontrol_protocol::*;
use crate::transport;

/// Get a guided random `u64` from the VMM.
///
/// The VMM returns a value from its deterministic PRNG.  Same seed
/// + same execution path = same value.  Different seeds explore
///   different choices.
///
/// # Example
///
/// ```rust,ignore
/// let delay_ms = chaoscontrol_sdk::random::get_random() % 1000;
/// sleep(Duration::from_millis(delay_ms));
/// ```
pub fn get_random() -> u64 {
    let (result, _status) = transport::hypercall_simple(CMD_RANDOM_GET, 0);
    result
}

/// Choose an index from `0..n` using guided randomness.
///
/// The VMM controls the choice for systematic exploration.
/// Returns a value in `0..n`.  Panics if `n == 0`.
///
/// # Example
///
/// ```rust,ignore
/// let actions = ["read", "write", "delete"];
/// let idx = chaoscontrol_sdk::random::random_choice(actions.len());
/// execute(actions[idx]);
/// ```
pub fn random_choice(n: usize) -> usize {
    debug_assert!(n > 0, "random_choice: n must be > 0");
    if n <= 1 {
        return 0;
    }
    let (result, _status) = transport::hypercall_simple(CMD_RANDOM_CHOICE, n as u32);
    (result as usize) % n
}

/// Fill a byte slice with guided random bytes from the VMM.
///
/// Issues multiple `get_random` calls to fill the buffer.
pub fn fill_bytes(buf: &mut [u8]) {
    let mut offset = 0;
    while offset < buf.len() {
        let val = get_random();
        let bytes = val.to_le_bytes();
        let remaining = buf.len() - offset;
        let to_copy = if remaining < 8 { remaining } else { 8 };
        buf[offset..offset + to_copy].copy_from_slice(&bytes[..to_copy]);
        offset += to_copy;
    }
}
