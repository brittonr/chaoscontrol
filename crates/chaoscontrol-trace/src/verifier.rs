//! Determinism verification by comparing two execution traces.
//!
//! The [`DeterminismVerifier`] compares two [`TraceLog`]s event-by-event,
//! ignoring host-side non-deterministic fields (timestamps, sequence
//! numbers) and focusing on determinism-relevant data (exit reasons,
//! I/O ports, data values, interrupt vectors, etc.).
//!
//! This is the core CI tool for ChaosControl: run the same VM config
//! with the same seed twice, collect traces, and verify they match.

use crate::collector::TraceLog;
use crate::events::{EventKind, TraceEvent};
use serde::{Deserialize, Serialize};
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════
//  Divergence
// ═══════════════════════════════════════════════════════════════════════

/// A point where two execution traces diverged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Divergence {
    /// Index in the event stream where divergence occurred.
    pub event_index: usize,
    /// Description of what differs.
    pub description: String,
    /// Event from trace A at the divergence point.
    pub trace_a_event: Option<TraceEvent>,
    /// Event from trace B at the divergence point.
    pub trace_b_event: Option<TraceEvent>,
    /// A few events before the divergence for context.
    pub context_before: Vec<(TraceEvent, TraceEvent)>,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "DIVERGENCE at event index {}:",
            self.event_index
        )?;
        writeln!(f, "  {}", self.description)?;

        if let Some(ref a) = self.trace_a_event {
            writeln!(f, "  Trace A: {}", a)?;
        } else {
            writeln!(f, "  Trace A: <ended>")?;
        }

        if let Some(ref b) = self.trace_b_event {
            writeln!(f, "  Trace B: {}", b)?;
        } else {
            writeln!(f, "  Trace B: <ended>")?;
        }

        if !self.context_before.is_empty() {
            writeln!(f, "  Context (preceding events):")?;
            for (i, (a, b)) in self.context_before.iter().enumerate() {
                let idx = self.event_index - self.context_before.len() + i;
                if a.determinism_eq(b) {
                    writeln!(f, "    [{:>6}] ✓ {}", idx, a)?;
                } else {
                    writeln!(f, "    [{:>6}] ✗ A: {}", idx, a)?;
                    writeln!(f, "           ✗ B: {}", b)?;
                }
            }
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Verification result
// ═══════════════════════════════════════════════════════════════════════

/// Result of comparing two traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the traces are deterministically equivalent.
    pub is_deterministic: bool,
    /// Number of events in trace A.
    pub trace_a_len: usize,
    /// Number of events in trace B.
    pub trace_b_len: usize,
    /// Number of events that matched.
    pub matching_events: usize,
    /// First divergence (if any).
    pub first_divergence: Option<Divergence>,
    /// Event type breakdown for trace A.
    pub trace_a_summary: std::collections::HashMap<String, usize>,
    /// Event type breakdown for trace B.
    pub trace_b_summary: std::collections::HashMap<String, usize>,
}

impl fmt::Display for VerificationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_deterministic {
            writeln!(
                f,
                "✅ DETERMINISTIC: {} events matched perfectly",
                self.matching_events
            )?;
        } else {
            writeln!(f, "❌ NON-DETERMINISTIC")?;
            writeln!(
                f,
                "   Trace A: {} events, Trace B: {} events",
                self.trace_a_len, self.trace_b_len
            )?;
            writeln!(
                f,
                "   Matched {} events before divergence",
                self.matching_events
            )?;
        }

        if let Some(ref div) = self.first_divergence {
            writeln!(f)?;
            write!(f, "{}", div)?;
        }

        // Show summary comparison
        writeln!(f, "\nEvent type summary:")?;
        let mut all_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        all_types.extend(self.trace_a_summary.keys().cloned());
        all_types.extend(self.trace_b_summary.keys().cloned());

        writeln!(f, "  {:>20} {:>10} {:>10} {:>10}", "Event", "Trace A", "Trace B", "Delta")?;
        for t in &all_types {
            let a = self.trace_a_summary.get(t).copied().unwrap_or(0);
            let b = self.trace_b_summary.get(t).copied().unwrap_or(0);
            let delta = b as i64 - a as i64;
            let marker = if delta != 0 { " ⚠" } else { "" };
            writeln!(
                f,
                "  {:>20} {:>10} {:>10} {:>+10}{}",
                t, a, b, delta, marker
            )?;
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Verifier
// ═══════════════════════════════════════════════════════════════════════

/// Compares two execution traces for deterministic equivalence.
///
/// Two traces are considered deterministic if they contain the same
/// sequence of events with the same determinism-relevant data. Fields
/// like host timestamps and per-CPU sequence numbers are ignored.
///
/// # Example
///
/// ```no_run
/// use chaoscontrol_trace::verifier::DeterminismVerifier;
/// use chaoscontrol_trace::collector::TraceLog;
///
/// let trace_a = TraceLog::load("run1.json").unwrap();
/// let trace_b = TraceLog::load("run2.json").unwrap();
///
/// let result = DeterminismVerifier::compare(&trace_a, &trace_b);
/// println!("{}", result);
/// assert!(result.is_deterministic);
/// ```
pub struct DeterminismVerifier;

impl DeterminismVerifier {
    /// Compare two traces for deterministic equivalence.
    ///
    /// Compares event-by-event, ignoring host_ns and seq (those are
    /// non-deterministic by nature). Focuses on event type and data.
    pub fn compare(trace_a: &TraceLog, trace_b: &TraceLog) -> VerificationResult {
        Self::compare_events(&trace_a.events, &trace_b.events, trace_a, trace_b)
    }

    /// Compare two raw event vectors.
    fn compare_events(
        events_a: &[TraceEvent],
        events_b: &[TraceEvent],
        log_a: &TraceLog,
        log_b: &TraceLog,
    ) -> VerificationResult {
        let context_window = 5;
        let mut matching = 0usize;

        let min_len = events_a.len().min(events_b.len());

        for i in 0..min_len {
            if !events_a[i].determinism_eq(&events_b[i]) {
                // Found divergence
                let context_start = i.saturating_sub(context_window);
                let context_before: Vec<(TraceEvent, TraceEvent)> = (context_start..i)
                    .map(|j| (events_a[j].clone(), events_b[j].clone()))
                    .collect();

                let description = describe_divergence(&events_a[i], &events_b[i]);

                return VerificationResult {
                    is_deterministic: false,
                    trace_a_len: events_a.len(),
                    trace_b_len: events_b.len(),
                    matching_events: matching,
                    first_divergence: Some(Divergence {
                        event_index: i,
                        description,
                        trace_a_event: Some(events_a[i].clone()),
                        trace_b_event: Some(events_b[i].clone()),
                        context_before,
                    }),
                    trace_a_summary: log_a.summary(),
                    trace_b_summary: log_b.summary(),
                };
            }
            matching += 1;
        }

        // Check length mismatch
        if events_a.len() != events_b.len() {
            let longer_idx = min_len;
            let (longer_event, which) = if events_a.len() > events_b.len() {
                (Some(events_a[longer_idx].clone()), "A")
            } else {
                (Some(events_b[longer_idx].clone()), "B")
            };

            let context_start = longer_idx.saturating_sub(context_window);
            let context_before: Vec<(TraceEvent, TraceEvent)> = (context_start..min_len)
                .map(|j| (events_a[j].clone(), events_b[j].clone()))
                .collect();

            let (a_event, b_event) = if which == "A" {
                (longer_event, None)
            } else {
                (None, longer_event)
            };

            return VerificationResult {
                is_deterministic: false,
                trace_a_len: events_a.len(),
                trace_b_len: events_b.len(),
                matching_events: matching,
                first_divergence: Some(Divergence {
                    event_index: min_len,
                    description: format!(
                        "Trace length mismatch: A has {} events, B has {} events (delta: {})",
                        events_a.len(),
                        events_b.len(),
                        events_a.len() as i64 - events_b.len() as i64,
                    ),
                    trace_a_event: a_event,
                    trace_b_event: b_event,
                    context_before,
                }),
                trace_a_summary: log_a.summary(),
                trace_b_summary: log_b.summary(),
            };
        }

        // Perfect match
        VerificationResult {
            is_deterministic: true,
            trace_a_len: events_a.len(),
            trace_b_len: events_b.len(),
            matching_events: matching,
            first_divergence: None,
            trace_a_summary: log_a.summary(),
            trace_b_summary: log_b.summary(),
        }
    }

    /// Compare two traces, filtering to only specific event types.
    ///
    /// Useful for checking determinism of a subset of events (e.g.,
    /// only I/O events, only interrupts).
    pub fn compare_filtered(
        trace_a: &TraceLog,
        trace_b: &TraceLog,
        filter: &dyn Fn(&TraceEvent) -> bool,
    ) -> VerificationResult {
        let filtered_a: Vec<TraceEvent> =
            trace_a.events.iter().filter(|e| filter(e)).cloned().collect();
        let filtered_b: Vec<TraceEvent> =
            trace_b.events.iter().filter(|e| filter(e)).cloned().collect();
        Self::compare_events(&filtered_a, &filtered_b, trace_a, trace_b)
    }
}

/// Describe how two events differ.
fn describe_divergence(a: &TraceEvent, b: &TraceEvent) -> String {
    let type_a = a.event_type();
    let type_b = b.event_type();

    if type_a != type_b {
        return format!(
            "Event type mismatch: A={}, B={}",
            type_a.name(),
            type_b.name()
        );
    }

    match (&a.kind, &b.kind) {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::*;

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

    fn make_log(events: Vec<TraceEvent>) -> TraceLog {
        TraceLog::new(1, events)
    }

    #[test]
    fn identical_traces_are_deterministic() {
        let events = vec![
            make_exit(12, 0x1000),
            make_pio(IoDirection::Write, 0x3f8, 0x48),
            make_exit(12, 0x2000),
        ];
        let log_a = make_log(events.clone());
        let log_b = make_log(events);

        let result = DeterminismVerifier::compare(&log_a, &log_b);
        assert!(result.is_deterministic);
        assert_eq!(result.matching_events, 3);
        assert!(result.first_divergence.is_none());
    }

    #[test]
    fn different_event_types_diverge() {
        let log_a = make_log(vec![make_exit(12, 0x1000)]);
        let log_b = make_log(vec![make_pio(IoDirection::Write, 0x3f8, 0x48)]);

        let result = DeterminismVerifier::compare(&log_a, &log_b);
        assert!(!result.is_deterministic);
        assert_eq!(result.matching_events, 0);
        let div = result.first_divergence.unwrap();
        assert_eq!(div.event_index, 0);
        assert!(div.description.contains("type mismatch"));
    }

    #[test]
    fn different_lengths_diverge() {
        let log_a = make_log(vec![
            make_exit(12, 0x1000),
            make_exit(12, 0x2000),
        ]);
        let log_b = make_log(vec![make_exit(12, 0x1000)]);

        let result = DeterminismVerifier::compare(&log_a, &log_b);
        assert!(!result.is_deterministic);
        assert_eq!(result.matching_events, 1);
        let div = result.first_divergence.unwrap();
        assert!(div.description.contains("length mismatch"));
    }

    #[test]
    fn pio_data_divergence_is_descriptive() {
        let log_a = make_log(vec![make_pio(IoDirection::Write, 0x3f8, 0x48)]);
        let log_b = make_log(vec![make_pio(IoDirection::Write, 0x3f8, 0x49)]);

        let result = DeterminismVerifier::compare(&log_a, &log_b);
        assert!(!result.is_deterministic);
        let div = result.first_divergence.unwrap();
        assert!(div.description.contains("PIO data differs"));
        assert!(div.description.contains("0x3f8"));
    }

    #[test]
    fn host_timestamps_ignored() {
        let a = TraceEvent {
            seq: 1,
            host_ns: 100_000,
            pid: 1,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1000,
                info1: 0,
                info2: 0,
            },
        };
        let b = TraceEvent {
            seq: 999,
            host_ns: 999_999_999,
            pid: 1,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1000,
                info1: 0,
                info2: 0,
            },
        };
        let log_a = make_log(vec![a]);
        let log_b = make_log(vec![b]);

        let result = DeterminismVerifier::compare(&log_a, &log_b);
        assert!(result.is_deterministic);
    }

    #[test]
    fn filtered_comparison() {
        let events_a = vec![
            make_exit(12, 0x1000),
            make_pio(IoDirection::Write, 0x3f8, 0x48),
            make_exit(12, 0x2000),
        ];
        // Same PIO but different exits
        let events_b = vec![
            make_exit(7, 0x1000), // different reason
            make_pio(IoDirection::Write, 0x3f8, 0x48),
            make_exit(12, 0x2000),
        ];
        let log_a = make_log(events_a);
        let log_b = make_log(events_b);

        // Full comparison: not deterministic
        let result = DeterminismVerifier::compare(&log_a, &log_b);
        assert!(!result.is_deterministic);

        // PIO-only comparison: deterministic
        let result = DeterminismVerifier::compare_filtered(&log_a, &log_b, &|e| {
            matches!(e.kind, EventKind::KvmPio { .. })
        });
        assert!(result.is_deterministic);
    }
}
