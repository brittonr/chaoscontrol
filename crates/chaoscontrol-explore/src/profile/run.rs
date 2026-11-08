use serde::{Deserialize, Serialize};

use super::{checked_usize, valid_identifier, ArtifactReference};
use crate::explorer::{ExplorationMode, ExplorerConfig, FrontierPolicy};
use crate::mutator::MutationConfig;
use chaoscontrol_vmm::controller::DEFAULT_RANDOM_FAULTS;
use chaoscontrol_vmm::scheduler::core::{ProgressBudget, ProgressMode};
use chaoscontrol_vmm::scheduler::SchedulingStrategy;
use chaoscontrol_vmm::vm::VmConfig;

const RUN_SCHEMA: &str = "chaoscontrol.run-profile.v3";
const RUN_MODE: &str = "vm-exploration";
const RUN_SCOPE: &str = "pre-run VM exploration intent; not KVM availability, guest correctness, deterministic replay, fault effect, campaign completion, or accepted evidence";
const BYTES_PER_MIB: u64 = 1024 * 1024;
const MAX_VMS: u64 = 64;
const MAX_VCPUS: u64 = 64;
const MAX_GUEST_CMDLINE_BYTES: usize = 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunProfile {
    pub schema: String,
    pub profile_id: String,
    pub mode: String,
    pub progress_mode: ProgressMode,
    pub artifacts: RunArtifacts,
    pub guest_cmdline: Option<String>,
    pub seed: u64,
    pub topology: RunTopology,
    pub exploration: ExplorationProfile,
    pub coverage: CoverageProfile,
    pub logging: LoggingProfile,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunArtifacts {
    pub kernel: ArtifactReference,
    pub initrd: Option<ArtifactReference>,
    pub disk: Option<ArtifactReference>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunTopology {
    pub num_vms: u64,
    pub num_vcpus: u64,
    pub memory_mib: u64,
    pub scheduling: SchedulingProfile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulingProfile {
    pub strategy: SchedulingMode,
    pub minimum_budget: ProgressBudget,
    pub maximum_budget: ProgressBudget,
    pub journal_limit: usize,
    pub entry_watchdog_microseconds: u64,
    pub diversity: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulingMode {
    RoundRobin,
    Randomized,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationProfile {
    pub mode: ExplorationProfileMode,
    pub branch_factor: u64,
    pub ticks_per_branch: u64,
    pub max_rounds: u64,
    pub max_frontier: u64,
    /// Outer controller KVM-exit bound, not scheduler instruction/boundary units.
    pub outer_exit_budget: u64,
    pub progress_budget: ProgressBudget,
    pub bootstrap_budget: u64,
    pub stale_round_limit: u64,
    /// Frontier policy. Absent means the default Campaign policy; the legacy
    /// frontier runs only when a profile names `legacy-diagnostic`
    /// explicitly. Unknown and retired names (`legacy`, `campaign-candidate`)
    /// are rejected.
    #[serde(default, skip_serializing_if = "FrontierPolicy::is_campaign")]
    pub frontier_policy: FrontierPolicy,
    /// Whether every system run injects seed-derived random faults. Absent
    /// means the system-run default, which is on; `false` is the explicit
    /// opt-out and is the only value written back.
    #[serde(
        default = "default_random_faults",
        skip_serializing_if = "is_default_random_faults"
    )]
    pub random_faults: bool,
}

const fn default_random_faults() -> bool {
    DEFAULT_RANDOM_FAULTS
}

/// `skip_serializing_if` predicate (serde passes a reference).
const fn is_default_random_faults(random_faults: &bool) -> bool {
    *random_faults == DEFAULT_RANDOM_FAULTS
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExplorationProfileMode {
    FaultSchedule,
    InputTree,
    Hybrid,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageProfile {
    pub mode: CoverageMode,
    pub bitmap_gpa: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageMode {
    Blind,
    Bitmap,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingProfile {
    pub raw_log_policy: RawLogPolicy,
    pub determinism_log_policy: DeterminismLogPolicy,
    pub determinism_log_output: Option<ArtifactReference>,
    pub register_interval: u64,
    pub memory_hash: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RawLogPolicy {
    Disabled,
    DebugOnlyExcludedFromGit,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DeterminismLogPolicy {
    Disabled,
    Bounded,
}

impl RunProfile {
    pub fn try_into_explorer_config(
        self,
        seed: u64,
        output_dir: Option<String>,
    ) -> Result<ExplorerConfig, String> {
        self.validate()?;
        let strategy = match self.topology.scheduling.strategy {
            SchedulingMode::RoundRobin => SchedulingStrategy::RoundRobin,
            SchedulingMode::Randomized => SchedulingStrategy::Randomized {
                min_budget: self.topology.scheduling.minimum_budget,
                max_budget: self.topology.scheduling.maximum_budget,
            },
        };
        let memory_bytes = self
            .topology
            .memory_mib
            .checked_mul(BYTES_PER_MIB)
            .ok_or_else(|| "profile memory byte count overflow".to_string())?;
        let mutation = MutationConfig {
            num_vms: checked_usize("num_vms", self.topology.num_vms)?,
            max_tick: self.exploration.ticks_per_branch,
            base_progress_budget: self.exploration.progress_budget,
            ..MutationConfig::default()
        };
        let dlog_dir = self
            .logging
            .determinism_log_output
            .map(|reference| std::path::PathBuf::from(reference.path));
        let mut vm_config = VmConfig {
            memory_size: checked_usize("memory bytes", memory_bytes)?,
            num_vcpus: checked_usize("num_vcpus", self.topology.num_vcpus)?,
            scheduling_strategy: strategy,
            smp_progress_mode: self.progress_mode,
            smp_progress_budget: self.exploration.progress_budget,
            smp_schedule_journal_limit: self.topology.scheduling.journal_limit,
            smp_entry_watchdog_microseconds: self.topology.scheduling.entry_watchdog_microseconds,
            extra_cmdline: self.guest_cmdline,
            dlog_register_interval: self.logging.register_interval,
            dlog_memory_hash: self.logging.memory_hash,
            ..VmConfig::default()
        };
        vm_config.cpu.seed = seed;
        if let ProgressBudget::KvmBoundaries { profile_id, .. } = self.exploration.progress_budget {
            vm_config
                .apply_kvm_debug_boundary_profile(profile_id)
                .map_err(|error| error.to_string())?;
        }
        Ok(ExplorerConfig {
            num_vms: checked_usize("num_vms", self.topology.num_vms)?,
            vm_config,
            kernel_path: self.artifacts.kernel.path,
            initrd_path: self.artifacts.initrd.map(|reference| reference.path),
            seed,
            branch_factor: checked_usize("branch_factor", self.exploration.branch_factor)?,
            ticks_per_branch: self.exploration.ticks_per_branch,
            max_rounds: self.exploration.max_rounds,
            max_frontier: checked_usize("max_frontier", self.exploration.max_frontier)?,
            quantum: self.exploration.outer_exit_budget,
            scheduling_strategy: strategy,
            mutation,
            exploration_mode: match self.exploration.mode {
                ExplorationProfileMode::FaultSchedule => ExplorationMode::FaultSchedule,
                ExplorationProfileMode::InputTree => ExplorationMode::InputTree,
                ExplorationProfileMode::Hybrid => ExplorationMode::Hybrid,
            },
            coverage_gpa: self.coverage.bitmap_gpa,
            output_dir,
            disk_image_path: self.artifacts.disk.map(|reference| reference.path),
            bootstrap_budget: self.exploration.bootstrap_budget,
            dlog_dir,
            dlog_register_interval: self.logging.register_interval,
            dlog_memory_hash: self.logging.memory_hash,
            num_workers: 1,
            stale_round_limit: self.exploration.stale_round_limit,
            schedule_diversity: self.topology.scheduling.diversity,
            frontier_policy: self.exploration.frontier_policy,
            random_faults: self.exploration.random_faults,
            ..ExplorerConfig::default()
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != RUN_SCHEMA
            || self.mode != RUN_MODE
            || self.scope != RUN_SCOPE
            || !valid_identifier(&self.profile_id)
            || self.guest_cmdline.as_ref().is_some_and(|value| {
                value.is_empty()
                    || value.len() > MAX_GUEST_CMDLINE_BYTES
                    || value.bytes().any(|byte| byte.is_ascii_control())
            })
        {
            return Err(
                "run profile schema, identity, command line, or scope is invalid".to_string(),
            );
        }
        self.artifacts.kernel.validate()?;
        for reference in [&self.artifacts.initrd, &self.artifacts.disk]
            .into_iter()
            .flatten()
        {
            reference.validate()?;
        }
        if let Some(reference) = &self.logging.determinism_log_output {
            reference.validate()?;
        }
        if self.topology.num_vms == 0
            || self.topology.num_vms > MAX_VMS
            || self.topology.num_vcpus == 0
            || self.topology.num_vcpus > MAX_VCPUS
            || self.topology.memory_mib == 0
            || self.exploration.branch_factor == 0
            || self.exploration.ticks_per_branch == 0
            || self.exploration.max_rounds == 0
            || self.exploration.max_frontier == 0
            || self.exploration.outer_exit_budget == 0
            || self.exploration.progress_budget.count() == 0
            || self.exploration.bootstrap_budget == 0
        {
            return Err("run profile topology or exploration bound is invalid".to_string());
        }
        let scheduling = &self.topology.scheduling;
        let budget = self.exploration.progress_budget;
        let mode_matches = match (self.progress_mode, budget) {
            (ProgressMode::KvmDebugBoundary, ProgressBudget::KvmBoundaries { .. }) => {
                self.topology.num_vcpus >= 2
            }
            (
                ProgressMode::ExactSingleStep | ProgressMode::PmuAccelerated { .. },
                ProgressBudget::Instructions(_),
            ) => true,
            _ => false,
        };
        if !mode_matches
            || !budget.same_unit(scheduling.minimum_budget)
            || !budget.same_unit(scheduling.maximum_budget)
            || scheduling.minimum_budget.count() == 0
            || scheduling.minimum_budget.count() > scheduling.maximum_budget.count()
            || (scheduling.strategy == SchedulingMode::Randomized
                && scheduling.minimum_budget.count() == scheduling.maximum_budget.count())
            || scheduling.journal_limit == 0
            || scheduling.entry_watchdog_microseconds == 0
            || scheduling.entry_watchdog_microseconds > i64::MAX as u64 / 1000
            || (scheduling.strategy == SchedulingMode::RoundRobin
                && (scheduling.minimum_budget != scheduling.maximum_budget
                    || scheduling.minimum_budget != budget))
            || (self.topology.num_vcpus == 1
                && (scheduling.strategy != SchedulingMode::RoundRobin || scheduling.diversity))
        {
            return Err("run profile scheduling is incompatible with topology".to_string());
        }
        if let ProgressMode::PmuAccelerated { exact_step_margin } = self.progress_mode {
            if exact_step_margin == 0 || exact_step_margin >= scheduling.minimum_budget.count() {
                return Err("run profile PMU margin must fit every instruction budget".to_string());
            }
        }
        if self.progress_mode == ProgressMode::KvmDebugBoundary {
            let cmdline = self.guest_cmdline.as_deref().unwrap_or("");
            if !["noapictimer", "clocksource=jiffies", "notsc"]
                .iter()
                .all(|required| {
                    cmdline
                        .split_ascii_whitespace()
                        .any(|word| word == *required)
                })
            {
                return Err(
                    "coarse run profile requires noapictimer clocksource=jiffies notsc".to_string(),
                );
            }
        }
        if (self.coverage.mode == CoverageMode::Blind) != (self.coverage.bitmap_gpa == 0) {
            return Err("run profile coverage mode and address conflict".to_string());
        }
        let logging_disabled =
            self.logging.determinism_log_policy == DeterminismLogPolicy::Disabled;
        if logging_disabled
            != (self.logging.determinism_log_output.is_none()
                && self.logging.register_interval == 0
                && !self.logging.memory_hash)
        {
            return Err("run profile determinism log fields conflict".to_string());
        }
        Ok(())
    }
}
