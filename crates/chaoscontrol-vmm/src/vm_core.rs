//! Pure VM lifecycle, retention, poison, and teardown plans.
//!
//! This module owns deterministic decisions only. It must not call KVM, read
//! files, inspect clocks, spawn threads, print output, or mutate devices.

// r[impl chaoscontrol.architecture_modules.vmm]
// r[impl chaoscontrol.architecture_modules.boundary]

const BYTES_PER_MIB: usize = 1024 * 1024;
const SERIAL_CAPTURE_MIB: usize = 4;
const RETAINED_CAPACITY_DIVISOR: usize = 2;
/// Maximum bytes retained by one serial capture buffer.
pub(crate) const MAX_SERIAL_CAPTURE_BYTES: usize = SERIAL_CAPTURE_MIB * BYTES_PER_MIB;
const SERIAL_CAPTURE_RETAINED_BYTES: usize = MAX_SERIAL_CAPTURE_BYTES / RETAINED_CAPACITY_DIVISOR;
const SERIAL_SANITIZE_REPLACEMENT: u8 = b'.';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VcpuCountLimit {
    pub(crate) found: usize,
    pub(crate) limit: usize,
}

/// Preserve the public zero-means-one behavior before any KVM effect.
pub(crate) fn plan_vcpu_construction(
    requested: usize,
    limit: usize,
) -> Result<usize, VcpuCountLimit> {
    let planned = requested.max(1);
    if planned > limit {
        return Err(VcpuCountLimit {
            found: planned,
            limit,
        });
    }
    Ok(planned)
}

pub(crate) fn validate_snapshot_count(
    snapshot_count: usize,
    current_count: usize,
) -> Result<(), (usize, usize)> {
    if snapshot_count == current_count {
        Ok(())
    } else {
        Err((snapshot_count, current_count))
    }
}

pub(crate) fn serial_snapshot_fits(input_len: usize, fifo_capacity: usize) -> bool {
    input_len <= fifo_capacity
}

pub(crate) fn hlt_snapshot_is_valid(
    latches: &[bool],
    runnable_vcpus: &[bool],
    current_vcpu_count: usize,
) -> bool {
    let count_matches = latches.len() == current_vcpu_count;
    let no_conflict = latches
        .iter()
        .zip(runnable_vcpus)
        .all(|(latched, runnable)| !*latched || !*runnable);
    count_matches && no_conflict
}

/// Checked mutation plan after exact schedule evidence can no longer be proved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoisonPlan {
    pub(crate) clear_pending_reservation: bool,
    pub(crate) latch_permanent_poison: bool,
}

/// Return the only admitted post-progress poison transition.
pub(crate) fn plan_poison() -> PoisonPlan {
    PoisonPlan {
        clear_pending_reservation: true,
        latch_permanent_poison: true,
    }
}

/// Checked teardown order for VM-owned timer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TeardownPlan {
    pub(crate) disarm_watchdog: bool,
    pub(crate) release_thread_timer: bool,
}

/// Plan teardown from supplied ownership facts.
pub(crate) fn plan_teardown(thread_timer_owned: bool) -> TeardownPlan {
    TeardownPlan {
        disarm_watchdog: true,
        release_thread_timer: thread_timer_owned,
    }
}

/// Append bytes and retain a bounded recent suffix.
pub(crate) fn capture_serial_bounded(buf: &mut Vec<u8>, incoming: &[u8]) -> usize {
    if incoming.len() >= MAX_SERIAL_CAPTURE_BYTES {
        let dropped = buf.len() + (incoming.len() - MAX_SERIAL_CAPTURE_BYTES);
        buf.clear();
        buf.extend_from_slice(&incoming[incoming.len() - MAX_SERIAL_CAPTURE_BYTES..]);
        return dropped;
    }
    let mut dropped = 0;
    if buf.len() + incoming.len() > MAX_SERIAL_CAPTURE_BYTES {
        let room_target = MAX_SERIAL_CAPTURE_BYTES - incoming.len();
        let target = buf
            .len()
            .min(SERIAL_CAPTURE_RETAINED_BYTES)
            .min(room_target);
        dropped = buf.len() - target;
        buf.drain(..dropped);
    }
    buf.extend_from_slice(incoming);
    debug_assert!(buf.len() <= MAX_SERIAL_CAPTURE_BYTES);
    dropped
}

/// Replace terminal control bytes while preserving text bytes and whitespace.
pub(crate) fn sanitize_serial_for_terminal(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|&byte| match byte {
            b'\n' | b'\r' | b'\t' => byte,
            0x00..=0x1F | 0x7F => SERIAL_SANITIZE_REPLACEMENT,
            _ => byte,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_plan_preserves_zero_as_one_and_rejects_excess() {
        const VCPU_LIMIT: usize = 4;
        assert_eq!(plan_vcpu_construction(0, VCPU_LIMIT), Ok(1));
        assert_eq!(
            plan_vcpu_construction(VCPU_LIMIT, VCPU_LIMIT),
            Ok(VCPU_LIMIT)
        );
        assert_eq!(
            plan_vcpu_construction(VCPU_LIMIT + 1, VCPU_LIMIT),
            Err(VcpuCountLimit {
                found: VCPU_LIMIT + 1,
                limit: VCPU_LIMIT,
            })
        );
    }

    #[test]
    fn snapshot_shape_rejects_count_capacity_and_hlt_conflicts() {
        assert!(validate_snapshot_count(2, 2).is_ok());
        assert_eq!(validate_snapshot_count(1, 2), Err((1, 2)));
        assert!(serial_snapshot_fits(1, 1));
        assert!(!serial_snapshot_fits(2, 1));
        assert!(hlt_snapshot_is_valid(&[true, false], &[false, true], 2));
        assert!(!hlt_snapshot_is_valid(&[true], &[true], 1));
    }

    #[test]
    fn poison_plan_is_permanent_and_clears_pending_authority() {
        let plan = plan_poison();
        assert!(plan.clear_pending_reservation);
        assert!(plan.latch_permanent_poison);
    }

    #[test]
    fn teardown_releases_only_owned_timer_state() {
        let without_timer = plan_teardown(false);
        assert!(without_timer.disarm_watchdog);
        assert!(!without_timer.release_thread_timer);

        let with_timer = plan_teardown(true);
        assert!(with_timer.disarm_watchdog);
        assert!(with_timer.release_thread_timer);
    }

    #[test]
    fn invalid_control_bytes_are_sanitized_without_changing_raw_retention() {
        let raw = b"ok\x1b[2J\x7f";
        let mut retained = Vec::new();
        assert_eq!(capture_serial_bounded(&mut retained, raw), 0);
        assert_eq!(retained, raw);
        assert_eq!(sanitize_serial_for_terminal(raw), b"ok.[2J.");
    }
}
