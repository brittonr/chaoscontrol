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
    let latches_compatible = latches
        .iter()
        .zip(runnable_vcpus)
        .all(|(latched, runnable)| !*latched || !*runnable);
    count_matches && latches_compatible
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

const DIRTY_WORD_BITS: usize = u64::BITS as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncrementalPageError {
    BitmapLength { expected: usize, found: usize },
    PageOutOfRange { page: usize, page_count: usize },
}

fn validate_dirty_bitmap(bitmap: &[u64], page_count: usize) -> Result<(), IncrementalPageError> {
    let expected = page_count.div_ceil(DIRTY_WORD_BITS);
    if bitmap.len() != expected {
        return Err(IncrementalPageError::BitmapLength {
            expected,
            found: bitmap.len(),
        });
    }
    let trailing_pages = page_count % DIRTY_WORD_BITS;
    if trailing_pages != 0 {
        let last_word = bitmap[expected - 1];
        let excess = last_word & (u64::MAX << trailing_pages);
        if excess != 0 {
            return Err(IncrementalPageError::PageOutOfRange {
                page: (expected - 1) * DIRTY_WORD_BITS + excess.trailing_zeros() as usize,
                page_count,
            });
        }
    }
    Ok(())
}

// r[impl chaoscontrol.deterministic_snapshots.boundary]
pub(crate) fn merge_dirty_pages(
    current: &mut Vec<u64>,
    incoming: &[u64],
    page_count: usize,
) -> Result<(), IncrementalPageError> {
    validate_dirty_bitmap(incoming, page_count)?;
    if !current.is_empty() {
        validate_dirty_bitmap(current, page_count)?;
    }
    current.resize(incoming.len(), 0);
    for (retained, dirty) in current.iter_mut().zip(incoming) {
        *retained |= dirty;
    }
    Ok(())
}

pub(crate) struct IncrementalPagePlan {
    pub(crate) revert: Vec<u64>,
    pub(crate) next: Vec<u64>,
}

pub(crate) fn overlay_page_bitmap(
    mut retained: Vec<u64>,
    overlay_pages: impl Iterator<Item = usize>,
    page_count: usize,
) -> Result<Vec<u64>, IncrementalPageError> {
    retained.resize(page_count.div_ceil(DIRTY_WORD_BITS), 0);
    retained.fill(0);
    for page in overlay_pages {
        if page >= page_count {
            return Err(IncrementalPageError::PageOutOfRange { page, page_count });
        }
        retained[page / DIRTY_WORD_BITS] |= 1 << (page % DIRTY_WORD_BITS);
    }
    Ok(retained)
}

/// Reuse the retained and KVM-provided bitmaps; no per-page allocation.
// r[impl chaoscontrol.deterministic_snapshots.boundary]
pub(crate) fn plan_incremental_restore(
    current: Vec<u64>,
    mut dirty: Vec<u64>,
    overlay_pages: impl Iterator<Item = usize>,
    page_count: usize,
) -> Result<IncrementalPagePlan, IncrementalPageError> {
    validate_dirty_bitmap(&dirty, page_count)?;
    if !current.is_empty() {
        merge_dirty_pages(&mut dirty, &current, page_count)?;
    }
    let next = overlay_page_bitmap(current, overlay_pages, page_count)?;
    Ok(IncrementalPagePlan {
        revert: dirty,
        next,
    })
}

pub(crate) fn dirty_page_indices(bitmap: &[u64]) -> impl Iterator<Item = usize> + '_ {
    bitmap.iter().enumerate().flat_map(|(index, &word)| {
        let mut remaining = word;
        std::iter::from_fn(move || {
            if remaining == 0 {
                return None;
            }
            let page = index * DIRTY_WORD_BITS + remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            Some(page)
        })
    })
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

    // r[verify chaoscontrol.deterministic_snapshots.validation.core]
    #[test]
    fn incremental_plan_reverts_inherited_and_guest_pages_then_tracks_only_target() {
        let plan =
            plan_incremental_restore(vec![1 << 1, 1], vec![1 << 3, 0], [5, 66].into_iter(), 67)
                .expect("plan valid page sets");
        assert_eq!(
            dirty_page_indices(&plan.revert).collect::<Vec<_>>(),
            [1, 3, 64]
        );
        assert_eq!(dirty_page_indices(&plan.next).collect::<Vec<_>>(), [5, 66]);
    }

    #[test]
    fn invalid_dirty_bitmap_leaves_retained_pages_unchanged() {
        let mut current = vec![1 << 2, 0];
        assert_eq!(
            merge_dirty_pages(&mut current, &[0, 1 << 3], 67),
            Err(IncrementalPageError::PageOutOfRange {
                page: 67,
                page_count: 67
            })
        );
        assert_eq!(current, [1 << 2, 0]);
        assert_eq!(
            merge_dirty_pages(&mut current, &[0], 67),
            Err(IncrementalPageError::BitmapLength {
                expected: 2,
                found: 1
            })
        );
        assert_eq!(current, [1 << 2, 0]);
    }

    #[test]
    fn incremental_plan_rejects_out_of_range_overlay_before_any_shell_effect() {
        assert!(matches!(
            plan_incremental_restore(vec![0, 0], vec![0, 0], [67].into_iter(), 67),
            Err(IncrementalPageError::PageOutOfRange {
                page: 67,
                page_count: 67
            })
        ));
    }

    #[test]
    fn incremental_capture_keeps_pages_across_consumed_dirty_logs() {
        let mut retained = Vec::new();
        merge_dirty_pages(&mut retained, &[1 << 63, 0], 65).expect("initial capture");
        merge_dirty_pages(&mut retained, &[0, 1], 65).expect("child capture");
        merge_dirty_pages(&mut retained, &[0, 0], 65).expect("unchanged child capture");
        assert_eq!(dirty_page_indices(&retained).collect::<Vec<_>>(), [63, 64]);
    }
}
