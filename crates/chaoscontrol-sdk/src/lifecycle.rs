//! Lifecycle events for coordinating test phases.
//!
//! These signals tell the VMM about the state of the workload, enabling
//! it to begin fault injection at the right time and to correlate events
//! across multiple VMs.

use chaoscontrol_protocol::*;
use crate::transport;

/// Signal that workload setup is complete and testing may begin.
///
/// The VMM will not inject faults before this signal, ensuring that
/// initialization code runs without interference.  Call this after all
/// services are started and ready to handle requests.
///
/// # Example
///
/// ```rust,ignore
/// start_raft_cluster(&config);
/// wait_for_leader_election();
/// chaoscontrol_sdk::lifecycle::setup_complete(&[
///     ("nodes", "3"),
///     ("protocol", "raft"),
/// ]);
/// // Faults may now be injected
/// ```
pub fn setup_complete(details: &[(&str, &str)]) {
    transport::hypercall(CMD_LIFECYCLE_SETUP_COMPLETE, 0, 0, "setup_complete", details);
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
/// chaoscontrol_sdk::lifecycle::send_event("leader_elected", &[
///     ("node_id", "2"),
///     ("term", "5"),
/// ]);
/// ```
pub fn send_event(name: &str, details: &[(&str, &str)]) {
    let id = crate::assert::location_id(name);
    transport::hypercall(CMD_LIFECYCLE_SEND_EVENT, 0, id, name, details);
}
