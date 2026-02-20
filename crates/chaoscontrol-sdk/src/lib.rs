//! Guest-side SDK for ChaosControl deterministic simulation testing.
//!
//! This crate provides an [Antithesis](https://antithesis.com)-style API for
//! annotating software under test with **assertions**, **lifecycle events**,
//! and **guided randomness**.  The VMM collects these signals to drive fault
//! injection and property checking across thousands of deterministic runs.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::{assert, lifecycle, random};
//!
//! // Signal that setup is done — faults may begin
//! lifecycle::setup_complete(&[("nodes", "3")]);
//!
//! // Property: leader must always be valid when checked
//! assert::always(leader_id < num_nodes, "valid leader", &[
//!     ("leader", &leader_id.to_string()),
//! ]);
//!
//! // Property: eventually at least one write succeeds
//! assert::sometimes(write_ok, "write succeeded", &[]);
//!
//! // Guided random: VMM controls this for exploration
//! let action = random::random_choice(3); // 0, 1, or 2
//! ```
//!
//! # Transport
//!
//! Communication with the VMM uses a shared memory page at a fixed
//! guest-physical address plus an I/O port trigger.  See
//! [`chaoscontrol_protocol`] for the wire format.
//!
//! # `no_std` support
//!
//! The SDK is `no_std` by default for bare-metal guests and initrd
//! programs.  Enable the `std` feature for Linux userspace transport
//! (via `/dev/mem` + `iopl`).

#![cfg_attr(not(feature = "std"), no_std)]

pub mod assert;
pub mod coverage;
#[cfg(feature = "std")]
pub mod kcov;
pub mod lifecycle;
pub mod random;
mod transport;
