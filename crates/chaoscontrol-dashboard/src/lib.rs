//! Standalone dashboard binary for ChaosControl.
//!
//! This crate provides the `chaoscontrol-dashboard` binary.
//! The actual server implementation lives in `chaoscontrol_explore::server`
//! (enabled via the `dashboard` feature flag).

pub use chaoscontrol_explore::server;
