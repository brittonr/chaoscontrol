//! Process-scoped supervisor control over the SDK hypercall transport.

use chaoscontrol_protocol::process::ProcessFaultCommand;

/// Poll one host-directed process fault command.
///
/// An empty queue returns `Ok(None)`. Malformed commands fail closed.
pub fn poll_fault() -> Result<Option<ProcessFaultCommand>, String> {
    let (present, status, payload) =
        crate::transport::hypercall_response(chaoscontrol_protocol::CMD_PROCESS_FAULT_POLL);
    if status != chaoscontrol_protocol::STATUS_OK {
        return Err(format!("process fault poll failed with status {status}"));
    }
    if present == 0 {
        return Ok(None);
    }
    let command: ProcessFaultCommand =
        serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
    command.validate().map_err(|error| format!("{error:?}"))?;
    Ok(Some(command))
}
