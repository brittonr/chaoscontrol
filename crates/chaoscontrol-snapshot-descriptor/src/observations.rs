#![allow(
    non_trait_imports,
    reason = "detached observation records compose the descriptor-owned cohort and content DTOs without effects"
)]

use serde::{Deserialize, Serialize};

use crate::model::{ContentIdentity, RuntimeCohort, SnapshotTopology, TaggedDigest};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationObservation {
    pub destination_id: String,
    pub completeness_profile: String,
    pub state_schema_version: u32,
    pub architecture: String,
    pub runtime: RuntimeCohort,
    pub topology: SnapshotTopology,
    pub available_memory_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightBlocker {
    pub code: String,
    pub expected: String,
    pub observed: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreflightStatus {
    Admitted,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestorePhase {
    Materialize,
    Quiesce,
    GuestMemory,
    IrqChip,
    Pit,
    Clock,
    Vcpu,
    Scheduler,
    Devices,
    HostHandles,
    Continuation,
}

pub const REQUIRED_RESTORE_PHASES: [RestorePhase; 11] = [
    RestorePhase::Materialize,
    RestorePhase::Quiesce,
    RestorePhase::GuestMemory,
    RestorePhase::IrqChip,
    RestorePhase::Pit,
    RestorePhase::Clock,
    RestorePhase::Vcpu,
    RestorePhase::Scheduler,
    RestorePhase::Devices,
    RestorePhase::HostHandles,
    RestorePhase::Continuation,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestorePlan {
    pub descriptor_id: TaggedDigest,
    pub destination_id: TaggedDigest,
    pub phases: Vec<RestorePhase>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightDecision {
    pub status: PreflightStatus,
    pub blockers: Vec<PreflightBlocker>,
    pub plan: Option<RestorePlan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhaseStatus {
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestorePhaseObservation {
    pub phase: RestorePhase,
    pub status: PhaseStatus,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationObservation {
    pub checked_steps: u64,
    pub deterministic_trace_matches: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreReceipt {
    pub descriptor_id: TaggedDigest,
    pub destination_id: TaggedDigest,
    pub preflight_id: TaggedDigest,
    pub materialized: bool,
    pub mutation_started: bool,
    pub phases: Vec<RestorePhaseObservation>,
    pub poisoned: bool,
    pub completed: bool,
    pub continuation: Option<ContinuationObservation>,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocatorKind {
    File,
    Redb,
    IrohTicket,
    Url,
    Mirror,
    Provider,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocatorHint {
    pub kind: LocatorKind,
    pub locator: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocatorSidecar {
    pub descriptor_id: TaggedDigest,
    pub hints: Vec<LocatorHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisallowedConsumerClaim {
    RestoreAuthority,
    WorldBranch,
    WorldMerge,
    Promotion,
    ReleaseEligibility,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerSnapshotReference {
    pub descriptor_id: TaggedDigest,
    pub completeness_profile: String,
    pub logical_payload: ContentIdentity,
    pub closure_members: Vec<ContentIdentity>,
    pub preflight_id: TaggedDigest,
    pub disallowed_claims: Vec<DisallowedConsumerClaim>,
}
