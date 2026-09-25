//! Stable ChaosControl adapter identities bound into every admitted campaign.
//!
//! The adapter identity binds the rank-conversion and entropy-conversion profiles.
//! The entropy source identity additionally binds the run seed, so a ticket derived
//! for another seed fails Campaign's entropy validation.

use campaign_core::{FrameWriter, Id, IdKind};

use super::ranks::rank_profile_id;

/// Adapter identity label and version.
pub const ADAPTER_LABEL: &[u8] = b"chaoscontrol.campaign-adapter.v1";
/// Entropy conversion rule: ChaCha8 epsilon draws mapped to explicit Campaign tickets.
pub const ENTROPY_RULE_LABEL: &[u8] = b"chaoscontrol.entropy.chacha8-epsilon.v1";
/// Exact `rand` release whose `gen::<f64>` and `gen_range` draws the rule converts.
pub const RAND_VERSION: &str = "0.8.6";
/// Exact `rand_chacha` release of the seeded exploration generator.
pub const RAND_CHACHA_VERSION: &str = "0.3.1";
/// Version of the draw-to-ticket rule in `entropy`.
pub const ENTROPY_RULE_VERSION: u32 = 1;
/// Projection schema of the ChaosControl control projection.
pub const PROJECTION_SCHEMA_LABEL: &[u8] = b"chaoscontrol.campaign-projection.v1";
/// Control domain of ChaosControl exploration.
pub const CONTROL_DOMAIN_LABEL: &[u8] = b"chaoscontrol.exploration-control.v1";
/// Operation schema of one frontier expansion.
pub const OPERATION_SCHEMA_LABEL: &[u8] = b"chaoscontrol.expansion-operation.v1";
/// Resource dimension counting branch selections.
pub const RESOURCE_DIMENSION_LABEL: &[u8] = b"chaoscontrol.branch-selection-unit.v1";
/// Cost profile: every candidate reserves one selection unit.
pub const COST_PROFILE_LABEL: &[u8] = b"chaoscontrol.one-selection-unit.v1";
/// Admitted Campaign policy identity.
pub const POLICY_LABEL: &[u8] = b"chaoscontrol.campaign-policy.v1";
/// Compiled production selection policy.
pub const PRODUCTION_POLICY_LABEL: &[u8] = b"chaoscontrol.campaign-production-policy.v1";
/// Sequential scheduling: one outstanding selection, settled before the next plan.
pub const SCHEDULING_LABEL: &[u8] = b"chaoscontrol.sequential-rounds.v1";
/// Duplicate candidates are rejected, never coalesced.
pub const COALESCING_LABEL: &[u8] = b"chaoscontrol.no-coalescing.v1";
/// Retention profile: keep the best `max_frontier` by original score, then insertion.
pub const RETENTION_PROFILE_LABEL: &[u8] = b"chaoscontrol.frontier-retention.v1";
const ENTROPY_SOURCE_LABEL: &[u8] = b"chaoscontrol.entropy.chacha8-epsilon.source.v1";

/// Bounded consumer reasons the adapter records.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AdapterReason {
    /// Explicit post-settlement guidance revision for division decay.
    SelectionCountDecay,
    /// Retention pruning to `max_frontier`.
    RetentionPrune,
    /// Coverage plateau stop.
    CoveragePlateau,
    /// Short-run finding stop.
    ShortRunFinding,
    /// Operator signal stop.
    OperatorSignal,
    /// `max_rounds == 0`: the run completes without a selection.
    CompletedWithoutSelection,
    /// Candidate retired because its parent snapshot was not retained.
    SnapshotNotRetained,
    /// Candidate retired because its parent snapshot has another memory base set.
    SnapshotStaleBaseSet,
    /// Candidate retired because its parent snapshot belongs to an earlier epoch.
    SnapshotStaleEpoch,
    /// Outstanding selection closed because its snapshot state was lost.
    SnapshotStateLost,
    /// A recovered epoch was closed before a new epoch started.
    ResumeEpochClosed,
}

impl AdapterReason {
    /// Every reason in stable order.
    pub const ALL: [Self; 11] = [
        Self::SelectionCountDecay,
        Self::RetentionPrune,
        Self::CoveragePlateau,
        Self::ShortRunFinding,
        Self::OperatorSignal,
        Self::CompletedWithoutSelection,
        Self::SnapshotNotRetained,
        Self::SnapshotStaleBaseSet,
        Self::SnapshotStaleEpoch,
        Self::SnapshotStateLost,
        Self::ResumeEpochClosed,
    ];

    /// Stable reason label.
    #[must_use]
    pub const fn label(self) -> &'static [u8] {
        match self {
            Self::SelectionCountDecay => b"chaoscontrol.selection-count-decay.v1",
            Self::RetentionPrune => b"chaoscontrol.retention.max-frontier.v1",
            Self::CoveragePlateau => b"chaoscontrol.stop.coverage-plateau.v1",
            Self::ShortRunFinding => b"chaoscontrol.stop.short-run-finding.v1",
            Self::OperatorSignal => b"chaoscontrol.stop.operator-signal.v1",
            Self::CompletedWithoutSelection => b"chaoscontrol.stop.completed-without-selection.v1",
            Self::SnapshotNotRetained => b"chaoscontrol.eligibility.not-retained.v1",
            Self::SnapshotStaleBaseSet => b"chaoscontrol.eligibility.stale-base-set.v1",
            Self::SnapshotStaleEpoch => b"chaoscontrol.eligibility.stale-epoch.v1",
            Self::SnapshotStateLost => b"chaoscontrol.closure.snapshot-state-lost.v1",
            Self::ResumeEpochClosed => b"chaoscontrol.stop.resume-epoch-closed.v1",
        }
    }

    /// Maps a Campaign reason identity back to the adapter reason.
    #[must_use]
    pub fn from_id(reason_id: Id) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|reason| named(IdKind::Reason, reason.label()) == reason_id)
    }
}

/// Complete identity set of the ChaosControl adapter for one run seed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterIdentity {
    /// Adapter identity binding both conversion profiles.
    pub adapter_id: Id,
    /// Projection schema identity.
    pub projection_schema_id: Id,
    /// Control domain identity.
    pub control_domain_id: Id,
    /// Rank-conversion (guidance ordering) profile.
    pub guidance_profile_id: Id,
    /// Seed-independent entropy-conversion rule.
    pub entropy_rule_id: Id,
    /// Seed-bound entropy source identity.
    pub entropy_source_id: Id,
    /// Expansion operation schema.
    pub operation_schema_id: Id,
    /// Branch-selection resource dimension.
    pub resource_dimension_id: Id,
    /// One-unit cost profile.
    pub cost_profile_id: Id,
    /// Admitted Campaign policy.
    pub policy_id: Id,
    /// Compiled production selection policy.
    pub production_policy_id: Id,
    /// Sequential scheduling profile.
    pub scheduling_policy_id: Id,
    /// Coalescing profile (duplicates rejected).
    pub coalescing_policy_id: Id,
    /// Retention profile.
    pub retention_profile_id: Id,
}

impl AdapterIdentity {
    /// Derives every adapter identity for one exploration seed.
    #[must_use]
    pub fn for_seed(seed: u64) -> Self {
        let guidance_profile_id = rank_profile_id();
        let entropy_rule_id = entropy_rule_id();
        Self {
            adapter_id: adapter_id(guidance_profile_id, entropy_rule_id),
            projection_schema_id: named(IdKind::Projection, PROJECTION_SCHEMA_LABEL),
            control_domain_id: named(IdKind::ControlDomain, CONTROL_DOMAIN_LABEL),
            guidance_profile_id,
            entropy_rule_id,
            entropy_source_id: entropy_source_id(seed),
            operation_schema_id: named(IdKind::OperationSchema, OPERATION_SCHEMA_LABEL),
            resource_dimension_id: named(IdKind::ResourceDimension, RESOURCE_DIMENSION_LABEL),
            cost_profile_id: named(IdKind::CostProfile, COST_PROFILE_LABEL),
            policy_id: named(IdKind::Policy, POLICY_LABEL),
            production_policy_id: named(IdKind::Policy, PRODUCTION_POLICY_LABEL),
            scheduling_policy_id: named(IdKind::SchedulingProfile, SCHEDULING_LABEL),
            coalescing_policy_id: named(IdKind::CoalescingPolicy, COALESCING_LABEL),
            retention_profile_id: named(IdKind::RetentionProfile, RETENTION_PROFILE_LABEL),
        }
    }

    /// Bounded `Reason` identity of one adapter reason.
    #[must_use]
    pub fn reason_id(&self, reason: AdapterReason) -> Id {
        named(IdKind::Reason, reason.label())
    }
}

/// Seed-independent entropy conversion rule identity.
#[must_use]
pub fn entropy_rule_id() -> Id {
    FrameWriter::new(IdKind::EntropyProfile, ENTROPY_RULE_LABEL)
        .bytes(RAND_VERSION.as_bytes())
        .bytes(RAND_CHACHA_VERSION.as_bytes())
        .u32(ENTROPY_RULE_VERSION)
        .finish(IdKind::EntropyProfile)
}

/// Seed-bound entropy source: rule label, seed, generator versions, and rule version.
#[must_use]
pub fn entropy_source_id(seed: u64) -> Id {
    FrameWriter::new(IdKind::EntropyProfile, ENTROPY_SOURCE_LABEL)
        .bytes(ENTROPY_RULE_LABEL)
        .u64(seed)
        .bytes(RAND_VERSION.as_bytes())
        .bytes(RAND_CHACHA_VERSION.as_bytes())
        .u32(ENTROPY_RULE_VERSION)
        .finish(IdKind::EntropyProfile)
}

fn adapter_id(guidance_profile_id: Id, entropy_rule_id: Id) -> Id {
    campaign_core::derive_id(
        IdKind::Adapter,
        ADAPTER_LABEL,
        &[guidance_profile_id, entropy_rule_id],
    )
}

/// Label-only identity of one kind.
pub(super) fn named(kind: IdKind, label: &[u8]) -> Id {
    campaign_core::derive_id(kind, label, &[])
}
