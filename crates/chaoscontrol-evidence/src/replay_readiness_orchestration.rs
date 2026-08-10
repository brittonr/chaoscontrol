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
