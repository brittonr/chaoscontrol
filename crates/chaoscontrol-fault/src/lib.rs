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
//! assert::always() ──→ handle_hypercall() ──→ oracle.record_always()
//! random::get()    ──→ handle_hypercall() ──→ engine.next_random()
//!                      step() exit loop   ──→ engine.maybe_inject()
//! ```

pub mod engine;
pub mod faults;
pub mod oracle;
pub mod schedule;
