//! Process-scoped guest supervisor commands shared by the host and SDK.

pub const PROCESS_FAULT_SCHEMA: &str = "chaoscontrol.process-fault.v1";
pub const MAX_PROCESS_ID_BYTES: usize = 96;
pub const MAX_PROCESS_FAULT_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessFaultAction {
    Kill,
    Pause,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessFaultCommand {
    pub schema: String,
    pub request_id: String,
    pub target: String,
    pub action: ProcessFaultAction,
    pub pause_ticks: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessFaultCommandError {
    InvalidSchema,
    InvalidRequestId,
    InvalidTarget,
    InvalidPauseTicks,
    PayloadLimit,
}

impl ProcessFaultCommand {
    pub fn new(
        request_id: impl Into<String>,
        target: impl Into<String>,
        action: ProcessFaultAction,
        pause_ticks: Option<u64>,
    ) -> Result<Self, ProcessFaultCommandError> {
        let command = Self {
            schema: PROCESS_FAULT_SCHEMA.to_string(),
            request_id: request_id.into(),
            target: target.into(),
            action,
            pause_ticks,
        };
        command.validate()?;
        Ok(command)
    }

    pub fn validate(&self) -> Result<(), ProcessFaultCommandError> {
        if self.schema != PROCESS_FAULT_SCHEMA {
            return Err(ProcessFaultCommandError::InvalidSchema);
        }
        validate_process_token(&self.request_id)
            .then_some(())
            .ok_or(ProcessFaultCommandError::InvalidRequestId)?;
        validate_process_token(&self.target)
            .then_some(())
            .ok_or(ProcessFaultCommandError::InvalidTarget)?;
        match (self.action, self.pause_ticks) {
            (ProcessFaultAction::Pause, Some(ticks)) if ticks > 0 => {}
            (ProcessFaultAction::Pause, _) => {
                return Err(ProcessFaultCommandError::InvalidPauseTicks);
            }
            (ProcessFaultAction::Kill | ProcessFaultAction::Restart, None) => {}
            (ProcessFaultAction::Kill | ProcessFaultAction::Restart, Some(_)) => {
                return Err(ProcessFaultCommandError::InvalidPauseTicks);
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|_| ProcessFaultCommandError::PayloadLimit)?;
        if bytes.len() > MAX_PROCESS_FAULT_BYTES {
            return Err(ProcessFaultCommandError::PayloadLimit);
        }
        Ok(())
    }
}

pub fn validate_process_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROCESS_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAUSE_TICKS: u64 = 4;

    #[test]
    fn valid_commands_round_trip() {
        let command = ProcessFaultCommand::new(
            "request-1",
            "writer",
            ProcessFaultAction::Pause,
            Some(PAUSE_TICKS),
        )
        .unwrap();
        let encoded = serde_json::to_vec(&command).unwrap();
        let decoded: ProcessFaultCommand = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(command, decoded);
        decoded.validate().unwrap();
    }

    #[test]
    fn malformed_target_and_pause_fail_closed() {
        assert_eq!(
            ProcessFaultCommand::new("request-1", "bad target", ProcessFaultAction::Kill, None),
            Err(ProcessFaultCommandError::InvalidTarget)
        );
        assert_eq!(
            ProcessFaultCommand::new("request-1", "writer", ProcessFaultAction::Pause, None),
            Err(ProcessFaultCommandError::InvalidPauseTicks)
        );
        assert_eq!(
            ProcessFaultCommand::new(
                "request-1",
                "writer",
                ProcessFaultAction::Restart,
                Some(PAUSE_TICKS),
            ),
            Err(ProcessFaultCommandError::InvalidPauseTicks)
        );
    }
}
