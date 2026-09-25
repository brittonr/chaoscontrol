//! One legacy frontier entry as one reusable Campaign candidate.
//!
//! The candidate binds its parent moment, an operation profile over the exploration
//! mode, the product entry sequence number, and the marker digest, the expansion
//! operation schema, the operation-generation profile, depth `parent + 1`, and one
//! branch-selection unit. No product DTO (snapshot, schedule, coverage) enters it.
//! The product sequence number advances for every admitted entry, including entries
//! pruned in the same round, so duplicate marker metadata still maps to distinct
//! candidates.

use campaign_core::{
    CandidateRecord, FrameWriter, FrontierState, Id, IdKind, ResourceAmount, UNIT_COST,
};

use super::identity::AdapterIdentity;
use super::AdapterError;
use crate::explorer::{ExplorationMode, ExplorerConfig};
use crate::marker_branching::MarkerFrontierMetadata;

const OPERATION_PROFILE_LABEL: &[u8] = b"chaoscontrol.expansion-operation-profile.v1";
const GENERATION_PROFILE_LABEL: &[u8] = b"chaoscontrol.operation-generation-profile.v1";
const MARKER_DIGEST_LABEL: &[u8] = b"chaoscontrol.marker-frontier-metadata.v1";
/// Input-tree alternative generation version bound into the generation profile.
pub const INPUT_TREE_GENERATION_VERSION: u32 = 1;
const MODE_FAULT_SCHEDULE: u32 = 1;
const MODE_INPUT_TREE: u32 = 2;
const MODE_HYBRID: u32 = 3;

/// Product facts of one admitted legacy frontier entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontierEntryFacts {
    /// Campaign moment of the entry's snapshot.
    pub parent_moment_id: Id,
    /// Exploration mode that expands the entry.
    pub mode: ExplorationMode,
    /// Product frontier sequence number.
    pub entry_seq: u64,
    /// Digest of the entry's marker metadata, if a marker created it.
    pub marker_digest: Option<[u8; 32]>,
}

const fn mode_code(mode: ExplorationMode) -> u32 {
    match mode {
        ExplorationMode::FaultSchedule => MODE_FAULT_SCHEDULE,
        ExplorationMode::InputTree => MODE_INPUT_TREE,
        ExplorationMode::Hybrid => MODE_HYBRID,
    }
}

fn option_bytes(writer: FrameWriter, value: Option<&[u8]>) -> FrameWriter {
    match value {
        None => writer.bool(false),
        Some(bytes) => writer.bool(true).bytes(bytes),
    }
}

/// Canonical digest of one marker's frontier metadata.
#[must_use]
pub fn marker_digest(marker: &MarkerFrontierMetadata) -> [u8; 32] {
    let writer = FrameWriter::new(IdKind::SourceFacts, MARKER_DIGEST_LABEL)
        .bytes(marker.marker_identity.as_bytes())
        .bytes(marker.owner.as_bytes());
    let writer = option_bytes(writer, marker.state_ref.as_deref().map(str::as_bytes));
    let writer = option_bytes(
        writer,
        marker.logical_position_ref.as_deref().map(str::as_bytes),
    );
    writer
        .u32(marker.run_id)
        .u64(marker.observed_tick)
        .finish(IdKind::SourceFacts)
        .digest
}

/// Operation profile over mode, entry sequence number, and marker digest.
#[must_use]
pub fn operation_profile_id(
    mode: ExplorationMode,
    entry_seq: u64,
    marker_digest: Option<[u8; 32]>,
) -> Id {
    let writer = FrameWriter::new(IdKind::OperationProfile, OPERATION_PROFILE_LABEL)
        .u32(mode_code(mode))
        .u64(entry_seq);
    option_bytes(writer, marker_digest.as_ref().map(<[u8; 32]>::as_slice))
        .finish(IdKind::OperationProfile)
}

fn count(value: usize) -> Result<u64, AdapterError> {
    u64::try_from(value).map_err(|_| AdapterError::BoundOverflow)
}

/// Operation-generation profile: mutation, havoc, diversity, branch factor, ticks,
/// and the input-tree generation version.
///
/// # Errors
/// Returns `BoundOverflow` when a count does not fit `u64`.
pub fn generation_profile_id(config: &ExplorerConfig) -> Result<Id, AdapterError> {
    let mutation = &config.mutation;
    let [havoc_min, havoc_max] = config.havoc_mutations;
    Ok(FrameWriter::new(IdKind::Policy, GENERATION_PROFILE_LABEL)
        .u64(count(mutation.num_vms)?)
        .u64(mutation.max_tick)
        .u64(mutation.add_prob.to_bits())
        .u64(mutation.remove_prob.to_bits())
        .u64(mutation.shift_prob.to_bits())
        .u64(mutation.replace_prob.to_bits())
        .u64(mutation.schedule_mutation_ratio.to_bits())
        .u64(mutation.base_quantum)
        .u64(config.havoc_after_stale)
        .u32(havoc_min)
        .u32(havoc_max)
        .bool(config.schedule_diversity)
        .u64(count(config.branch_factor)?)
        .u64(config.ticks_per_branch)
        .u32(INPUT_TREE_GENERATION_VERSION)
        .finish(IdKind::Policy))
}

/// Maps one frontier entry to its reusable Campaign candidate.
///
/// # Errors
/// Returns `UnknownParentMoment` when Campaign does not know the parent moment,
/// `BoundOverflow` for an exhausted depth, or a Campaign identity error.
// r[impl chaoscontrol.campaign_policy.adapter]
pub fn candidate_record(
    identity: &AdapterIdentity,
    state: &FrontierState,
    entry: &FrontierEntryFacts,
    generation_profile_id: Id,
) -> Result<CandidateRecord, AdapterError> {
    let parent_depth = state
        .known_moment_depth(entry.parent_moment_id)
        .ok_or(AdapterError::UnknownParentMoment)?;
    let depth = parent_depth
        .checked_add(1)
        .ok_or(AdapterError::BoundOverflow)?;
    build_candidate(
        identity,
        state.admission.campaign_id,
        entry.parent_moment_id,
        depth,
        operation_profile_id(entry.mode, entry.entry_seq, entry.marker_digest),
        generation_profile_id,
        UNIT_COST,
    )
}

/// Checks that `record` is exactly the candidate `entry` maps to in `state`.
///
/// # Errors
/// Returns `CrossedCampaign` for another campaign and `CandidateMismatch` for any
/// other moment, mode, sequence, marker, profile, cost, or adapter.
pub fn validate_candidate(
    identity: &AdapterIdentity,
    state: &FrontierState,
    entry: &FrontierEntryFacts,
    generation_profile_id: Id,
    record: &CandidateRecord,
) -> Result<(), AdapterError> {
    if record.campaign_id != state.admission.campaign_id {
        return Err(AdapterError::CrossedCampaign);
    }
    let expected = candidate_record(identity, state, entry, generation_profile_id)?;
    if expected != *record {
        return Err(AdapterError::CandidateMismatch);
    }
    Ok(())
}

/// Derives one candidate record from explicit identities and a cost in selection units.
pub(super) fn build_candidate(
    identity: &AdapterIdentity,
    campaign_id: Id,
    parent_moment_id: Id,
    depth: u64,
    operation_profile_id: Id,
    generation_profile_id: Id,
    selection_units: u64,
) -> Result<CandidateRecord, AdapterError> {
    Ok(CandidateRecord {
        candidate_id: Id::absent(),
        campaign_id,
        parent_moment_id,
        operation_profile_id,
        operation_schema_id: identity.operation_schema_id,
        source_adapter_id: identity.adapter_id,
        depth,
        cost_profile_id: identity.cost_profile_id,
        declared_costs: vec![ResourceAmount {
            dimension_id: identity.resource_dimension_id,
            amount: selection_units,
        }],
        operation_generation_profile_id: generation_profile_id,
    }
    .derive()?)
}
