//! Verified pure functions for the ChaosControl VMM.
//!
//! This module contains pure, deterministic functions extracted from the
//! imperative shell for formal verification with [Verus]. Each sub-module
//! corresponds to a domain (CPU, memory, PIT) and contains only functions
//! with **no I/O, no async, and no external state mutation**.
//!
//! # Why a separate module?
//!
//! Formal verification tools like Verus work best on small, pure functions
//! that take values in and return values out.  By extracting the arithmetic
//! core of each subsystem here, we get:
//!
//! 1. A clear boundary between *verified logic* and *effectful shell*.
//! 2. Functions that can be tested with standard `#[test]` today and
//!    proven correct with Verus `requires`/`ensures` tomorrow.
//! 3. No transitive dependency on `kvm-ioctls`, `vm-memory`, etc.
//!
//! # Corresponding Verus specs
//!
//! Formal specifications live in `verus/cpu_spec.rs` (and future
//! `verus/memory_spec.rs`, `verus/pit_spec.rs`).  They are not compiled
//! by `cargo` — they are consumed by the Verus verifier separately.
//!
//! [Verus]: https://github.com/verus-lang/verus

pub mod block;
pub mod cpu;
pub mod entropy;
pub mod memory;
pub mod net;
pub mod pit;
