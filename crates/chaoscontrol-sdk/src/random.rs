//! Guided randomness — the VMM controls these values for exploration.
//!
//! Instead of using `rand` or hardware RNG, guest code should use these
//! functions for any random decisions that affect test behavior.  The VMM
//! can then vary these values across runs to explore different execution
//! paths, and replay them exactly for reproduction.
//!
//! When running outside a ChaosControl VM, these functions fall back to
//! the host's entropy source (similar to Antithesis's fallback behavior).
//!
//! # `rand` crate integration
//!
//! With the `full` feature (default), [`ChaosControlRng`] implements
//! `rand::RngCore`, allowing use with the entire `rand` ecosystem:
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::random::ChaosControlRng;
//! use rand::Rng;
//!
//! let mut rng = ChaosControlRng;
//! let value: u32 = rng.gen();
//! let items = vec!["a", "b", "c"];
//! let chosen = items.choose(&mut rng);
//! ```

use crate::transport;
use chaoscontrol_protocol::*;

// ═══════════════════════════════════════════════════════════════════════
//  Core random functions
// ═══════════════════════════════════════════════════════════════════════

/// Get a guided random `u64` from the VMM.
///
/// The VMM returns a value from its deterministic PRNG.  Same seed
/// + same execution path = same value.  Different seeds explore
///   different choices.
///
/// Outside a VM, falls back to host entropy.
///
/// # Important
///
/// Do **not** use this to seed a PRNG.  Call it at each decision
/// point for maximum exploration power.  See the
/// [Antithesis random docs](https://antithesis.com/docs/generated/sdk/rust/antithesis_sdk/random/)
/// for rationale.
///
/// # Example
///
/// ```rust,ignore
/// let delay_ms = chaoscontrol_sdk::random::get_random() % 1000;
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

// ═══════════════════════════════════════════════════════════════════════
//  rand::RngCore integration (requires `full` feature)
// ═══════════════════════════════════════════════════════════════════════

/// A random number generator backed by ChaosControl's guided randomness.
///
/// Implements `rand::RngCore` so it can be used anywhere the `rand`
/// ecosystem expects an RNG — `rng.gen()`, `slice.choose(&mut rng)`,
/// `slice.shuffle(&mut rng)`, etc.
///
/// Inside a ChaosControl VM, every call goes through the VMM's
/// deterministic PRNG.  Outside a VM, falls back to host entropy.
///
/// # Example
///
/// ```rust,ignore
/// use chaoscontrol_sdk::random::ChaosControlRng;
/// use rand::{Rng, seq::SliceRandom};
///
/// let mut rng = ChaosControlRng;
/// let x: f64 = rng.gen();
/// let items = vec![1, 2, 3, 4, 5];
/// let picked = items.choose(&mut rng).unwrap();
/// ```
#[cfg(feature = "full")]
#[derive(Debug, Clone, Copy, Default)]
pub struct ChaosControlRng;

#[cfg(feature = "full")]
impl ChaosControlRng {
    /// Create a new `ChaosControlRng`.
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(feature = "full")]
impl rand::RngCore for ChaosControlRng {
    fn next_u32(&mut self) -> u32 {
        (get_random() & 0xFFFF_FFFF) as u32
    }

    fn next_u64(&mut self) -> u64 {
        get_random()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        crate::random::fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        crate::random::fill_bytes(dest);
        Ok(())
    }
}

/// Mark `ChaosControlRng` as cryptographically secure for the `rand`
/// ecosystem.  The VMM's ChaCha20 PRNG is cryptographically strong
/// (though the VMM of course knows all values).
#[cfg(feature = "full")]
impl rand::CryptoRng for ChaosControlRng {}

// ═══════════════════════════════════════════════════════════════════════
//  Slice-based random choice (requires `full` feature)
// ═══════════════════════════════════════════════════════════════════════

/// Choose a random element from a slice using guided randomness.
///
/// Returns `Some(&T)` for a uniformly random element, or `None` if
/// the slice is empty.  This matches Antithesis's `random_choice`
/// signature.
///
/// # Example
///
/// ```rust,ignore
/// let actions = ["read", "write", "delete"];
/// if let Some(&action) = chaoscontrol_sdk::random::random_choice_from(&actions) {
///     execute(action);
/// }
/// ```
#[cfg(feature = "full")]
pub fn random_choice_from<T>(choices: &[T]) -> Option<&T> {
    match choices {
        [] => None,
        [x] => Some(x),
        _ => {
            let idx = random_choice(choices.len());
            Some(&choices[idx])
        }
    }
}
