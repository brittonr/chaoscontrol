use chaoscontrol_vmm::scheduler::core::{
    BoundaryProfileId, ProgressBudget, ProgressMode, DEFAULT_SCHEDULE_JOURNAL_LIMIT,
};
use chaoscontrol_vmm::vm::{VmConfig, VmError};

const COARSE_ENTRY_WATCHDOG_MICROSECONDS: u64 = 250_000;

/// Diagnostic selection only: strict remains the default and never downgrades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmpProgressSelection {
    pub mode: ProgressMode,
    pub budget: ProgressBudget,
}

impl SmpProgressSelection {
    pub fn parse(
        mode: Option<&str>,
        profile: Option<&str>,
        boundary_quantum: Option<u64>,
    ) -> Result<Self, String> {
        match mode.unwrap_or("exact-single-step") {
            "exact-single-step" if boundary_quantum.is_none() && profile.is_none() => {
                let config = VmConfig::default();
                Ok(Self {
                    mode: ProgressMode::ExactSingleStep,
                    budget: config.smp_progress_budget,
                })
            }
            "exact-single-step" => Err(
                "boundary profile and quantum require --smp-progress-mode kvm-debug-boundary".to_string(),
            ),
            "kvm-debug-boundary" => {
                if profile != Some("kvm-debug-exit-boundary-v1") {
                    return Err("kvm-debug-boundary requires --smp-boundary-profile kvm-debug-exit-boundary-v1".to_string());
                }
                let count = boundary_quantum.ok_or_else(|| {
                    "kvm-debug-boundary requires --smp-boundary-quantum".to_string()
                })?;
                if count == 0 {
                    return Err("--smp-boundary-quantum must be positive".to_string());
                }
                Ok(Self {
                    mode: ProgressMode::KvmDebugBoundary,
                    budget: ProgressBudget::KvmBoundaries {
                        profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                        count,
                    },
                })
            }
            other => Err(format!(
                "unknown --smp-progress-mode {other:?}; expected exact-single-step or kvm-debug-boundary"
            )),
        }
    }

    pub fn is_coarse(self) -> bool {
        self.mode == ProgressMode::KvmDebugBoundary
    }

    pub fn apply(self, config: &mut VmConfig) -> Result<(), VmError> {
        if config.num_vcpus <= 1 {
            return Ok(());
        }
        config.smp_progress_mode = self.mode;
        config.smp_progress_budget = self.budget;
        if self.is_coarse() {
            config.smp_entry_watchdog_microseconds = COARSE_ENTRY_WATCHDOG_MICROSECONDS;
            config.smp_schedule_journal_limit = DEFAULT_SCHEDULE_JOURNAL_LIMIT;
            config.apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)?;
        }
        Ok(())
    }

    pub fn describe(self) -> String {
        if self.is_coarse() {
            format!(
                "kvm-debug-boundary profile={} boundary-quantum={} (diagnostic only; not exact instructions or release eligibility)",
                BoundaryProfileId::KvmDebugExitBoundaryV1.name(),
                self.budget.count(),
            )
        } else {
            format!(
                "exact-single-step instruction-quantum={}",
                self.budget.count()
            )
        }
    }
}

pub fn parse_boundary_quantum(value: &str) -> Result<u64, String> {
    let count = value
        .parse::<u64>()
        .map_err(|_| "--smp-boundary-quantum requires a positive u64".to_string())?;
    if count == 0 {
        return Err("--smp-boundary-quantum must be positive".to_string());
    }
    Ok(count)
}

pub fn parse_progress_args(
    args: impl IntoIterator<Item = String>,
    allow_other_options: bool,
) -> Result<(Vec<String>, SmpProgressSelection), String> {
    let mut positional = Vec::new();
    let mut mode = None;
    let mut quantum = None;
    let mut profile = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smp-progress-mode" => {
                if mode.is_some() {
                    return Err("duplicate --smp-progress-mode".to_string());
                }
                mode = Some(args.next().ok_or("--smp-progress-mode requires a value")?);
            }
            "--smp-boundary-profile" => {
                if profile.is_some() {
                    return Err("duplicate --smp-boundary-profile".to_string());
                }
                profile = Some(
                    args.next()
                        .ok_or("--smp-boundary-profile requires a profile")?,
                );
            }
            "--smp-boundary-quantum" => {
                if quantum.is_some() {
                    return Err("duplicate --smp-boundary-quantum".to_string());
                }
                quantum = Some(parse_boundary_quantum(
                    &args
                        .next()
                        .ok_or("--smp-boundary-quantum requires a count")?,
                )?);
            }
            _ if arg.starts_with('-') && !allow_other_options => {
                return Err(format!("unknown option: {arg}"))
            }
            _ => positional.push(arg),
        }
    }
    Ok((
        positional,
        SmpProgressSelection::parse(mode.as_deref(), profile.as_deref(), quantum)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_flags_parse_without_host_or_guest_access() {
        let (remaining, progress) = parse_progress_args(
            [
                "kernel",
                "initrd",
                "--smp-progress-mode",
                "kvm-debug-boundary",
                "--smp-boundary-profile",
                "kvm-debug-exit-boundary-v1",
                "--smp-boundary-quantum",
                "37",
            ]
            .map(str::to_string),
            false,
        )
        .unwrap();
        assert_eq!(remaining, ["kernel", "initrd"]);
        assert!(progress.is_coarse());
        assert_eq!(progress.budget.count(), 37);
    }

    #[test]
    fn missing_duplicate_unknown_and_mixed_flags_fail_closed() {
        for args in [
            vec!["--smp-progress-mode"],
            vec!["--smp-boundary-profile"],
            vec!["--smp-boundary-quantum"],
            vec!["--smp-progress-mode", "kvm-debug-boundary"],
            vec!["--smp-boundary-profile", "kvm-debug-exit-boundary-v1"],
            vec!["--smp-boundary-quantum", "37"],
            vec![
                "--smp-progress-mode",
                "exact-single-step",
                "--smp-boundary-profile",
                "kvm-debug-exit-boundary-v1",
            ],
            vec![
                "--smp-progress-mode",
                "exact-single-step",
                "--smp-progress-mode",
                "kvm-debug-boundary",
            ],
            vec![
                "--smp-boundary-profile",
                "kvm-debug-exit-boundary-v1",
                "--smp-boundary-profile",
                "kvm-debug-exit-boundary-v1",
            ],
            vec![
                "--smp-boundary-quantum",
                "37",
                "--smp-boundary-quantum",
                "38",
            ],
            vec!["--unknown"],
        ] {
            assert!(parse_progress_args(args.into_iter().map(str::to_string), false).is_err());
        }
    }

    #[test]
    fn strict_default_and_explicit_coarse_have_distinct_units() {
        let strict = SmpProgressSelection::parse(None, None, None).unwrap();
        assert_eq!(strict.mode, ProgressMode::ExactSingleStep);
        assert!(matches!(strict.budget, ProgressBudget::Instructions(_)));
        let coarse = SmpProgressSelection::parse(
            Some("kvm-debug-boundary"),
            Some("kvm-debug-exit-boundary-v1"),
            Some(37),
        )
        .unwrap();
        assert_eq!(coarse.mode, ProgressMode::KvmDebugBoundary);
        assert_eq!(
            coarse.budget,
            ProgressBudget::KvmBoundaries {
                profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: 37,
            }
        );
    }

    #[test]
    fn boundary_quantum_never_selects_or_downgrades_strict_mode() {
        assert!(SmpProgressSelection::parse(None, None, Some(37)).is_err());
        assert!(SmpProgressSelection::parse(Some("exact-single-step"), None, Some(37)).is_err());
        assert!(SmpProgressSelection::parse(
            Some("kvm-debug-boundary"),
            Some("kvm-debug-exit-boundary-v1"),
            Some(0)
        )
        .is_err());
        assert!(SmpProgressSelection::parse(
            Some("kvm-debug-boundary"),
            Some("kvm-debug-exit-boundary-v1"),
            None
        )
        .is_err());
        assert!(SmpProgressSelection::parse(Some("kvm-debug-boundary"), None, Some(37)).is_err());
        assert!(
            SmpProgressSelection::parse(Some("kvm-debug-boundary"), Some("unknown"), Some(37))
                .is_err()
        );
        assert!(
            SmpProgressSelection::parse(None, Some("kvm-debug-exit-boundary-v1"), None).is_err()
        );
        assert!(SmpProgressSelection::parse(Some("legacy-wall-clock"), None, None).is_err());
        for value in ["0", "-1", "no", "18446744073709551616"] {
            assert!(parse_boundary_quantum(value).is_err());
        }
    }

    #[test]
    fn coarse_selection_changes_only_smp_and_selects_supported_clock() {
        let coarse = SmpProgressSelection::parse(
            Some("kvm-debug-boundary"),
            Some("kvm-debug-exit-boundary-v1"),
            Some(37),
        )
        .unwrap();
        let mut single = VmConfig::default();
        coarse.apply(&mut single).unwrap();
        assert_eq!(single.smp_progress_mode, ProgressMode::ExactSingleStep);
        assert!(single.extra_cmdline.is_none());
        let mut smp = VmConfig {
            num_vcpus: 2,
            ..VmConfig::default()
        };
        coarse.apply(&mut smp).unwrap();
        assert_eq!(smp.smp_progress_mode, ProgressMode::KvmDebugBoundary);
        assert_eq!(smp.smp_progress_budget, coarse.budget);
        assert!(smp
            .extra_cmdline
            .as_deref()
            .unwrap()
            .contains("noapictimer"));
        assert!(smp.cpu.hide_tsc);
        assert_eq!(
            smp.smp_entry_watchdog_microseconds,
            COARSE_ENTRY_WATCHDOG_MICROSECONDS
        );
        assert_eq!(
            smp.smp_schedule_journal_limit,
            DEFAULT_SCHEDULE_JOURNAL_LIMIT
        );
    }
}
