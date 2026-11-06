//! Fresh-process explorer repeatability gate.
//!
//! Runs the same bounded exploration configuration twice in separate processes,
//! loads both checkpoints, and compares per-round branch digests with the pure
//! [`crate::repeatability`] core. Operational rounds are excluded, never treated
//! as matches. A passing receipt covers only the recorded inputs.

use crate::bounded_json::read_checkpoint;
use crate::checkpoint::load_checkpoint;
use crate::explorer::RoundHistory;
use crate::repeatability::{compare, RepeatabilityVerdict};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Gate receipt schema identifier.
pub const REPEATABILITY_GATE_SCHEMA: &str = "chaoscontrol.explorer-repeatability-gate.v1";

/// Non-claims retained on every gate receipt.
pub const NON_CLAIMS: [&str; 4] = [
    "The receipt compares two bounded runs of one recorded configuration.",
    "A match does not prove determinism for other seeds, hosts, kernels, or workloads.",
    "Rounds with watchdog firings, discarded branches, or failed branches are excluded.",
    "No coverage-completeness, correctness, replay, or release-eligibility claim follows.",
];

/// One bounded exploration configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateConfig {
    /// Kernel path.
    pub kernel: String,
    /// Initrd path.
    pub initrd: String,
    /// Workload variant name (`none`, `premature_commit`, ...).
    pub workload: String,
    /// Extra guest cmdline, including any `raft_bug=` selector.
    pub extra_cmdline: String,
    /// Gate seed.
    pub seed: u64,
    /// Rounds per run.
    pub rounds: u64,
    /// Branches per round.
    pub branches: usize,
    /// Ticks per branch.
    pub ticks: u64,
    /// Bootstrap budget.
    pub bootstrap_budget: u64,
    /// VM count.
    pub vms: usize,
}

/// Facts retained for one run of the gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRunFacts {
    /// Output directory.
    pub output: String,
    /// BLAKE3 identity of the checkpoint bytes.
    pub checkpoint_digest: String,
    /// Rounds present in the checkpoint.
    pub rounds: usize,
    /// Branches recorded across rounds.
    pub branches: usize,
    /// Rounds excluded as operational.
    pub operational_rounds: usize,
    /// Rounds without branch digests.
    pub unavailable_rounds: usize,
}

/// Gate receipt binding both runs and the comparison verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatabilityGateReceipt {
    /// Receipt schema identifier.
    pub schema: String,
    /// Recorded configuration.
    pub config: GateConfig,
    /// First run facts.
    pub first: GateRunFacts,
    /// Second run facts.
    pub second: GateRunFacts,
    /// Comparison verdict.
    pub verdict: RepeatabilityVerdict,
    /// Explicit boundaries.
    pub non_claims: Vec<String>,
}

/// Gate failure before a receipt can be built.
#[derive(Debug)]
pub enum GateError {
    /// The explorer process could not be started or failed.
    Run(String),
    /// A checkpoint could not be read.
    Checkpoint(String),
    /// The receipt could not be written.
    Receipt(String),
}

impl core::fmt::Display for GateError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Run(message) => write!(formatter, "repeatability run failed: {message}"),
            Self::Checkpoint(message) => write!(formatter, "checkpoint read failed: {message}"),
            Self::Receipt(message) => write!(formatter, "receipt write failed: {message}"),
        }
    }
}

impl std::error::Error for GateError {}

fn run_facts(output: &Path, history: &[RoundHistory], digest: String) -> GateRunFacts {
    GateRunFacts {
        output: output.display().to_string(),
        checkpoint_digest: digest,
        rounds: history.len(),
        branches: history.iter().map(|round| round.branches_run).sum(),
        operational_rounds: history
            .iter()
            .filter(|round| {
                round.host_watchdog_timeouts > 0
                    || round.host_watchdog_discarded_branches > 0
                    || round.failed_branches > 0
            })
            .count(),
        unavailable_rounds: history
            .iter()
            .filter(|round| round.branch_digests.is_empty())
            .count(),
    }
}

fn load(output: &Path) -> Result<(Vec<RoundHistory>, String), GateError> {
    let checkpoint_path = output.join("checkpoint.json");
    let checkpoint = load_checkpoint(&checkpoint_path)
        .map_err(|error| GateError::Checkpoint(error.to_string()))?;
    let bytes = read_checkpoint(&checkpoint_path)
        .map_err(|error| GateError::Checkpoint(error.to_string()))?;
    let digest = format!("b3:{}", blake3::hash(bytes.as_bytes()).to_hex());
    Ok((checkpoint.round_history.unwrap_or_default(), digest))
}

fn command(explorer: &Path, config: &GateConfig, output: &Path) -> Command {
    let mut command = Command::new(explorer);
    command
        .arg("run")
        .arg("--kernel")
        .arg(&config.kernel)
        .arg("--initrd")
        .arg(&config.initrd)
        .arg("--vms")
        .arg(config.vms.to_string())
        .arg("--seed")
        .arg(config.seed.to_string())
        .arg("--rounds")
        .arg(config.rounds.to_string())
        .arg("--branches")
        .arg(config.branches.to_string())
        .arg("--ticks")
        .arg(config.ticks.to_string())
        .arg("--bootstrap-budget")
        .arg(config.bootstrap_budget.to_string())
        .arg("--output")
        .arg(output);
    if !config.extra_cmdline.is_empty() {
        command.arg("--extra-cmdline").arg(&config.extra_cmdline);
    }
    command
}

fn run_once(explorer: &Path, config: &GateConfig, output: &Path) -> Result<(), GateError> {
    std::fs::create_dir_all(output)
        .map_err(|error| GateError::Run(format!("create {}: {error}", output.display())))?;
    let status = command(explorer, config, output)
        .status()
        .map_err(|error| GateError::Run(format!("spawn explorer: {error}")))?;
    // Explorer exit 1 means safety bugs were found; the gate compares
    // trajectories, so only operational failures (2, 3) and signals fail here.
    if matches!(status.code(), Some(0 | 1)) {
        Ok(())
    } else {
        Err(GateError::Run(format!(
            "explorer exited with {status} for {}",
            output.display()
        )))
    }
}

/// Run both gate passes in fresh processes and compare their checkpoints.
pub fn run_gate(
    explorer: &Path,
    config: &GateConfig,
    first_output: &Path,
    second_output: &Path,
) -> Result<RepeatabilityGateReceipt, GateError> {
    run_once(explorer, config, first_output)?;
    run_once(explorer, config, second_output)?;
    let (first_history, first_digest) = load(first_output)?;
    let (second_history, second_digest) = load(second_output)?;
    let verdict = compare(&first_history, &second_history);
    Ok(RepeatabilityGateReceipt {
        schema: REPEATABILITY_GATE_SCHEMA.to_string(),
        config: config.clone(),
        first: run_facts(first_output, &first_history, first_digest),
        second: run_facts(second_output, &second_history, second_digest),
        verdict,
        non_claims: NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    })
}

/// Write one receipt as pretty JSON, refusing to overwrite.
pub fn write_receipt(path: &Path, receipt: &RepeatabilityGateReceipt) -> Result<(), GateError> {
    let mut bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| GateError::Receipt(error.to_string()))?;
    bytes.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| GateError::Receipt(format!("create {}: {error}", path.display())))?;
    use std::io::Write;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| GateError::Receipt(format!("write {}: {error}", path.display())))
}

/// Suggested process exit status for a gate verdict.
pub fn exit_status(verdict: RepeatabilityVerdict) -> i32 {
    match verdict {
        RepeatabilityVerdict::Match => 0,
        RepeatabilityVerdict::Mismatch { .. } | RepeatabilityVerdict::RoundCountMismatch { .. } => {
            1
        }
        RepeatabilityVerdict::Operational { .. } => 2,
        RepeatabilityVerdict::Unavailable { .. } => 3,
    }
}

/// Default output directories for one gate invocation.
pub fn default_outputs(root: &Path, workload: &str) -> (PathBuf, PathBuf) {
    (
        root.join(format!("{workload}-first")),
        root.join(format!("{workload}-second")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(round: u64, digests: &[u8], watchdog: usize, failed: usize) -> RoundHistory {
        RoundHistory {
            round,
            branches_run: digests.len(),
            failed_branches: failed,
            new_edges: 0,
            cumulative_edges: 0,
            bugs_found: 0,
            cumulative_bugs: 0,
            frontier_size: 0,
            corpus_size: 0,
            restore_ms: 0.0,
            run_ms: 0.0,
            snapshot_ms: 0.0,
            coverage_ms: 0.0,
            wall_clock_seconds: 0.0,
            host_watchdog_timeouts: watchdog,
            host_watchdog_discarded_branches: 0,
            branch_digests: digests
                .iter()
                .map(|value| format!("b3:{}", format!("{value:02x}").repeat(32)))
                .collect(),
        }
    }

    #[test]
    fn run_facts_account_for_rounds_and_exclusions() {
        let history = vec![
            history(1, &[1, 2], 0, 0),
            history(2, &[], 1, 0),
            history(3, &[3], 0, 2),
        ];
        let facts = run_facts(Path::new("/tmp/out"), &history, "b3:digest".to_string());
        assert_eq!(facts.rounds, 3);
        assert_eq!(facts.branches, 3);
        assert_eq!(facts.operational_rounds, 2);
        assert_eq!(facts.unavailable_rounds, 1);
    }

    #[test]
    fn exit_status_distinguishes_verdicts() {
        assert_eq!(exit_status(RepeatabilityVerdict::Match), 0);
        assert_eq!(
            exit_status(RepeatabilityVerdict::RoundCountMismatch {
                first: 1,
                second: 2
            }),
            1
        );
        assert_eq!(
            exit_status(RepeatabilityVerdict::Operational {
                round: 1,
                reason: crate::repeatability::OperationalReason::WatchdogFirings,
            }),
            2
        );
        assert_eq!(
            exit_status(RepeatabilityVerdict::Unavailable { round: 1 }),
            3
        );
    }

    #[test]
    fn default_outputs_are_workload_scoped() {
        let (first, second) = default_outputs(Path::new("/tmp/root"), "none");
        assert_eq!(first, PathBuf::from("/tmp/root/none-first"));
        assert_eq!(second, PathBuf::from("/tmp/root/none-second"));
    }

    #[test]
    fn receipt_round_trips_and_refuses_overwrite() {
        let config = GateConfig {
            kernel: "kernel".into(),
            initrd: "initrd".into(),
            workload: "none".into(),
            extra_cmdline: String::new(),
            seed: 42,
            rounds: 1,
            branches: 1,
            ticks: 10,
            bootstrap_budget: 100,
            vms: 1,
        };
        let receipt = RepeatabilityGateReceipt {
            schema: REPEATABILITY_GATE_SCHEMA.to_string(),
            config,
            first: run_facts(
                Path::new("/tmp/a"),
                &[history(1, &[1], 0, 0)],
                "b3:a".into(),
            ),
            second: run_facts(
                Path::new("/tmp/b"),
                &[history(1, &[1], 0, 0)],
                "b3:b".into(),
            ),
            verdict: RepeatabilityVerdict::Match,
            non_claims: NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_string())
                .collect(),
        };
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("receipt.json");
        write_receipt(&path, &receipt).expect("receipt writes");
        assert!(write_receipt(&path, &receipt).is_err());
        let decoded: RepeatabilityGateReceipt =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(decoded, receipt);
    }
}
