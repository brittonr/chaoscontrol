//! Pure, verifiable functions for determinism verification.
//!
//! Every function in this module is:
//! - **Pure**: no I/O, no system calls, no side effects beyond the return value.
//! - **Deterministic**: same inputs always produce the same outputs.
//! - **Assertion-guarded**: Tiger Style `debug_assert!` preconditions and
//!   postconditions on every non-trivial function.
//!
//! These properties make the functions suitable for formal verification with
//! [Verus](https://github.com/verus-lang/verus).  The corresponding spec
//! file is `verus/events_spec.rs`.
//!
//! # Mapping to `verifier.rs`
//!
//! | Verified function            | Original location in `verifier.rs`        |
//! |------------------------------|-------------------------------------------|
//! | [`describe_divergence`]      | `describe_divergence()` (free function)   |
//! | [`find_first_divergence`]    | `DeterminismVerifier::compare_events()`   |

use crate::events::{EventKind, TraceEvent};

// ─── Divergence description ─────────────────────────────────────────

/// Produce a human-readable description of how two events differ.
///
/// This is the pure core of the verifier's divergence reporting.
/// It inspects the event types and data fields to produce a specific
/// message (e.g. "PIO data differs on port 0x3f8").
///
/// # Properties
///
/// - Always returns a non-empty string.
/// - If the event types differ, the message mentions "type mismatch".
/// - If the types match but data differs, the message is type-specific.
pub fn describe_divergence(a: &TraceEvent, b: &TraceEvent) -> String {
    let type_a = a.event_type();
    let type_b = b.event_type();

    if type_a != type_b {
        let result = format!(
            "Event type mismatch: A={}, B={}",
            type_a.name(),
            type_b.name()
        );
        debug_assert!(
            !result.is_empty(),
            "describe_divergence must return non-empty string"
        );
        return result;
    }

    let result = match (&a.kind, &b.kind) {
        (
            EventKind::KvmExit {
                reason: r1,
                guest_rip: rip1,
                ..
            },
            EventKind::KvmExit {
                reason: r2,
                guest_rip: rip2,
                ..
            },
        ) => {
            if r1 != r2 {
                format!("Exit reason differs: A={}, B={}", r1, r2)
            } else {
                format!("Guest RIP differs: A={:#x}, B={:#x}", rip1, rip2)
            }
        }
        (
            EventKind::KvmPio {
                port: p1,
                val: v1,
                direction: d1,
                ..
            },
            EventKind::KvmPio {
                port: p2,
                val: v2,
                direction: d2,
                ..
            },
        ) => {
            if p1 != p2 {
                format!("PIO port differs: A={:#x}, B={:#x}", p1, p2)
            } else if d1 != d2 {
                format!("PIO direction differs: A={}, B={}", d1, d2)
            } else {
                format!(
                    "PIO data differs on port {:#x}: A={:#x}, B={:#x}",
                    p1, v1, v2
                )
            }
        }
        (
            EventKind::KvmInjVirq { vector: v1, .. },
            EventKind::KvmInjVirq { vector: v2, .. },
        ) => {
            format!("Injected vector differs: A={}, B={}", v1, v2)
        }
        (
            EventKind::KvmSetIrq { gsi: g1, level: l1 },
            EventKind::KvmSetIrq { gsi: g2, level: l2 },
        ) => {
            if g1 != g2 {
                format!("IRQ GSI differs: A={}, B={}", g1, g2)
            } else {
                format!("IRQ level differs on GSI {}: A={}, B={}", g1, l1, l2)
            }
        }
        (
            EventKind::KvmMsr {
                index: i1,
                data: d1,
                ..
            },
            EventKind::KvmMsr {
                index: i2,
                data: d2,
                ..
            },
        ) => {
            if i1 != i2 {
                format!("MSR index differs: A={:#x}, B={:#x}", i1, i2)
            } else {
                format!(
                    "MSR data differs for {:#x}: A={:#x}, B={:#x}",
                    i1, d1, d2
                )
            }
        }
        _ => format!("Events differ (same type: {})", type_a.name()),
    };

    // Postcondition: result is non-empty.
    debug_assert!(
        !result.is_empty(),
        "describe_divergence must return non-empty string"
    );

    result
}

// ─── First divergence finder ────────────────────────────────────────

/// Find the first index at which two event slices diverge.
///
/// Compares event-by-event using [`TraceEvent::determinism_eq`], ignoring
/// host timestamps and sequence numbers.  Returns `None` if the slices
/// are deterministically equivalent (same length, same data).
///
/// When a divergence is found, returns `(index, description)` where
/// `description` explains how the events differ (via [`describe_divergence`]).
///
/// If the slices match element-wise but have different lengths, the
/// divergence index is `min(a.len(), b.len())` and the description
/// mentions the length mismatch.
///
/// # Properties
///
/// - Returns `None` iff the slices are identical (per `determinism_eq`).
/// - `index <= min(a.len(), b.len())`.
/// - All events before `index` are deterministically equal.
pub fn find_first_divergence(
    events_a: &[TraceEvent],
    events_b: &[TraceEvent],
) -> Option<(usize, String)> {
    let min_len = events_a.len().min(events_b.len());

    for i in 0..min_len {
        if !events_a[i].determinism_eq(&events_b[i]) {
            let description = describe_divergence(&events_a[i], &events_b[i]);

            // Postcondition: index is valid.
            debug_assert!(i < events_a.len() && i < events_b.len());
            // Postcondition: all prior events are equal.
            debug_assert!(
                (0..i).all(|j| events_a[j].determinism_eq(&events_b[j])),
                "all events before divergence must be equal"
            );

            return Some((i, description));
        }
    }

    // Check length mismatch.
    if events_a.len() != events_b.len() {
        let description = format!(
            "Trace length mismatch: A has {} events, B has {} events (delta: {})",
            events_a.len(),
            events_b.len(),
            events_a.len() as i64 - events_b.len() as i64,
        );

        // Postcondition: all min_len events matched.
        debug_assert!(
            (0..min_len).all(|j| events_a[j].determinism_eq(&events_b[j])),
            "all events in the common prefix must be equal"
        );

        return Some((min_len, description));
    }

    // Postcondition: slices are identical.
    debug_assert_eq!(events_a.len(), events_b.len());
    debug_assert!(
        (0..events_a.len()).all(|j| events_a[j].determinism_eq(&events_b[j])),
        "None result requires all events to be equal"
    );

    None
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventKind, IoDirection, TraceEvent};

    fn make_event(kind: EventKind) -> TraceEvent {
        TraceEvent {
            seq: 0,
            host_ns: 0,
            pid: 1,
            kind,
        }
    }

    fn make_exit(reason: u32, rip: u64) -> TraceEvent {
        make_event(EventKind::KvmExit {
            reason,
            guest_rip: rip,
            info1: 0,
            info2: 0,
        })
    }

    fn make_pio(dir: IoDirection, port: u16, val: u32) -> TraceEvent {
        make_event(EventKind::KvmPio {
            direction: dir,
            port,
            size: 1,
            val,
        })
    }

    // ── describe_divergence ──────────────────────────────────────

    #[test]
    fn describe_type_mismatch() {
        let a = make_exit(12, 0x1000);
        let b = make_pio(IoDirection::Write, 0x3f8, 0x48);
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("type mismatch"),
            "should mention type mismatch: got {desc:?}"
        );
        assert!(desc.contains("kvm_exit"), "should mention A type");
        assert!(desc.contains("kvm_pio"), "should mention B type");
    }

    #[test]
    fn describe_exit_reason_differs() {
        let a = make_exit(12, 0x1000);
        let b = make_exit(7, 0x1000);
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("Exit reason differs"),
            "should describe exit reason diff: got {desc:?}"
        );
    }

    #[test]
    fn describe_exit_rip_differs() {
        let a = make_exit(12, 0x1000);
        let b = make_exit(12, 0x2000);
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("Guest RIP differs"),
            "should describe RIP diff: got {desc:?}"
        );
    }

    #[test]
    fn describe_pio_data_differs() {
        let a = make_pio(IoDirection::Write, 0x3f8, 0x48);
        let b = make_pio(IoDirection::Write, 0x3f8, 0x49);
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("PIO data differs"),
            "should describe PIO data diff: got {desc:?}"
        );
        assert!(desc.contains("0x3f8"), "should mention port");
    }

    #[test]
    fn describe_pio_port_differs() {
        let a = make_pio(IoDirection::Write, 0x3f8, 0x48);
        let b = make_pio(IoDirection::Write, 0x3f9, 0x48);
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("PIO port differs"),
            "should describe port diff: got {desc:?}"
        );
    }

    #[test]
    fn describe_pio_direction_differs() {
        let a = make_pio(IoDirection::Read, 0x3f8, 0x48);
        let b = make_pio(IoDirection::Write, 0x3f8, 0x48);
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("PIO direction differs"),
            "should describe direction diff: got {desc:?}"
        );
    }

    #[test]
    fn describe_msr_index_differs() {
        let a = make_event(EventKind::KvmMsr {
            direction: IoDirection::Write,
            index: 0x174,
            data: 0,
        });
        let b = make_event(EventKind::KvmMsr {
            direction: IoDirection::Write,
            index: 0x175,
            data: 0,
        });
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("MSR index differs"),
            "should describe MSR index diff: got {desc:?}"
        );
    }

    #[test]
    fn describe_msr_data_differs() {
        let a = make_event(EventKind::KvmMsr {
            direction: IoDirection::Write,
            index: 0x174,
            data: 1,
        });
        let b = make_event(EventKind::KvmMsr {
            direction: IoDirection::Write,
            index: 0x174,
            data: 2,
        });
        let desc = describe_divergence(&a, &b);
        assert!(
            desc.contains("MSR data differs"),
            "should describe MSR data diff: got {desc:?}"
        );
    }

    #[test]
    fn describe_always_nonempty() {
        let a = make_exit(12, 0x1000);
        let b = make_exit(12, 0x2000);
        assert!(!describe_divergence(&a, &b).is_empty());

        let c = make_pio(IoDirection::Write, 0x3f8, 0x48);
        assert!(!describe_divergence(&a, &c).is_empty());
    }

    // ── find_first_divergence ────────────────────────────────────

    #[test]
    fn identical_slices_return_none() {
        let events = vec![
            make_exit(12, 0x1000),
            make_pio(IoDirection::Write, 0x3f8, 0x48),
            make_exit(12, 0x2000),
        ];
        assert!(
            find_first_divergence(&events, &events).is_none(),
            "identical slices must return None"
        );
    }

    #[test]
    fn empty_slices_return_none() {
        assert!(
            find_first_divergence(&[], &[]).is_none(),
            "two empty slices must return None"
        );
    }

    #[test]
    fn first_event_differs() {
        let a = vec![make_exit(12, 0x1000)];
        let b = vec![make_exit(7, 0x1000)];
        let (idx, desc) = find_first_divergence(&a, &b).unwrap();
        assert_eq!(idx, 0);
        assert!(desc.contains("Exit reason differs"));
    }

    #[test]
    fn divergence_at_middle() {
        let a = vec![
            make_exit(12, 0x1000),
            make_pio(IoDirection::Write, 0x3f8, 0x48),
            make_exit(12, 0x2000),
        ];
        let b = vec![
            make_exit(12, 0x1000),
            make_pio(IoDirection::Write, 0x3f8, 0x49), // different val
            make_exit(12, 0x2000),
        ];
        let (idx, desc) = find_first_divergence(&a, &b).unwrap();
        assert_eq!(idx, 1);
        assert!(desc.contains("PIO data differs"));
    }

    #[test]
    fn length_mismatch_a_longer() {
        let a = vec![
            make_exit(12, 0x1000),
            make_exit(12, 0x2000),
        ];
        let b = vec![make_exit(12, 0x1000)];
        let (idx, desc) = find_first_divergence(&a, &b).unwrap();
        assert_eq!(idx, 1, "divergence should be at the end of shorter slice");
        assert!(
            desc.contains("length mismatch"),
            "should mention length mismatch: got {desc:?}"
        );
    }

    #[test]
    fn length_mismatch_b_longer() {
        let a = vec![make_exit(12, 0x1000)];
        let b = vec![
            make_exit(12, 0x1000),
            make_exit(12, 0x2000),
        ];
        let (idx, desc) = find_first_divergence(&a, &b).unwrap();
        assert_eq!(idx, 1);
        assert!(desc.contains("length mismatch"));
    }

    #[test]
    fn host_timestamps_ignored() {
        let a = vec![TraceEvent {
            seq: 1,
            host_ns: 100_000,
            pid: 1,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1000,
                info1: 0,
                info2: 0,
            },
        }];
        let b = vec![TraceEvent {
            seq: 999,
            host_ns: 999_999_999,
            pid: 1,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1000,
                info1: 0,
                info2: 0,
            },
        }];
        assert!(
            find_first_divergence(&a, &b).is_none(),
            "host timestamps should be ignored"
        );
    }

    #[test]
    fn type_mismatch_at_first_event() {
        let a = vec![make_exit(12, 0x1000)];
        let b = vec![make_pio(IoDirection::Write, 0x3f8, 0x48)];
        let (idx, desc) = find_first_divergence(&a, &b).unwrap();
        assert_eq!(idx, 0);
        assert!(desc.contains("type mismatch"));
    }

    #[test]
    fn divergence_index_never_exceeds_min_len() {
        let a = vec![make_exit(12, 0x1000), make_exit(7, 0x2000)];
        let b = vec![make_exit(12, 0x1000)];
        let (idx, _) = find_first_divergence(&a, &b).unwrap();
        assert!(idx <= a.len().min(b.len()), "index must be <= min_len");
    }
}
