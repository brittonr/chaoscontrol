//! Pure, verifiable functions for block device bounds checking and fault matching.
//!
//! Every function in this module is:
//! - **Pure**: no I/O, no system calls, no side effects beyond the return value.
//! - **Deterministic**: same inputs always produce the same outputs.
//! - **Assertion-guarded**: Tiger Style `debug_assert!` preconditions and
//!   postconditions on every non-trivial function.
//!
//! These properties make the functions suitable for formal verification with
//! [Verus](https://github.com/verus-lang/verus).  The corresponding spec
//! file is `verus/block_spec.rs`.
//!
//! # Mapping to `devices/block.rs`
//!
//! | Verified function            | Original location in `devices/block.rs`    |
//! |------------------------------|--------------------------------------------|
//! | [`check_bounds`]             | `DeterministicBlock::check_bounds()`       |
//! | [`find_matching_fault`]      | `DeterministicBlock::find_fault()`         |

use std::collections::VecDeque;

use crate::devices::block::{BlockError, BlockFault};

// ─── Bounds checking ────────────────────────────────────────────────

/// Check whether a byte range `[offset, offset+len)` fits within a
/// device of `device_size` bytes.
///
/// Returns `Ok(())` if the range is valid, or `Err(BlockError::OutOfBounds)`
/// if it exceeds the device boundary.  Uses saturating addition to avoid
/// overflow when `offset + len` would exceed `u64::MAX`.
///
/// # Properties verified by `verus/block_spec.rs`
///
/// - `offset + len <= device_size  ⟹  Ok(())`
/// - `offset.saturating_add(len) > device_size  ⟹  Err(OutOfBounds)`
/// - The function is total: it never panics for any input combination.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(check_bounds(1024, 0, 512).is_ok());
/// assert!(check_bounds(1024, 1020, 8).is_err());
/// ```
pub fn check_bounds(device_size: u64, offset: u64, len: u64) -> Result<(), BlockError> {
    if offset.saturating_add(len) > device_size {
        let result = Err(BlockError::OutOfBounds {
            offset,
            len,
            device_size,
        });

        // Postcondition: error carries the correct parameters.
        debug_assert!(matches!(
            &result,
            Err(BlockError::OutOfBounds {
                offset: o,
                len: l,
                device_size: d,
            }) if *o == offset && *l == len && *d == device_size
        ));

        return result;
    }

    // Postcondition: the range is within bounds (per saturating arithmetic).
    debug_assert!(
        offset.saturating_add(len) <= device_size,
        "check_bounds: Ok result requires range to be in bounds"
    );

    Ok(())
}

// ─── Fault matching ─────────────────────────────────────────────────

/// Find the index of the first fault in `faults` that matches `predicate`.
///
/// This is a pure, linear search over the queue.  The caller consumes
/// the matched fault by removing it at the returned index.
///
/// # Properties
///
/// - Returns `None` if no fault matches.
/// - Returns `Some(i)` where `i < faults.len()` and `predicate(&faults[i])`.
/// - No faults before index `i` match the predicate.
pub fn find_matching_fault(
    faults: &VecDeque<BlockFault>,
    predicate: impl Fn(&BlockFault) -> bool,
) -> Option<usize> {
    let result = faults.iter().position(&predicate);

    // Postcondition: if Some(i), then i is a valid index.
    debug_assert!(
        result.is_none() || result.unwrap() < faults.len(),
        "find_matching_fault: returned index must be valid"
    );
    // Postcondition: if Some(i), the fault at i matches the predicate.
    debug_assert!(
        result.is_none() || predicate(&faults[result.unwrap()]),
        "find_matching_fault: fault at returned index must match predicate"
    );
    // Postcondition: no earlier fault matches.
    debug_assert!(
        result.is_none() || (0..result.unwrap()).all(|j| !predicate(&faults[j])),
        "find_matching_fault: no earlier fault should match"
    );

    result
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::block::{BlockError, BlockFault};

    // ── check_bounds ─────────────────────────────────────────────

    #[test]
    fn bounds_ok_exact_fit() {
        // Range fills the entire device.
        assert!(
            check_bounds(1024, 0, 1024).is_ok(),
            "exact fit must succeed"
        );
    }

    #[test]
    fn bounds_ok_zero_len() {
        // Zero-length range at any valid offset.
        assert!(check_bounds(1024, 0, 0).is_ok());
        assert!(check_bounds(1024, 1024, 0).is_ok());
        assert!(check_bounds(0, 0, 0).is_ok());
    }

    #[test]
    fn bounds_ok_partial_range() {
        assert!(check_bounds(1024, 512, 512).is_ok());
        assert!(check_bounds(1024, 0, 1).is_ok());
        assert!(check_bounds(1024, 1023, 1).is_ok());
    }

    #[test]
    fn bounds_err_one_past_end() {
        let err = check_bounds(1024, 0, 1025).unwrap_err();
        assert!(matches!(
            err,
            BlockError::OutOfBounds {
                offset: 0,
                len: 1025,
                device_size: 1024
            }
        ));
    }

    #[test]
    fn bounds_err_offset_past_end() {
        let err = check_bounds(1024, 1024, 1).unwrap_err();
        assert!(matches!(err, BlockError::OutOfBounds { .. }));
    }

    #[test]
    fn bounds_err_overflow_saturates() {
        // offset + len would overflow u64, but saturating_add clamps to MAX.
        let err = check_bounds(1024, u64::MAX, 1).unwrap_err();
        assert!(
            matches!(err, BlockError::OutOfBounds { .. }),
            "overflow case must be caught"
        );
    }

    #[test]
    fn bounds_saturating_clamp_at_max() {
        // When offset + len overflows, saturating_add clamps to u64::MAX.
        // If device_size == u64::MAX, the clamped value equals device_size,
        // so the function returns Ok (conservative behaviour).
        assert!(
            check_bounds(u64::MAX, u64::MAX, u64::MAX).is_ok(),
            "saturating_add(MAX, MAX) == MAX <= MAX, so Ok"
        );
        // But for a smaller device, the overflow is caught.
        assert!(
            check_bounds(u64::MAX - 1, u64::MAX, u64::MAX).is_err(),
            "saturating_add(MAX, MAX) == MAX > MAX-1, so Err"
        );
    }

    #[test]
    fn bounds_ok_max_device() {
        // Largest possible valid range.
        assert!(check_bounds(u64::MAX, 0, u64::MAX).is_ok());
    }

    #[test]
    fn bounds_err_carries_params() {
        let err = check_bounds(100, 90, 20).unwrap_err();
        match err {
            BlockError::OutOfBounds {
                offset,
                len,
                device_size,
            } => {
                assert_eq!(offset, 90, "error must carry the offset");
                assert_eq!(len, 20, "error must carry the len");
                assert_eq!(device_size, 100, "error must carry the device_size");
            }
            _ => panic!("expected OutOfBounds"),
        }
    }

    #[test]
    fn bounds_ok_zero_device() {
        // Zero-size device with zero-length range.
        assert!(check_bounds(0, 0, 0).is_ok());
        // But any nonzero range fails.
        assert!(check_bounds(0, 0, 1).is_err());
    }

    // ── find_matching_fault ──────────────────────────────────────

    #[test]
    fn find_fault_empty_queue() {
        let faults: VecDeque<BlockFault> = VecDeque::new();
        assert!(
            find_matching_fault(&faults, |_| true).is_none(),
            "empty queue must return None"
        );
    }

    #[test]
    fn find_fault_no_match() {
        let mut faults = VecDeque::new();
        faults.push_back(BlockFault::ReadError { offset: 0 });
        faults.push_back(BlockFault::WriteError { offset: 512 });

        assert!(
            find_matching_fault(&faults, |f| {
                matches!(f, BlockFault::ReadError { offset: 1024 })
            })
            .is_none(),
            "no matching fault must return None"
        );
    }

    #[test]
    fn find_fault_first_match() {
        let mut faults = VecDeque::new();
        faults.push_back(BlockFault::ReadError { offset: 0 });
        faults.push_back(BlockFault::ReadError { offset: 512 });
        faults.push_back(BlockFault::ReadError { offset: 1024 });

        let idx = find_matching_fault(&faults, |f| {
            matches!(f, BlockFault::ReadError { offset: 512 })
        });
        assert_eq!(idx, Some(1), "must find the first matching fault");
    }

    #[test]
    fn find_fault_returns_earliest() {
        let mut faults = VecDeque::new();
        faults.push_back(BlockFault::ReadError { offset: 0 });
        faults.push_back(BlockFault::ReadError { offset: 0 });
        faults.push_back(BlockFault::ReadError { offset: 0 });

        let idx = find_matching_fault(&faults, |f| {
            matches!(f, BlockFault::ReadError { offset: 0 })
        });
        assert_eq!(idx, Some(0), "must return the earliest matching index");
    }

    #[test]
    fn find_fault_distinguishes_types() {
        let mut faults = VecDeque::new();
        faults.push_back(BlockFault::WriteError { offset: 0 });
        faults.push_back(BlockFault::ReadError { offset: 0 });

        // Looking for ReadError at offset 0 — should skip the WriteError.
        let idx = find_matching_fault(&faults, |f| {
            matches!(f, BlockFault::ReadError { offset: 0 })
        });
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn find_fault_torn_write() {
        let mut faults = VecDeque::new();
        faults.push_back(BlockFault::TornWrite {
            offset: 0,
            bytes_written: 4,
        });

        let idx = find_matching_fault(&faults, |f| {
            matches!(f, BlockFault::TornWrite { offset: 0, .. })
        });
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn find_fault_corruption() {
        let mut faults = VecDeque::new();
        faults.push_back(BlockFault::Corruption {
            offset: 256,
            len: 16,
        });

        let idx = find_matching_fault(&faults, |f| {
            matches!(f, BlockFault::Corruption { offset: 256, .. })
        });
        assert_eq!(idx, Some(0));
    }
}
