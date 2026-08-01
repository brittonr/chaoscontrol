//! Host-side fault injection engine and property oracle for ChaosControl.
//!
//! This crate provides three main components:
//!
//! 1. **[`faults`]** — Fault type definitions (network, disk, process, clock)
//! 2. **[`engine`]** — Fault injection scheduler that fires faults at
//!    deterministic (seeded) times
//! 3. **[`oracle`]** — Property oracle that tracks assertion satisfaction
//!    across multiple runs and produces test verdicts
//!
//! # Architecture
//!
//! ```text
//! Guest SDK              VMM run loop            Fault Engine
//! ─────────              ────────────            ────────────
//! bound assertion ───→ handle_hypercall() ──→ oracle.record_bound_event()
//! random::get()    ──→ handle_hypercall() ──→ engine.next_random()
//!                      step() exit loop   ──→ engine.maybe_inject()
//! ```

pub mod engine;
pub mod faults;
pub mod oracle;
mod oracle_event_validation;
mod oracle_record_validation;
mod oracle_snapshot_validation;
pub use oracle_snapshot_validation::resolve_snapshot_assertion_evidence;
pub mod oracle_validation;
pub mod report_merge;
pub mod scenario;
pub mod schedule;
