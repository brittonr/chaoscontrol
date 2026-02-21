//! Lifecycle events for coordinating test phases.
//!
//! These signals tell the VMM about the state of the workload, enabling
//! it to begin fault injection at the right time and to correlate events
//! across multiple VMs.

use crate::transport;
use chaoscontrol_protocol::*;

/// Signal that workload setup is complete and testing may begin.
///
/// The VMM will not inject faults before this signal, ensuring that
/// initialization code runs without interference.  Call this after all
/// services are started and ready to handle requests.
///
/// # Example
///
/// ```rust,ignore
/// use serde_json::json;
/// start_raft_cluster(&config);
/// wait_for_leader_election();
/// chaoscontrol_sdk::lifecycle::setup_complete(&json!({
///     "nodes": 3,
///     "protocol": "raft",
/// }));
/// // Faults may now be injected
/// ```
#[cfg(feature = "full")]
pub fn setup_complete(details: &serde_json::Value) {
    let json_bytes = serde_json::to_vec(details).unwrap_or_else(|_| b"{}".to_vec());
    transport::hypercall(
        CMD_LIFECYCLE_SETUP_COMPLETE,
        0,
        0,
        "setup_complete",
        &json_bytes,
    );
}

/// No-op stub for `setup_complete` when SDK features are disabled.
#[cfg(not(feature = "full"))]
pub fn setup_complete(_details: &()) {
    transport::hypercall(CMD_LIFECYCLE_SETUP_COMPLETE, 0, 0, "setup_complete", b"{}");
}

/// Emit a named structured event.
///
/// Events are recorded in the execution trace and can be used to
/// correlate behavior across VMs and across runs.  Use them to mark
/// significant application-level milestones.
///
/// # Example
///
/// ```rust,ignore
/// use serde_json::json;
/// chaoscontrol_sdk::lifecycle::send_event("leader_elected", &json!({
///     "node_id": 2,
///     "term": 5,
/// }));
/// ```
#[cfg(feature = "full")]
pub fn send_event(name: &str, details: &serde_json::Value) {
    let id = crate::assert::location_id(name);
    let json_bytes = serde_json::to_vec(details).unwrap_or_else(|_| b"{}".to_vec());
    transport::hypercall(CMD_LIFECYCLE_SEND_EVENT, 0, id, name, &json_bytes);
}

/// No-op stub for `send_event` when SDK features are disabled.
#[cfg(not(feature = "full"))]
pub fn send_event(_name: &str, _details: &()) {
    // No-op: transport discards everything
}
