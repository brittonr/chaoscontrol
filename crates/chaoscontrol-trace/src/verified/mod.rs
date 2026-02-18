//! Verified pure functions for the ChaosControl trace system.
//!
//! This module contains pure, deterministic functions extracted from the
//! imperative event parsing and verification modules for formal verification
//! with [Verus](https://github.com/verus-lang/verus).  Each sub-module
//! corresponds to a domain (events, verifier) and contains only functions
//! with **no I/O, no async, and no external state mutation**.
//!
//! # Why a separate module?
//!
//! Formal verification tools like Verus work best on small, pure functions
//! that take values in and return values out.  By extracting the parsing and
//! comparison core here, we get:
//!
//! 1. A clear boundary between *verified logic* and *effectful shell*.
//! 2. Functions that can be tested with standard `#[test]` today and
//!    proven correct with Verus `requires`/`ensures` tomorrow.
//! 3. No transitive dependency on `libbpf-rs`, `serde`, etc.
//!
//! # Corresponding Verus specs
//!
//! Formal specifications live in `verus/events_spec.rs`.  They are not
//! compiled by `cargo` — they are consumed by the Verus verifier separately.
//!
//! [Verus]: https://github.com/verus-lang/verus

pub mod events;
pub mod verifier;
