//! Bounded outcome classification for restore-probe seeks.
//!
//! The classification is descriptive: it names why the seek stopped. It is not a
//! replay verdict and does not imply the guest reached a failure.

/// Why a seek stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekOutcome {
    /// No target tick was requested; only the requested rounds ran.
    NoTarget,
    /// The target tick was reached.
    Reached,
    /// The requested round bound was exhausted before the target.
    RoundCap,
    /// The target was not reached because no VM was running.
    AllVmsStopped,
}

impl SeekOutcome {
    /// Stable lower-snake-case label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoTarget => "no_target",
            Self::Reached => "reached",
            Self::RoundCap => "round_cap",
            Self::AllVmsStopped => "all_vms_stopped",
        }
    }
}

// r[impl chaoscontrol.agent_investigation.restore_seek_outcome]
/// Classify a completed seek.
///
/// `rounds_run` counts executed rounds; `rounds_requested` is the bounded cap.
/// `vms_running` is the final running count. A reached target always wins.
pub fn classify(
    target: Option<u64>,
    reached: bool,
    rounds_run: u64,
    rounds_requested: u64,
    vms_running: u32,
) -> SeekOutcome {
    if reached {
        return SeekOutcome::Reached;
    }
    if target.is_none() {
        return SeekOutcome::NoTarget;
    }
    if vms_running == 0 {
        return SeekOutcome::AllVmsStopped;
    }
    if rounds_run >= rounds_requested {
        return SeekOutcome::RoundCap;
    }
    SeekOutcome::NoTarget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reached_target_wins() {
        assert_eq!(classify(Some(10), true, 2, 100, 0), SeekOutcome::Reached);
    }

    #[test]
    fn missing_target_is_distinct_from_round_cap() {
        assert_eq!(classify(None, false, 5, 5, 2), SeekOutcome::NoTarget);
        assert_eq!(classify(Some(10), false, 5, 5, 2), SeekOutcome::RoundCap);
    }

    #[test]
    fn stopped_vms_are_distinct() {
        assert_eq!(
            classify(Some(10), false, 1, 5, 0),
            SeekOutcome::AllVmsStopped
        );
    }
}
