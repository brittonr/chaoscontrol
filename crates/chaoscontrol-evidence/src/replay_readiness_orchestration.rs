//! Process and clock shell for replay-readiness orchestration.

// r[impl chaoscontrol.architecture_modules.evidence]

use std::process::{Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

/// Execute one trusted operator-authored command without a login shell.
pub(crate) fn run_plan_command(command: &str) -> std::io::Result<ExitStatus> {
    Command::new("sh").arg("-c").arg(command).status()
}

/// Observe the host wall clock for receipt metadata.
#[allow(unknown_lints)]
#[allow(
    ambient_clock,
    reason = "receipt writer shell timestamps bounded local scheduler evidence"
)]
pub(crate) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_command_success_is_observed() {
        let status = run_plan_command("exit 0").expect("execute successful command");
        assert!(status.success());
    }

    #[test]
    fn trusted_command_failure_is_not_promoted() {
        const EXPECTED_FAILURE_CODE: i32 = 7;
        const FAILURE_COMMAND: &str = "exit 7";
        let status = run_plan_command(FAILURE_COMMAND).expect("execute failed command");
        assert!(!status.success());
        assert_eq!(status.code(), Some(EXPECTED_FAILURE_CODE));
    }
}
