//! Trace event types shared between BPF and userspace.
//!
//! The [`RawEvent`] struct matches the BPF-side `struct trace_event` exactly
//! (64 bytes, cache-line aligned). Higher-level [`TraceEvent`] provides typed
//! access to event-specific fields.

use serde::{Deserialize, Serialize};
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════
//  Raw event (matches BPF struct exactly)
// ═══════════════════════════════════════════════════════════════════════

/// Raw 64-byte event as received from the BPF ring buffer.
///
/// Must match `struct trace_event` in `kvm_trace.bpf.c` exactly.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RawEvent {
    pub seq: u64,
    pub host_ns: u64,
    pub event_type: u32,
    pub pid: u32,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
}

const _: () = assert!(std::mem::size_of::<RawEvent>() == 56);

// ═══════════════════════════════════════════════════════════════════════
//  Event type constants (must match BPF side)
// ═══════════════════════════════════════════════════════════════════════

/// Event type discriminants matching the BPF `EVENT_*` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum EventType {
    KvmExit = 1,
    KvmEntry = 2,
    KvmPio = 3,
    KvmMmio = 4,
    KvmMsr = 5,
    KvmInjVirq = 6,
    KvmPicIrq = 7,
    KvmSetIrq = 8,
    KvmPageFault = 9,
    KvmCr = 10,
    KvmCpuid = 11,
}

impl EventType {
    /// Convert a `u32` discriminant to an [`EventType`], if valid.
    ///
    /// Delegates to [`crate::verified::events::event_type_from_u32`].
    pub fn from_u32(v: u32) -> Option<Self> {
        crate::verified::events::event_type_from_u32(v)
    }

    /// Return the human-readable tracepoint name.
    ///
    /// Delegates to [`crate::verified::events::event_type_name`].
    pub fn name(&self) -> &'static str {
        crate::verified::events::event_type_name(self)
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Typed event (parsed from RawEvent)
// ═══════════════════════════════════════════════════════════════════════

/// A parsed, typed trace event with named fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Monotonic sequence number (per-CPU, may interleave across CPUs).
    pub seq: u64,
    /// Host monotonic timestamp (nanoseconds, from `bpf_ktime_get_ns`).
    /// NOT deterministic — use only for performance analysis.
    pub host_ns: u64,
    /// PID of the traced process.
    pub pid: u32,
    /// Parsed event data.
    pub kind: EventKind,
}

/// KVM exit reason names (x86).
///
/// Delegates to [`crate::verified::events::exit_reason_name`].
pub fn exit_reason_name(reason: u32) -> &'static str {
    crate::verified::events::exit_reason_name(reason)
}

/// I/O direction for PIO/MMIO/MSR events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IoDirection {
    Read,
    Write,
}

impl fmt::Display for IoDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => write!(f, "R"),
            Self::Write => write!(f, "W"),
        }
    }
}

/// Typed event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventKind {
    KvmExit {
        reason: u32,
        guest_rip: u64,
        info1: u64,
        info2: u64,
    },
    KvmEntry {
        vcpu_id: u32,
        rip: u64,
    },
    KvmPio {
        direction: IoDirection,
        port: u16,
        size: u8,
        val: u32,
    },
    KvmMmio {
        direction: IoDirection,
        len: u32,
        gpa: u64,
        val: u64,
    },
    KvmMsr {
        direction: IoDirection,
        index: u32,
        data: u64,
    },
    KvmInjVirq {
        vector: u32,
        soft: bool,
        reinjected: bool,
    },
    KvmPicIrq {
        chip: u8,
        pin: u8,
        elcr: u8,
        imr: u8,
        coalesced: bool,
    },
    KvmSetIrq {
        gsi: u32,
        level: i32,
    },
    KvmPageFault {
        vcpu_id: u32,
        guest_rip: u64,
        fault_address: u64,
        error_code: u64,
    },
    KvmCr {
        direction: IoDirection,
        cr: u32,
        val: u64,
    },
    KvmCpuid {
        function: u32,
        index: u32,
        rax: u64,
        rbx: u64,
        rcx: u64,
        rdx: u64,
    },
    /// Unknown event type (forward compatibility).
    Unknown {
        event_type: u32,
        arg0: u64,
        arg1: u64,
        arg2: u64,
        arg3: u64,
    },
}

impl TraceEvent {
    /// Parse a [`RawEvent`] into a typed [`TraceEvent`].
    ///
    /// Delegates to [`crate::verified::events::parse_event_kind`] for
    /// the pure event-kind parsing logic.
    pub fn from_raw(raw: &RawEvent) -> Self {
        let kind = crate::verified::events::parse_event_kind(
            raw.event_type,
            raw.arg0,
            raw.arg1,
            raw.arg2,
            raw.arg3,
        );

        TraceEvent {
            seq: raw.seq,
            host_ns: raw.host_ns,
            pid: raw.pid,
            kind,
        }
    }

    /// Get the event type.
    pub fn event_type(&self) -> EventType {
        match &self.kind {
            EventKind::KvmExit { .. } => EventType::KvmExit,
            EventKind::KvmEntry { .. } => EventType::KvmEntry,
            EventKind::KvmPio { .. } => EventType::KvmPio,
            EventKind::KvmMmio { .. } => EventType::KvmMmio,
            EventKind::KvmMsr { .. } => EventType::KvmMsr,
            EventKind::KvmInjVirq { .. } => EventType::KvmInjVirq,
            EventKind::KvmPicIrq { .. } => EventType::KvmPicIrq,
            EventKind::KvmSetIrq { .. } => EventType::KvmSetIrq,
            EventKind::KvmPageFault { .. } => EventType::KvmPageFault,
            EventKind::KvmCr { .. } => EventType::KvmCr,
            EventKind::KvmCpuid { .. } => EventType::KvmCpuid,
            EventKind::Unknown { event_type, .. } => {
                EventType::from_u32(*event_type).unwrap_or(EventType::KvmExit)
            }
        }
    }

    /// Compare two events for determinism (ignoring host timestamps).
    ///
    /// Returns `true` if the events are equivalent from a determinism
    /// perspective: same event type and same data fields. Host-side
    /// timestamps and sequence numbers are NOT compared.
    pub fn determinism_eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

// PartialEq for EventKind — delegates to the verified pure function.
impl PartialEq for EventKind {
    fn eq(&self, other: &Self) -> bool {
        crate::verified::events::event_kind_eq(self, other)
    }
}

impl Eq for EventKind {}

// ═══════════════════════════════════════════════════════════════════════
//  Display
// ═══════════════════════════════════════════════════════════════════════

impl fmt::Display for TraceEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:>8}] {:>12}ns ", self.seq, self.host_ns)?;
        match &self.kind {
            EventKind::KvmExit {
                reason,
                guest_rip,
                info1,
                info2,
            } => {
                write!(
                    f,
                    "EXIT  reason={:<20} rip={:#x} info1={:#x} info2={:#x}",
                    exit_reason_name(*reason),
                    guest_rip,
                    info1,
                    info2,
                )
            }
            EventKind::KvmEntry { vcpu_id, rip } => {
                write!(f, "ENTRY vcpu={} rip={:#x}", vcpu_id, rip)
            }
            EventKind::KvmPio {
                direction,
                port,
                size,
                val,
            } => {
                write!(
                    f,
                    "PIO   {} port={:#06x} size={} val={:#x}",
                    direction, port, size, val,
                )
            }
            EventKind::KvmMmio {
                direction,
                len,
                gpa,
                val,
            } => {
                write!(
                    f,
                    "MMIO  {} gpa={:#x} len={} val={:#x}",
                    direction, gpa, len, val,
                )
            }
            EventKind::KvmMsr {
                direction,
                index,
                data,
            } => {
                write!(
                    f,
                    "MSR   {} index={:#x} data={:#x}",
                    direction, index, data,
                )
            }
            EventKind::KvmInjVirq {
                vector,
                soft,
                reinjected,
            } => {
                write!(
                    f,
                    "VIRQ  vector={} soft={} reinj={}",
                    vector, soft, reinjected,
                )
            }
            EventKind::KvmPicIrq { chip, pin, .. } => {
                write!(f, "PIC   chip={} pin={}", chip, pin)
            }
            EventKind::KvmSetIrq { gsi, level } => {
                write!(f, "IRQ   gsi={} level={}", gsi, level)
            }
            EventKind::KvmPageFault {
                fault_address,
                error_code,
                guest_rip,
                ..
            } => {
                write!(
                    f,
                    "FAULT addr={:#x} err={:#x} rip={:#x}",
                    fault_address, error_code, guest_rip,
                )
            }
            EventKind::KvmCr {
                direction,
                cr,
                val,
            } => {
                write!(f, "CR    {} cr{}={:#x}", direction, cr, val)
            }
            EventKind::KvmCpuid {
                function,
                index,
                rax,
                rbx,
                rcx,
                rdx,
            } => {
                write!(
                    f,
                    "CPUID fn={:#x} idx={} rax={:#x} rbx={:#x} rcx={:#x} rdx={:#x}",
                    function, index, rax, rbx, rcx, rdx,
                )
            }
            EventKind::Unknown { event_type, .. } => {
                write!(f, "UNK   type={}", event_type)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_event_size() {
        assert_eq!(std::mem::size_of::<RawEvent>(), 56);
    }

    #[test]
    fn event_type_roundtrip() {
        for t in 1..=11 {
            let et = EventType::from_u32(t).unwrap();
            assert_eq!(et as u32, t);
        }
        assert!(EventType::from_u32(0).is_none());
        assert!(EventType::from_u32(12).is_none());
    }

    #[test]
    fn parse_kvm_exit() {
        let raw = RawEvent {
            seq: 42,
            host_ns: 1_000_000,
            event_type: 1,
            pid: 1234,
            arg0: 12, // HLT
            arg1: 0xdead_beef,
            arg2: 0,
            arg3: 0,
        };
        let event = TraceEvent::from_raw(&raw);
        assert_eq!(event.seq, 42);
        assert!(matches!(
            event.kind,
            EventKind::KvmExit {
                reason: 12,
                guest_rip: 0xdead_beef,
                ..
            }
        ));
    }

    #[test]
    fn parse_kvm_pio() {
        let raw = RawEvent {
            seq: 1,
            host_ns: 0,
            event_type: 3,
            pid: 100,
            arg0: 1, // write
            arg1: 0x3f8,
            arg2: 1,
            arg3: 0x48, // 'H'
        };
        let event = TraceEvent::from_raw(&raw);
        assert!(matches!(
            event.kind,
            EventKind::KvmPio {
                direction: IoDirection::Write,
                port: 0x3f8,
                size: 1,
                val: 0x48,
            }
        ));
    }

    #[test]
    fn determinism_eq_ignores_host_ns() {
        let a = TraceEvent {
            seq: 1,
            host_ns: 100,
            pid: 1,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1000,
                info1: 0,
                info2: 0,
            },
        };
        let b = TraceEvent {
            seq: 1,
            host_ns: 999, // different host time
            pid: 1,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1000,
                info1: 0,
                info2: 0,
            },
        };
        assert!(a.determinism_eq(&b));
    }
}
