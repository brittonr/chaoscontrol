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
//! With the `rand_core_v0_6` feature (default via `rand_v0_8`), [`ChaosControlRng`] implements
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
/// Returns a value in `0..n`. `n == 0` is rejected by a debug assertion;
/// `n <= 1` returns `0` without a hypercall.
///
/// # Example
///
/// ```rust,ignore
/// let actions = ["read", "write", "delete"];
/// let idx = chaoscontrol_sdk::random::random_choice_index(actions.len());
/// execute(actions[idx]);
/// ```
pub fn random_choice_index(n: usize) -> usize {
    debug_assert!(n > 0, "random_choice_index: n must be > 0");
    if n <= 1 {
        return 0;
    }
    let (result, _status) = transport::hypercall_simple(CMD_RANDOM_CHOICE, n as u32);
    (result as usize) % n
}

/// Fill a byte slice with guided random bytes from the VMM.
///
/// Issues one `get_random` call per eight bytes and writes each value in
/// native byte order, matching Antithesis.
pub fn fill_bytes(buf: &mut [u8]) {
    fill_bytes_with(buf, get_random);
}

fn fill_bytes_with(buf: &mut [u8], mut next: impl FnMut() -> u64) {
    let (chunks, remainder) = buf.as_chunks_mut::<8>();
    for chunk in chunks {
        *chunk = next().to_ne_bytes();
    }
    if !remainder.is_empty() {
        let bytes = next().to_ne_bytes();
        remainder.copy_from_slice(&bytes[..remainder.len()]);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  rand::RngCore integration (requires `rand_core_v0_6` feature)
// ═══════════════════════════════════════════════════════════════════════

/// A random number generator backed by ChaosControl's guided randomness.
///
/// The RNG implements the `rand` 0.8 `RngCore` (with the `full` feature)
/// and the `rand_core` 0.9 / 0.10 traits behind the `rand_v0_9` /
/// `rand_v0_10` features. Inside a ChaosControl VM, every call goes
/// through the VMM's deterministic PRNG. Outside a VM, falls back to host
/// entropy.
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
#[derive(Debug, Clone, Copy, Default)]
pub struct ChaosControlRng;

impl ChaosControlRng {
    /// Create a new `ChaosControlRng`.
    pub const fn new() -> Self {
        Self
    }
}

/// Antithesis-compatible name for [`ChaosControlRng`].
pub type AntithesisRng = ChaosControlRng;

#[cfg(feature = "rand_core_v0_6")]
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
#[cfg(feature = "rand_core_v0_6")]
impl rand::CryptoRng for ChaosControlRng {}

#[cfg(feature = "rand_core_v0_9")]
impl rand_core_v0_9::RngCore for ChaosControlRng {
    fn next_u32(&mut self) -> u32 {
        (get_random() & 0xFFFF_FFFF) as u32
    }

    fn next_u64(&mut self) -> u64 {
        get_random()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        crate::random::fill_bytes(dest);
    }
}

#[cfg(feature = "rand_core_v0_10")]
impl rand_core_v0_10::TryRng for ChaosControlRng {
    type Error = core::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok((get_random() & 0xFFFF_FFFF) as u32)
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(get_random())
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        crate::random::fill_bytes(dest);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Slice-based random choice
// ═══════════════════════════════════════════════════════════════════════

/// Choose a random element from a slice using guided randomness.
///
/// Returns `Some(&T)` for a random element, or `None` if the slice is
/// empty. This matches Antithesis's `random_choice` signature.
///
/// # Example
///
/// ```rust,ignore
/// let actions = ["read", "write", "delete"];
/// if let Some(&action) = chaoscontrol_sdk::random::random_choice(&actions) {
///     execute(action);
/// }
/// ```
pub fn random_choice<T>(choices: &[T]) -> Option<&T> {
    match choices {
        [] => None,
        [x] => Some(x),
        _ => {
            let idx = unbiased_index(choices.len(), get_random);
            Some(&choices[idx])
        }
    }
}

/// Map random values to `0..len` without modulo bias.
fn unbiased_index(len: usize, mut next: impl FnMut() -> u64) -> usize {
    debug_assert!(len > 1);
    let len = len as u64;
    let ceiling = (u64::MAX / len) * len;
    let mut random = next();
    while random >= ceiling {
        random = next();
    }
    (random % len) as usize
}

#[cfg(test)]
mod tests {
    use super::fill_bytes_with;

    #[test]
    fn fill_bytes_uses_native_endian_chunks() {
        let mut calls = 0;
        let mut buffer = [0_u8; 11];
        fill_bytes_with(&mut buffer, || {
            calls += 1;
            if calls == 1 {
                0x0102_0304_0506_0708
            } else {
                0x1112_1314_1516_1718
            }
        });

        assert_eq!(&buffer[..8], &0x0102_0304_0506_0708_u64.to_ne_bytes());
        assert_eq!(&buffer[8..], &0x1112_1314_1516_1718_u64.to_ne_bytes()[..3]);
        assert_eq!(calls, 2);
    }

    #[test]
    fn unbiased_index_rejects_the_biased_tail() {
        let mut values = [u64::MAX, 0].into_iter();
        assert_eq!(super::unbiased_index(3, || values.next().unwrap()), 0);
        assert_eq!(super::unbiased_index(4, || 6), 2);
    }
}
