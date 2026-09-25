//! Domain-separated product digests that moments and candidates bind.
//!
//! Digests cover product facts only (schedules, variants, coverage, oracle
//! reports, configuration); snapshot bytes are never hashed. The memory base set
//! is identified by its bootstrap facts because it is fixed after bootstrap for
//! the life of one journal epoch.

use super::FrontierError;
use crate::coverage::CoverageBitmap;
use crate::explorer::{ExplorationMode, ExplorerConfig};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::scheduler::ScheduleVariant;

const SCHEDULE_CONTEXT: &str = "chaoscontrol.frontier.schedule-digest.v1";
const VARIANT_CONTEXT: &str = "chaoscontrol.frontier.variant-digest.v1";
const COVERAGE_CONTEXT: &str = "chaoscontrol.frontier.coverage-digest.v1";
const ORACLE_CONTEXT: &str = "chaoscontrol.frontier.oracle-digest.v1";
const CONFIG_CONTEXT: &str = "chaoscontrol.frontier.config-digest.v1";
const BASE_SET_CONTEXT: &str = "chaoscontrol.frontier.memory-base-set.v1";
const MANIFEST_CONTEXT: &str = "chaoscontrol.frontier.round-manifest.v1";
const MODE_FAULT_SCHEDULE: u8 = 1;
const MODE_INPUT_TREE: u8 = 2;
const MODE_HYBRID: u8 = 3;

fn serde_digest<T: serde::Serialize>(
    context: &'static str,
    value: &T,
) -> Result<[u8; 32], FrontierError> {
    let bytes = serde_json::to_vec(value).map_err(|_| FrontierError::Encoding(context))?;
    Ok(blake3::derive_key(context, &bytes))
}

/// Digest of one fault schedule and its progress cursor.
pub(crate) fn schedule_digest(schedule: &FaultSchedule) -> Result<[u8; 32], FrontierError> {
    serde_digest(SCHEDULE_CONTEXT, &schedule.snapshot())
}

/// Digest of one optional schedule variant.
pub(crate) fn variant_digest(variant: Option<&ScheduleVariant>) -> Result<[u8; 32], FrontierError> {
    serde_digest(VARIANT_CONTEXT, &variant)
}

/// Digest of one coverage bitmap.
pub(crate) fn coverage_digest(coverage: &CoverageBitmap) -> [u8; 32] {
    blake3::derive_key(COVERAGE_CONTEXT, coverage.as_slice())
}

/// Digest of one oracle report.
pub(crate) fn oracle_digest(report: &OracleReport) -> Result<[u8; 32], FrontierError> {
    serde_digest(ORACLE_CONTEXT, report)
}

const fn mode_code(mode: ExplorationMode) -> u8 {
    match mode {
        ExplorationMode::FaultSchedule => MODE_FAULT_SCHEDULE,
        ExplorationMode::InputTree => MODE_INPUT_TREE,
        ExplorationMode::Hybrid => MODE_HYBRID,
    }
}

/// Digest of the exploration configuration a root moment binds.
pub(crate) fn config_digest(
    config: &ExplorerConfig,
    generation_profile: campaign_core::Id,
) -> Result<[u8; 32], FrontierError> {
    let count = |value: usize| u64::try_from(value).map_err(|_| FrontierError::BoundOverflow);
    let mut hasher = blake3::Hasher::new_derive_key(CONFIG_CONTEXT);
    hasher.update(&generation_profile.digest);
    hasher.update(config.frontier_policy.as_str().as_bytes());
    hasher.update(&[
        mode_code(config.exploration_mode),
        config.rare_edge_threshold,
    ]);
    for value in [
        config.seed,
        config.max_rounds,
        count(config.max_frontier)?,
        count(config.num_vms)?,
        config.quantum,
        config.coverage_gpa,
        config.bootstrap_budget,
        config.stale_round_limit,
        config.rare_edge_weight.to_bits(),
    ] {
        hasher.update(&value.to_le_bytes());
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Identity of the memory base set shared by every overlay of one epoch.
pub(crate) fn base_set_id(
    epoch: u32,
    bootstrap_tick: u64,
    bootstrap_coverage: &[u8; 32],
    base_lengths: &[usize],
) -> Result<[u8; 32], FrontierError> {
    let mut hasher = blake3::Hasher::new_derive_key(BASE_SET_CONTEXT);
    hasher.update(&epoch.to_le_bytes());
    hasher.update(&bootstrap_tick.to_le_bytes());
    hasher.update(bootstrap_coverage);
    let bases = u64::try_from(base_lengths.len()).map_err(|_| FrontierError::BoundOverflow)?;
    hasher.update(&bases.to_le_bytes());
    for length in base_lengths {
        let length = u64::try_from(*length).map_err(|_| FrontierError::BoundOverflow)?;
        hasher.update(&length.to_le_bytes());
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Digest of one settled round's product manifest (branch facts, never payloads).
pub(crate) fn round_manifest_digest(
    observation: &super::RoundObservation,
) -> Result<[u8; 32], FrontierError> {
    let mut hasher = blake3::Hasher::new_derive_key(MANIFEST_CONTEXT);
    for branch in &observation.branches {
        let (slot, index) = super::records::slot_code(branch.slot);
        hasher.update(&[slot]);
        hasher.update(&index.to_le_bytes());
        let new_edges =
            u64::try_from(branch.new_edges).map_err(|_| FrontierError::BoundOverflow)?;
        hasher.update(&new_edges.to_le_bytes());
        hasher.update(&[u8::from(branch.snapshot_present)]);
        for finding in &branch.findings {
            hasher.update(&finding.dedup_key.to_le_bytes());
            hasher.update(&finding.assertion_fingerprint);
        }
        if let Some(retained) = &branch.retained {
            if let Some(digests) = &retained.digests {
                hasher.update(&digests.schedule);
                hasher.update(&digests.coverage);
                hasher.update(&digests.oracle);
            }
            for proposal in &retained.proposals {
                hasher.update(&proposal.score.to_bits().to_le_bytes());
            }
        }
    }
    Ok(*hasher.finalize().as_bytes())
}
