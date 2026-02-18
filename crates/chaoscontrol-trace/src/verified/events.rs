//! Pure, verifiable functions for trace event parsing and comparison.
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
//! # Mapping to `events.rs`
//!
//! | Verified function          | Original location in `events.rs`           |
//! |----------------------------|--------------------------------------------|
//! | [`event_type_from_u32`]    | `EventType::from_u32()`                    |
//! | [`event_type_name`]        | `EventType::name()`                        |
//! | [`exit_reason_name`]       | `exit_reason_name()` (free function)       |
//! | [`parse_event_kind`]       | `TraceEvent::from_raw()` (kind extraction) |
//! | [`event_kind_eq`]          | `PartialEq for EventKind`                  |

use crate::events::{EventKind, EventType, IoDirection};

// ─── EventType conversion ───────────────────────────────────────────

/// Convert a `u32` discriminant to an [`EventType`], if valid.
///
/// Valid discriminants are 1..=11 (matching the BPF `EVENT_*` constants).
/// Returns `None` for 0 or any value > 11.
///
/// # Properties verified by `verus/events_spec.rs`
///
/// - Round-trip: `event_type_from_u32(et as u32) == Some(et)` for all variants
/// - Rejects invalid: `event_type_from_u32(0).is_none()`
/// - Rejects invalid: `event_type_from_u32(v).is_none()` for `v > 11`
///
/// # Examples
///
/// ```rust,ignore
/// assert_eq!(event_type_from_u32(1), Some(EventType::KvmExit));
/// assert_eq!(event_type_from_u32(0), None);
/// assert_eq!(event_type_from_u32(12), None);
/// ```
pub fn event_type_from_u32(v: u32) -> Option<EventType> {
    let result = match v {
        1 => Some(EventType::KvmExit),
        2 => Some(EventType::KvmEntry),
        3 => Some(EventType::KvmPio),
        4 => Some(EventType::KvmMmio),
        5 => Some(EventType::KvmMsr),
        6 => Some(EventType::KvmInjVirq),
        7 => Some(EventType::KvmPicIrq),
        8 => Some(EventType::KvmSetIrq),
        9 => Some(EventType::KvmPageFault),
        10 => Some(EventType::KvmCr),
        11 => Some(EventType::KvmCpuid),
        _ => None,
    };

    // Postcondition: if Some, the discriminant matches the input.
    debug_assert!(
        result.is_none() || result.unwrap() as u32 == v,
        "event_type_from_u32: round-trip violation for v={v}"
    );

    result
}

/// Return the human-readable tracepoint name for an [`EventType`].
///
/// These names match the kernel KVM tracepoint names (e.g. `kvm_exit`).
///
/// # Properties verified by `verus/events_spec.rs`
///
/// - The returned string is non-empty for all variants.
/// - Each variant maps to a distinct string.
pub fn event_type_name(et: &EventType) -> &'static str {
    let result = match et {
        EventType::KvmExit => "kvm_exit",
        EventType::KvmEntry => "kvm_entry",
        EventType::KvmPio => "kvm_pio",
        EventType::KvmMmio => "kvm_mmio",
        EventType::KvmMsr => "kvm_msr",
        EventType::KvmInjVirq => "kvm_inj_virq",
        EventType::KvmPicIrq => "kvm_pic_set_irq",
        EventType::KvmSetIrq => "kvm_set_irq",
        EventType::KvmPageFault => "kvm_page_fault",
        EventType::KvmCr => "kvm_cr",
        EventType::KvmCpuid => "kvm_cpuid",
    };

    // Postcondition: name is non-empty.
    debug_assert!(!result.is_empty(), "event_type_name returned empty string");

    result
}

// ─── Exit reason name ───────────────────────────────────────────────

/// Map a KVM exit reason code (x86) to a human-readable name.
///
/// Returns `"UNKNOWN"` for unrecognised reason codes.  This is a pure
/// lookup table with no side effects.
///
/// # Properties verified by `verus/events_spec.rs`
///
/// - The returned string is non-empty for all inputs.
/// - Known Intel VMX reasons (0, 1, 2, 7, 10, 12, 18, 30–33, 48, 49)
///   return a name other than `"UNKNOWN"`.
pub fn exit_reason_name(reason: u32) -> &'static str {
    let result = match reason {
        0 => "EXCEPTION_NMI",
        1 => "EXTERNAL_INTERRUPT",
        2 => "IO_INSTRUCTION",
        7 => "INTERRUPT_WINDOW",
        10 => "CPUID",
        12 => "HLT",
        18 => "VMCALL",
        30 => "IO_IN",
        31 => "IO_OUT",
        32 => "RDMSR",
        33 => "WRMSR",
        48 => "EPT_VIOLATION",
        49 => "EPT_MISCONFIG",
        // SVM exit codes (AMD)
        0x040 => "SVM_INTR",
        0x061 => "SVM_CPUID",
        0x064 => "SVM_VMRUN",
        0x06e => "SVM_RDTSC",
        0x078 => "SVM_HLT",
        0x07b => "SVM_IOIO",
        0x07c => "SVM_MSR",
        0x400 => "SVM_NPF",
        _ => "UNKNOWN",
    };

    // Postcondition: name is non-empty.
    debug_assert!(!result.is_empty(), "exit_reason_name returned empty string");

    result
}

// ─── Event kind parsing ─────────────────────────────────────────────

/// Parse raw event arguments into a typed [`EventKind`].
///
/// This is the pure core of `TraceEvent::from_raw()`, extracted so that
/// it can be verified independently of the `RawEvent` struct layout.
///
/// # Arguments
///
/// - `event_type` — the raw event type discriminant (1..=11, or unknown)
/// - `arg0..arg3` — the four 64-bit argument fields from the BPF event
///
/// # Properties verified by `verus/events_spec.rs`
///
/// - Valid event types (1..=11) produce the corresponding `EventKind` variant.
/// - Invalid event types produce `EventKind::Unknown`.
/// - The function is total: it never panics for any input combination.
pub fn parse_event_kind(
    event_type: u32,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> EventKind {
    let result = match event_type_from_u32(event_type) {
        Some(EventType::KvmExit) => EventKind::KvmExit {
            reason: arg0 as u32,
            guest_rip: arg1,
            info1: arg2,
            info2: arg3,
        },
        Some(EventType::KvmEntry) => EventKind::KvmEntry {
            vcpu_id: arg0 as u32,
            rip: arg1,
        },
        Some(EventType::KvmPio) => EventKind::KvmPio {
            direction: if arg0 == 0 {
                IoDirection::Read
            } else {
                IoDirection::Write
            },
            port: arg1 as u16,
            size: arg2 as u8,
            val: arg3 as u32,
        },
        Some(EventType::KvmMmio) => EventKind::KvmMmio {
            direction: if arg0 == 0 {
                IoDirection::Read
            } else {
                IoDirection::Write
            },
            len: arg1 as u32,
            gpa: arg2,
            val: arg3,
        },
        Some(EventType::KvmMsr) => EventKind::KvmMsr {
            direction: if arg0 == 0 {
                IoDirection::Read
            } else {
                IoDirection::Write
            },
            index: arg1 as u32,
            data: arg2,
        },
        Some(EventType::KvmInjVirq) => EventKind::KvmInjVirq {
            vector: arg0 as u32,
            soft: arg1 != 0,
            reinjected: arg2 != 0,
        },
        Some(EventType::KvmPicIrq) => EventKind::KvmPicIrq {
            chip: (arg0 >> 32) as u8,
            pin: (arg0 >> 16) as u8,
            elcr: (arg0 >> 8) as u8,
            imr: arg0 as u8,
            coalesced: arg1 != 0,
        },
        Some(EventType::KvmSetIrq) => EventKind::KvmSetIrq {
            gsi: arg0 as u32,
            level: arg1 as i32,
        },
        Some(EventType::KvmPageFault) => EventKind::KvmPageFault {
            vcpu_id: arg0 as u32,
            guest_rip: arg1,
            fault_address: arg2,
            error_code: arg3,
        },
        Some(EventType::KvmCr) => EventKind::KvmCr {
            direction: if arg0 == 0 {
                IoDirection::Read
            } else {
                IoDirection::Write
            },
            cr: arg1 as u32,
            val: arg2,
        },
        Some(EventType::KvmCpuid) => EventKind::KvmCpuid {
            function: (arg0 >> 32) as u32,
            index: arg0 as u32,
            rax: arg1,
            rbx: arg2,
            rcx: arg3 >> 32,
            rdx: arg3 & 0xFFFF_FFFF,
        },
        None => EventKind::Unknown {
            event_type,
            arg0,
            arg1,
            arg2,
            arg3,
        },
    };

    // Postcondition: unknown types produce the Unknown variant.
    debug_assert!(
        event_type_from_u32(event_type).is_some()
            || matches!(result, EventKind::Unknown { .. }),
        "parse_event_kind: unrecognised type must produce Unknown"
    );

    result
}

// ─── EventKind equality ─────────────────────────────────────────────

/// Compare two [`EventKind`] values for deterministic equivalence.
///
/// This is the pure core of the `PartialEq for EventKind` impl,
/// extracted so that its properties (reflexivity, symmetry) can be
/// verified.
///
/// Note: `KvmPicIrq` compares only `chip` and `pin` (not `elcr`/`imr`/
/// `coalesced`), and `KvmPageFault` compares only `fault_address` and
/// `error_code` (not `vcpu_id`/`guest_rip`).  This matches the existing
/// `PartialEq` semantics.
///
/// # Properties verified by `verus/events_spec.rs`
///
/// - Reflexive: `event_kind_eq(e, e) == true` for all `e`
/// - Symmetric: `event_kind_eq(a, b) == event_kind_eq(b, a)`
pub fn event_kind_eq(a: &EventKind, b: &EventKind) -> bool {
    let result = match (a, b) {
        (
            EventKind::KvmExit {
                reason: r1,
                guest_rip: rip1,
                info1: i1a,
                info2: i2a,
            },
            EventKind::KvmExit {
                reason: r2,
                guest_rip: rip2,
                info1: i1b,
                info2: i2b,
            },
        ) => r1 == r2 && rip1 == rip2 && i1a == i1b && i2a == i2b,

        (
            EventKind::KvmEntry {
                vcpu_id: v1,
                rip: r1,
            },
            EventKind::KvmEntry {
                vcpu_id: v2,
                rip: r2,
            },
        ) => v1 == v2 && r1 == r2,

        (
            EventKind::KvmPio {
                direction: d1,
                port: p1,
                size: s1,
                val: v1,
            },
            EventKind::KvmPio {
                direction: d2,
                port: p2,
                size: s2,
                val: v2,
            },
        ) => d1 == d2 && p1 == p2 && s1 == s2 && v1 == v2,

        (
            EventKind::KvmMmio {
                direction: d1,
                len: l1,
                gpa: g1,
                val: v1,
            },
            EventKind::KvmMmio {
                direction: d2,
                len: l2,
                gpa: g2,
                val: v2,
            },
        ) => d1 == d2 && l1 == l2 && g1 == g2 && v1 == v2,

        (
            EventKind::KvmMsr {
                direction: d1,
                index: i1,
                data: da1,
            },
            EventKind::KvmMsr {
                direction: d2,
                index: i2,
                data: da2,
            },
        ) => d1 == d2 && i1 == i2 && da1 == da2,

        (
            EventKind::KvmInjVirq {
                vector: v1,
                soft: s1,
                reinjected: r1,
            },
            EventKind::KvmInjVirq {
                vector: v2,
                soft: s2,
                reinjected: r2,
            },
        ) => v1 == v2 && s1 == s2 && r1 == r2,

        (
            EventKind::KvmPicIrq {
                chip: c1,
                pin: p1,
                ..
            },
            EventKind::KvmPicIrq {
                chip: c2,
                pin: p2,
                ..
            },
        ) => c1 == c2 && p1 == p2,

        (
            EventKind::KvmSetIrq {
                gsi: g1,
                level: l1,
            },
            EventKind::KvmSetIrq {
                gsi: g2,
                level: l2,
            },
        ) => g1 == g2 && l1 == l2,

        (
            EventKind::KvmPageFault {
                fault_address: a1,
                error_code: e1,
                ..
            },
            EventKind::KvmPageFault {
                fault_address: a2,
                error_code: e2,
                ..
            },
        ) => a1 == a2 && e1 == e2,

        (
            EventKind::KvmCr {
                direction: d1,
                cr: c1,
                val: v1,
            },
            EventKind::KvmCr {
                direction: d2,
                cr: c2,
                val: v2,
            },
        ) => d1 == d2 && c1 == c2 && v1 == v2,

        (
            EventKind::KvmCpuid {
                function: f1,
                index: i1,
                rax: a1,
                rbx: b1,
                rcx: c1,
                rdx: d1,
            },
            EventKind::KvmCpuid {
                function: f2,
                index: i2,
                rax: a2,
                rbx: b2,
                rcx: c2,
                rdx: d2,
            },
        ) => f1 == f2 && i1 == i2 && a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2,

        (
            EventKind::Unknown {
                event_type: t1,
                arg0: a0_1,
                arg1: a1_1,
                arg2: a2_1,
                arg3: a3_1,
            },
            EventKind::Unknown {
                event_type: t2,
                arg0: a0_2,
                arg1: a1_2,
                arg2: a2_2,
                arg3: a3_2,
            },
        ) => t1 == t2 && a0_1 == a0_2 && a1_1 == a1_2 && a2_1 == a2_2 && a3_1 == a3_2,

        _ => false,
    };

    result
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventKind, EventType, IoDirection};

    // ── event_type_from_u32 ──────────────────────────────────────

    #[test]
    fn from_u32_roundtrip_all_variants() {
        let variants = [
            (1, EventType::KvmExit),
            (2, EventType::KvmEntry),
            (3, EventType::KvmPio),
            (4, EventType::KvmMmio),
            (5, EventType::KvmMsr),
            (6, EventType::KvmInjVirq),
            (7, EventType::KvmPicIrq),
            (8, EventType::KvmSetIrq),
            (9, EventType::KvmPageFault),
            (10, EventType::KvmCr),
            (11, EventType::KvmCpuid),
        ];
        for (disc, expected) in &variants {
            let result = event_type_from_u32(*disc);
            assert_eq!(
                result,
                Some(*expected),
                "event_type_from_u32({disc}) should be Some({expected:?})"
            );
            // Verify disc matches the as-u32 cast.
            assert_eq!(
                result.unwrap() as u32,
                *disc,
                "round-trip: variant as u32 should equal {disc}"
            );
        }
    }

    #[test]
    fn from_u32_rejects_zero() {
        assert!(
            event_type_from_u32(0).is_none(),
            "event_type_from_u32(0) must be None"
        );
    }

    #[test]
    fn from_u32_rejects_above_max() {
        for v in [12, 13, 100, u32::MAX] {
            assert!(
                event_type_from_u32(v).is_none(),
                "event_type_from_u32({v}) must be None"
            );
        }
    }

    #[test]
    fn from_u32_rejects_all_invalid_range() {
        // Exhaustive: every value outside 1..=11 must return None.
        assert!(event_type_from_u32(0).is_none());
        for v in 12..=256u32 {
            assert!(event_type_from_u32(v).is_none(), "v={v} should be None");
        }
    }

    // ── event_type_name ──────────────────────────────────────────

    #[test]
    fn name_is_nonempty_for_all_variants() {
        for disc in 1..=11u32 {
            let et = event_type_from_u32(disc).unwrap();
            let name = event_type_name(&et);
            assert!(!name.is_empty(), "name for disc={disc} must not be empty");
        }
    }

    #[test]
    fn names_are_distinct() {
        let mut names = Vec::new();
        for disc in 1..=11u32 {
            let et = event_type_from_u32(disc).unwrap();
            names.push(event_type_name(&et));
        }
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(
            names.len(),
            unique.len(),
            "all event type names must be distinct"
        );
    }

    #[test]
    fn name_matches_parent_module() {
        // Verify our names agree with the parent EventType::name().
        for disc in 1..=11u32 {
            let et = event_type_from_u32(disc).unwrap();
            assert_eq!(
                event_type_name(&et),
                et.name(),
                "verified name must match EventType::name() for disc={disc}"
            );
        }
    }

    // ── exit_reason_name ─────────────────────────────────────────

    #[test]
    fn known_exit_reasons_have_names() {
        let known = [0, 1, 2, 7, 10, 12, 18, 30, 31, 32, 33, 48, 49];
        for reason in &known {
            let name = exit_reason_name(*reason);
            assert_ne!(
                name, "UNKNOWN",
                "exit reason {reason} should have a known name"
            );
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn unknown_exit_reasons() {
        let unknown = [3, 4, 5, 6, 8, 9, 11, 50, 255, 1000, u32::MAX];
        for reason in &unknown {
            assert_eq!(
                exit_reason_name(*reason),
                "UNKNOWN",
                "exit reason {reason} should be UNKNOWN"
            );
        }
    }

    #[test]
    fn exit_reason_matches_parent_module() {
        for reason in 0..=0x500u32 {
            assert_eq!(
                exit_reason_name(reason),
                crate::events::exit_reason_name(reason),
                "verified exit_reason_name must match parent for reason={reason}"
            );
        }
    }

    // ── parse_event_kind ─────────────────────────────────────────

    #[test]
    fn parse_kvm_exit() {
        let kind = parse_event_kind(1, 12, 0xdead_beef, 0, 0);
        assert!(matches!(
            kind,
            EventKind::KvmExit {
                reason: 12,
                guest_rip: 0xdead_beef,
                info1: 0,
                info2: 0,
            }
        ));
    }

    #[test]
    fn parse_kvm_pio_write() {
        let kind = parse_event_kind(3, 1, 0x3f8, 1, 0x48);
        assert!(matches!(
            kind,
            EventKind::KvmPio {
                direction: IoDirection::Write,
                port: 0x3f8,
                size: 1,
                val: 0x48,
            }
        ));
    }

    #[test]
    fn parse_kvm_pio_read() {
        let kind = parse_event_kind(3, 0, 0x3f8, 1, 0x00);
        assert!(matches!(
            kind,
            EventKind::KvmPio {
                direction: IoDirection::Read,
                port: 0x3f8,
                size: 1,
                val: 0x00,
            }
        ));
    }

    #[test]
    fn parse_kvm_entry() {
        let kind = parse_event_kind(2, 0, 0x1000, 0, 0);
        assert!(matches!(
            kind,
            EventKind::KvmEntry {
                vcpu_id: 0,
                rip: 0x1000,
            }
        ));
    }

    #[test]
    fn parse_unknown_type() {
        let kind = parse_event_kind(255, 1, 2, 3, 4);
        assert!(matches!(
            kind,
            EventKind::Unknown {
                event_type: 255,
                arg0: 1,
                arg1: 2,
                arg2: 3,
                arg3: 4,
            }
        ));
    }

    #[test]
    fn parse_zero_type_is_unknown() {
        let kind = parse_event_kind(0, 0, 0, 0, 0);
        assert!(
            matches!(kind, EventKind::Unknown { event_type: 0, .. }),
            "type 0 must produce Unknown"
        );
    }

    #[test]
    fn parse_all_valid_types_are_not_unknown() {
        for event_type in 1..=11u32 {
            let kind = parse_event_kind(event_type, 0, 0, 0, 0);
            assert!(
                !matches!(kind, EventKind::Unknown { .. }),
                "valid type {event_type} must not produce Unknown"
            );
        }
    }

    // ── event_kind_eq ────────────────────────────────────────────

    #[test]
    fn eq_reflexive_kvm_exit() {
        let e = EventKind::KvmExit {
            reason: 12,
            guest_rip: 0x1000,
            info1: 0,
            info2: 0,
        };
        assert!(
            event_kind_eq(&e, &e),
            "event_kind_eq must be reflexive for KvmExit"
        );
    }

    #[test]
    fn eq_reflexive_all_variants() {
        let variants: Vec<EventKind> = vec![
            EventKind::KvmExit { reason: 1, guest_rip: 2, info1: 3, info2: 4 },
            EventKind::KvmEntry { vcpu_id: 0, rip: 0x1000 },
            EventKind::KvmPio { direction: IoDirection::Write, port: 0x3f8, size: 1, val: 0x48 },
            EventKind::KvmMmio { direction: IoDirection::Read, len: 4, gpa: 0x1000, val: 42 },
            EventKind::KvmMsr { direction: IoDirection::Write, index: 0x174, data: 0xdeadbeef },
            EventKind::KvmInjVirq { vector: 32, soft: true, reinjected: false },
            EventKind::KvmPicIrq { chip: 0, pin: 1, elcr: 0, imr: 0, coalesced: false },
            EventKind::KvmSetIrq { gsi: 4, level: 1 },
            EventKind::KvmPageFault { vcpu_id: 0, guest_rip: 0x1000, fault_address: 0x2000, error_code: 1 },
            EventKind::KvmCr { direction: IoDirection::Write, cr: 0, val: 0x80000001 },
            EventKind::KvmCpuid { function: 1, index: 0, rax: 1, rbx: 2, rcx: 3, rdx: 4 },
            EventKind::Unknown { event_type: 99, arg0: 1, arg1: 2, arg2: 3, arg3: 4 },
        ];
        for (i, v) in variants.iter().enumerate() {
            assert!(
                event_kind_eq(v, v),
                "event_kind_eq must be reflexive for variant index {i}"
            );
        }
    }

    #[test]
    fn eq_symmetric() {
        let a = EventKind::KvmPio {
            direction: IoDirection::Write,
            port: 0x3f8,
            size: 1,
            val: 0x48,
        };
        let b = EventKind::KvmPio {
            direction: IoDirection::Write,
            port: 0x3f8,
            size: 1,
            val: 0x48,
        };
        assert_eq!(
            event_kind_eq(&a, &b),
            event_kind_eq(&b, &a),
            "event_kind_eq must be symmetric"
        );
    }

    #[test]
    fn eq_different_variants_false() {
        let a = EventKind::KvmExit {
            reason: 12,
            guest_rip: 0x1000,
            info1: 0,
            info2: 0,
        };
        let b = EventKind::KvmEntry {
            vcpu_id: 0,
            rip: 0x1000,
        };
        assert!(
            !event_kind_eq(&a, &b),
            "different variants must not be equal"
        );
    }

    #[test]
    fn eq_different_data_false() {
        let a = EventKind::KvmPio {
            direction: IoDirection::Write,
            port: 0x3f8,
            size: 1,
            val: 0x48,
        };
        let b = EventKind::KvmPio {
            direction: IoDirection::Write,
            port: 0x3f8,
            size: 1,
            val: 0x49, // different
        };
        assert!(!event_kind_eq(&a, &b), "different data must not be equal");
    }

    #[test]
    fn eq_matches_partial_eq_impl() {
        // Verify our function agrees with the PartialEq trait impl.
        let cases: Vec<(EventKind, EventKind)> = vec![
            (
                EventKind::KvmExit { reason: 12, guest_rip: 0x1000, info1: 0, info2: 0 },
                EventKind::KvmExit { reason: 12, guest_rip: 0x1000, info1: 0, info2: 0 },
            ),
            (
                EventKind::KvmExit { reason: 12, guest_rip: 0x1000, info1: 0, info2: 0 },
                EventKind::KvmExit { reason: 7, guest_rip: 0x1000, info1: 0, info2: 0 },
            ),
            (
                EventKind::KvmPio { direction: IoDirection::Write, port: 0x3f8, size: 1, val: 0x48 },
                EventKind::KvmPio { direction: IoDirection::Write, port: 0x3f8, size: 1, val: 0x48 },
            ),
            (
                EventKind::KvmPio { direction: IoDirection::Write, port: 0x3f8, size: 1, val: 0x48 },
                EventKind::KvmEntry { vcpu_id: 0, rip: 0 },
            ),
        ];
        for (i, (a, b)) in cases.iter().enumerate() {
            assert_eq!(
                event_kind_eq(a, b),
                a == b,
                "event_kind_eq must agree with PartialEq for case {i}"
            );
        }
    }

    #[test]
    fn eq_pic_irq_ignores_elcr_imr_coalesced() {
        let a = EventKind::KvmPicIrq {
            chip: 0,
            pin: 1,
            elcr: 0xFF,
            imr: 0xFF,
            coalesced: true,
        };
        let b = EventKind::KvmPicIrq {
            chip: 0,
            pin: 1,
            elcr: 0x00,
            imr: 0x00,
            coalesced: false,
        };
        assert!(
            event_kind_eq(&a, &b),
            "KvmPicIrq equality should ignore elcr/imr/coalesced"
        );
    }

    #[test]
    fn eq_page_fault_ignores_vcpu_id_and_rip() {
        let a = EventKind::KvmPageFault {
            vcpu_id: 0,
            guest_rip: 0x1000,
            fault_address: 0x2000,
            error_code: 1,
        };
        let b = EventKind::KvmPageFault {
            vcpu_id: 1,
            guest_rip: 0x3000,
            fault_address: 0x2000,
            error_code: 1,
        };
        assert!(
            event_kind_eq(&a, &b),
            "KvmPageFault equality should ignore vcpu_id and guest_rip"
        );
    }
}
