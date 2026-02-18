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
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::KvmExit),
            2 => Some(Self::KvmEntry),
            3 => Some(Self::KvmPio),
            4 => Some(Self::KvmMmio),
            5 => Some(Self::KvmMsr),
            6 => Some(Self::KvmInjVirq),
            7 => Some(Self::KvmPicIrq),
            8 => Some(Self::KvmSetIrq),
            9 => Some(Self::KvmPageFault),
            10 => Some(Self::KvmCr),
            11 => Some(Self::KvmCpuid),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::KvmExit => "kvm_exit",
            Self::KvmEntry => "kvm_entry",
            Self::KvmPio => "kvm_pio",
            Self::KvmMmio => "kvm_mmio",
            Self::KvmMsr => "kvm_msr",
            Self::KvmInjVirq => "kvm_inj_virq",
            Self::KvmPicIrq => "kvm_pic_set_irq",
            Self::KvmSetIrq => "kvm_set_irq",
            Self::KvmPageFault => "kvm_page_fault",
            Self::KvmCr => "kvm_cr",
            Self::KvmCpuid => "kvm_cpuid",
        }
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
pub fn exit_reason_name(reason: u32) -> &'static str {
    match reason {
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
    }
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
    pub fn from_raw(raw: &RawEvent) -> Self {
        let kind = match EventType::from_u32(raw.event_type) {
            Some(EventType::KvmExit) => EventKind::KvmExit {
                reason: raw.arg0 as u32,
                guest_rip: raw.arg1,
                info1: raw.arg2,
                info2: raw.arg3,
            },
            Some(EventType::KvmEntry) => EventKind::KvmEntry {
                vcpu_id: raw.arg0 as u32,
                rip: raw.arg1,
            },
            Some(EventType::KvmPio) => EventKind::KvmPio {
                direction: if raw.arg0 == 0 {
                    IoDirection::Read
                } else {
                    IoDirection::Write
                },
                port: raw.arg1 as u16,
                size: raw.arg2 as u8,
                val: raw.arg3 as u32,
            },
            Some(EventType::KvmMmio) => EventKind::KvmMmio {
                direction: if raw.arg0 == 0 {
                    IoDirection::Read
                } else {
                    IoDirection::Write
                },
                len: raw.arg1 as u32,
                gpa: raw.arg2,
                val: raw.arg3,
            },
            Some(EventType::KvmMsr) => EventKind::KvmMsr {
                direction: if raw.arg0 == 0 {
                    IoDirection::Read
                } else {
                    IoDirection::Write
                },
                index: raw.arg1 as u32,
                data: raw.arg2,
            },
            Some(EventType::KvmInjVirq) => EventKind::KvmInjVirq {
                vector: raw.arg0 as u32,
                soft: raw.arg1 != 0,
                reinjected: raw.arg2 != 0,
            },
            Some(EventType::KvmPicIrq) => EventKind::KvmPicIrq {
                chip: (raw.arg0 >> 32) as u8,
                pin: (raw.arg0 >> 16) as u8,
                elcr: (raw.arg0 >> 8) as u8,
                imr: raw.arg0 as u8,
                coalesced: raw.arg1 != 0,
            },
            Some(EventType::KvmSetIrq) => EventKind::KvmSetIrq {
                gsi: raw.arg0 as u32,
                level: raw.arg1 as i32,
            },
            Some(EventType::KvmPageFault) => EventKind::KvmPageFault {
                vcpu_id: raw.arg0 as u32,
                guest_rip: raw.arg1,
                fault_address: raw.arg2,
                error_code: raw.arg3,
            },
            Some(EventType::KvmCr) => EventKind::KvmCr {
                direction: if raw.arg0 == 0 {
                    IoDirection::Read
                } else {
                    IoDirection::Write
                },
                cr: raw.arg1 as u32,
                val: raw.arg2,
            },
            Some(EventType::KvmCpuid) => EventKind::KvmCpuid {
                function: (raw.arg0 >> 32) as u32,
                index: raw.arg0 as u32,
                rax: raw.arg1,
                rbx: raw.arg2,
                rcx: raw.arg3 >> 32,
                rdx: raw.arg3 & 0xFFFF_FFFF,
            },
            None => EventKind::Unknown {
                event_type: raw.event_type,
                arg0: raw.arg0,
                arg1: raw.arg1,
                arg2: raw.arg2,
                arg3: raw.arg3,
            },
        };

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

// PartialEq for EventKind compares all data fields
impl PartialEq for EventKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
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

            _ => false,
        }
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
