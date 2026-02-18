//! Deterministic entropy source for the ChaosControl hypervisor.
//!
//! Replaces `virtio-rng` with a seeded ChaCha20-based PRNG so that every
//! guest execution with the same seed produces identical random output.
//! Supports snapshot/restore and reseeding for fork/explore workflows.

use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Snapshot of the PRNG state, suitable for serialisation or in-memory cloning.
#[derive(Clone, Debug)]
pub struct EntropySnapshot {
    /// Opaque seed material that can reconstruct the ChaCha20 state.
    ///
    /// We store the 32-byte internal seed plus the stream position so that
    /// restoring yields the exact same future output sequence.
    seed: [u8; 32],
    stream: u64,
    word_pos: u128,
    bytes_generated: u64,
}

/// A seeded, deterministic entropy source backed by ChaCha20.
///
/// # Examples
///
/// ```
/// use chaoscontrol_vmm::devices::entropy::DeterministicEntropy;
///
/// let mut ent = DeterministicEntropy::new(42);
/// let a = ent.next_u64();
///
/// // Same seed ⇒ same output
/// let mut ent2 = DeterministicEntropy::new(42);
/// assert_eq!(a, ent2.next_u64());
/// ```
#[derive(Clone, Debug)]
pub struct DeterministicEntropy {
    rng: ChaCha20Rng,
    /// Total number of bytes dispensed through [`fill_bytes`] and [`next_u64`].
    bytes_generated: u64,
}

impl DeterministicEntropy {
    /// Create a new deterministic entropy source seeded with `seed`.
    ///
    /// The seed is expanded to the 256-bit key required by ChaCha20 by
    /// placing the `u64` in the first 8 bytes of a zeroed 32-byte array.
    pub fn new(seed: u64) -> Self {
        let rng = Self::rng_from_u64(seed);
        Self {
            rng,
            bytes_generated: 0,
        }
    }

    /// Fill `buf` with deterministic pseudo-random bytes.
    pub fn fill_bytes(&mut self, buf: &mut [u8]) {
        self.rng.fill_bytes(buf);
        self.bytes_generated += buf.len() as u64;
    }

    /// Return the next deterministic `u64` value.
    pub fn next_u64(&mut self) -> u64 {
        let v = self.rng.next_u64();
        self.bytes_generated += 8;
        v
    }

    /// Capture a snapshot of the current PRNG state.
    pub fn snapshot(&self) -> EntropySnapshot {
        EntropySnapshot {
            seed: self.rng.get_seed(),
            stream: self.rng.get_stream(),
            word_pos: self.rng.get_word_pos(),
            bytes_generated: self.bytes_generated,
        }
    }

    /// Restore a `DeterministicEntropy` from a previously captured snapshot.
    pub fn restore(snapshot: &EntropySnapshot) -> Self {
        let mut rng = ChaCha20Rng::from_seed(snapshot.seed);
        rng.set_stream(snapshot.stream);
        rng.set_word_pos(snapshot.word_pos);
        Self {
            rng,
            bytes_generated: snapshot.bytes_generated,
        }
    }

    /// Re-seed the PRNG with a new value.
    ///
    /// This resets the internal state and is useful after forking to give
    /// each branch a distinct entropy stream.  The `bytes_generated`
    /// counter is **not** reset so that cumulative statistics are preserved.
    pub fn reseed(&mut self, seed: u64) {
        self.rng = Self::rng_from_u64(seed);
    }

    /// Total number of pseudo-random bytes dispensed so far.
    pub fn bytes_generated(&self) -> u64 {
        self.bytes_generated
    }

    // ── internal helpers ──────────────────────────────────────────────

    fn rng_from_u64(seed: u64) -> ChaCha20Rng {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        ChaCha20Rng::from_seed(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_output() {
        let mut a = DeterministicEntropy::new(1);
        let mut b = DeterministicEntropy::new(1);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn different_seed_different_output() {
        let mut a = DeterministicEntropy::new(1);
        let mut b = DeterministicEntropy::new(2);
        // Overwhelmingly unlikely to collide
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn fill_bytes_deterministic() {
        let mut a = DeterministicEntropy::new(99);
        let mut b = DeterministicEntropy::new(99);
        let mut buf_a = [0u8; 64];
        let mut buf_b = [0u8; 64];
        a.fill_bytes(&mut buf_a);
        b.fill_bytes(&mut buf_b);
        assert_eq!(buf_a, buf_b);
    }

    #[test]
    fn bytes_generated_tracking() {
        let mut e = DeterministicEntropy::new(0);
        assert_eq!(e.bytes_generated(), 0);

        e.next_u64();
        assert_eq!(e.bytes_generated(), 8);

        let mut buf = [0u8; 32];
        e.fill_bytes(&mut buf);
        assert_eq!(e.bytes_generated(), 40);
    }

    #[test]
    fn snapshot_restore_continues_sequence() {
        let mut orig = DeterministicEntropy::new(77);
        // Advance the state a bit
        for _ in 0..10 {
            orig.next_u64();
        }
        let snap = orig.snapshot();

        // Continue the original
        let v1 = orig.next_u64();
        let v2 = orig.next_u64();

        // Restore and expect the same continuation
        let mut restored = DeterministicEntropy::restore(&snap);
        assert_eq!(restored.next_u64(), v1);
        assert_eq!(restored.next_u64(), v2);
        assert_eq!(restored.bytes_generated(), orig.bytes_generated());
    }

    #[test]
    fn reseed_changes_output() {
        let mut a = DeterministicEntropy::new(1);
        let before = a.next_u64();

        a.reseed(2);
        let after = a.next_u64();

        // After reseed the stream should differ from the original seed-1 output
        let mut fresh = DeterministicEntropy::new(1);
        fresh.next_u64(); // skip first
        assert_ne!(fresh.next_u64(), after);

        // But a fresh seed-2 source should match
        let mut fresh2 = DeterministicEntropy::new(2);
        assert_eq!(fresh2.next_u64(), after);

        // bytes_generated is cumulative across reseeds
        assert_eq!(a.bytes_generated(), 16);

        let _ = before; // suppress unused warning
    }

    #[test]
    fn reseed_preserves_bytes_generated() {
        let mut e = DeterministicEntropy::new(0);
        e.next_u64();
        e.next_u64();
        assert_eq!(e.bytes_generated(), 16);

        e.reseed(42);
        assert_eq!(e.bytes_generated(), 16); // not reset
    }
}
