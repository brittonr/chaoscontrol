//! Retained exploration moments as immutable Choregraph application events.
//!
//! Root `M0` is the bootstrap snapshot moment; each child is one branch result with
//! new edges and a retained snapshot. Payload digests bind product facts and digests
//! only: snapshot bytes are never hashed. Moment events stay outside Campaign's linear
//! control lineage, so callers re-derive the projection binding from the current graph
//! and control branch before every publication.

use campaign_choregraph::{
    derive_projection_binding, digest_from_id, id_from_digest, BranchRef, Digest, EventClass,
    EventDraft, EventId, HistoryError, HistoryGraph, MetadataIdentity, ParentLink,
};
use campaign_core::{FrameWriter, Id, IdKind, ProjectionBinding};

use super::identity::AdapterIdentity;
use super::AdapterError;

/// Choregraph schema of every ChaosControl exploration moment event.
pub const MOMENT_SCHEMA_LABEL: &[u8] = b"chaoscontrol.exploration-moment.v1";
/// Parent role linking a child moment to the moment it expanded.
pub const EXPLORATION_PARENT_ROLE: &str = "chaoscontrol.exploration-parent";
/// Name of the Choregraph branch that carries Campaign's linear control lineage.
pub const CONTROL_BRANCH_NAME: &str = "chaoscontrol.control";
/// Generation of the control branch before the campaign admission is published.
pub const ROOT_BRANCH_GENERATION: u64 = 0;
/// Metadata key naming the adapter that produced a moment.
pub const ADAPTER_METADATA_KEY: &str = "chaoscontrol.adapter";
/// Metadata key naming the moment kind.
pub const MOMENT_KIND_METADATA_KEY: &str = "chaoscontrol.moment-kind";
const MOMENT_DOMAIN: &[u8] = b"chaoscontrol.campaign-adapter.moment";
const ROOT_PAYLOAD_LABEL: &[u8] = b"chaoscontrol.moment.root.v1";
const CHILD_PAYLOAD_LABEL: &[u8] = b"chaoscontrol.moment.child.v1";
const ROOT_KIND: &[u8] = b"root";
const CHILD_KIND: &[u8] = b"child";
const SLOT_PROBE: u32 = 1;
const SLOT_VARIANT: u32 = 2;
const SLOT_ALTERNATIVE: u32 = 3;

/// Bootstrap facts the root moment binds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootMomentFacts {
    /// Journal epoch.
    pub epoch: u64,
    /// Digest of the admitted exploration configuration.
    pub config_digest: [u8; 32],
    /// Exploration seed.
    pub seed: u64,
    /// Tick at which the bootstrap snapshot was taken.
    pub bootstrap_tick: u64,
    /// Digest of the bootstrap coverage bitmap.
    pub coverage_digest: [u8; 32],
    /// Digest of the bootstrap oracle report.
    pub oracle_digest: [u8; 32],
    /// Identity of the incremental-snapshot memory base set.
    pub base_set_id: [u8; 32],
}

/// Branch position that produced a child moment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchSlot {
    /// Input-tree probe run.
    Probe,
    /// Fault-schedule variant `k`.
    Variant(u32),
    /// Input-tree alternative `k`.
    Alternative(u32),
}

/// Branch-result facts one child moment binds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildMomentFacts {
    /// Admitted campaign.
    pub campaign_id: Id,
    /// Published selection the branch executed.
    pub selection_id: Id,
    /// Operation of that selection.
    pub operation_id: Id,
    /// Branch position.
    pub branch_slot: BranchSlot,
    /// Digest of the branch fault schedule.
    pub schedule_digest: [u8; 32],
    /// Digest of the branch schedule variant.
    pub variant_digest: [u8; 32],
    /// Digest of the enriched branch coverage.
    pub coverage_digest: [u8; 32],
    /// Digest of the branch oracle report.
    pub oracle_digest: [u8; 32],
    /// Ticks the branch ran.
    pub total_ticks: u64,
    /// Tick at which the child snapshot was taken.
    pub snapshot_tick: u64,
    /// Identity of the memory base set the snapshot overlays.
    pub base_set_id: [u8; 32],
}

/// One admitted moment and the graph that contains it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedMoment {
    /// Graph after admission.
    pub graph: HistoryGraph,
    /// Choregraph event identity.
    pub event: EventId,
    /// Campaign moment identity (`RootMoment` or `ChildMoment`).
    pub moment_id: Id,
    /// `EventPayload` identity of the moment event.
    pub event_payload_id: Id,
}

/// Choregraph schema digest of exploration moments.
#[must_use]
pub fn moment_schema() -> Digest {
    Digest::derive(MOMENT_DOMAIN, MOMENT_SCHEMA_LABEL)
}

/// Payload identity of the root moment.
#[must_use]
pub fn root_payload_id(facts: &RootMomentFacts) -> Id {
    FrameWriter::new(IdKind::EventPayload, ROOT_PAYLOAD_LABEL)
        .u64(facts.epoch)
        .bytes(&facts.config_digest)
        .u64(facts.seed)
        .u64(facts.bootstrap_tick)
        .bytes(&facts.coverage_digest)
        .bytes(&facts.oracle_digest)
        .bytes(&facts.base_set_id)
        .finish(IdKind::EventPayload)
}

/// Payload identity of one child moment.
///
/// # Errors
/// Returns a Campaign identity error for a missing or crossed campaign, selection,
/// or operation identity.
pub fn child_payload_id(facts: &ChildMomentFacts) -> Result<Id, AdapterError> {
    facts.campaign_id.require_kind(IdKind::Campaign)?;
    facts.selection_id.require_kind(IdKind::Selection)?;
    facts.operation_id.require_kind(IdKind::Operation)?;
    let (slot, index) = match facts.branch_slot {
        BranchSlot::Probe => (SLOT_PROBE, 0),
        BranchSlot::Variant(index) => (SLOT_VARIANT, index),
        BranchSlot::Alternative(index) => (SLOT_ALTERNATIVE, index),
    };
    Ok(FrameWriter::new(IdKind::EventPayload, CHILD_PAYLOAD_LABEL)
        .id(facts.campaign_id)
        .id(facts.selection_id)
        .id(facts.operation_id)
        .u32(slot)
        .u32(index)
        .bytes(&facts.schedule_digest)
        .bytes(&facts.variant_digest)
        .bytes(&facts.coverage_digest)
        .bytes(&facts.oracle_digest)
        .u64(facts.total_ticks)
        .u64(facts.snapshot_tick)
        .bytes(&facts.base_set_id)
        .finish(IdKind::EventPayload))
}

fn metadata(identity: &AdapterIdentity, kind: &[u8]) -> Vec<MetadataIdentity> {
    let entries = vec![
        MetadataIdentity {
            key: ADAPTER_METADATA_KEY.to_owned(),
            value: digest_from_id(identity.adapter_id),
        },
        MetadataIdentity {
            key: MOMENT_KIND_METADATA_KEY.to_owned(),
            value: Digest::derive(MOMENT_DOMAIN, kind),
        },
    ];
    debug_assert!(entries.is_sorted());
    entries
}

fn moment_draft(
    identity: &AdapterIdentity,
    payload_id: Id,
    parents: Vec<ParentLink>,
    kind: &[u8],
) -> EventDraft {
    EventDraft {
        metadata: metadata(identity, kind),
        ..EventDraft::v1(
            EventClass::Application,
            digest_from_id(payload_id),
            parents,
            moment_schema(),
        )
    }
}

/// Root moment event draft: no parents.
#[must_use]
// r[impl chaoscontrol.campaign_policy.history]
pub fn root_moment_draft(identity: &AdapterIdentity, facts: &RootMomentFacts) -> EventDraft {
    moment_draft(identity, root_payload_id(facts), Vec::new(), ROOT_KIND)
}

/// Child moment event draft with one exploration-parent link.
///
/// # Errors
/// Returns a Campaign identity error for crossed facts.
pub fn child_moment_draft(
    identity: &AdapterIdentity,
    facts: &ChildMomentFacts,
    parent_event: EventId,
) -> Result<EventDraft, AdapterError> {
    let parents = vec![ParentLink {
        role: EXPLORATION_PARENT_ROLE.to_owned(),
        event: parent_event,
    }];
    Ok(moment_draft(
        identity,
        child_payload_id(facts)?,
        parents,
        CHILD_KIND,
    ))
}

/// Campaign `RootMoment` identity of an admitted root moment event.
#[must_use]
pub const fn root_moment_id(event: EventId) -> Id {
    id_from_digest(IdKind::RootMoment, event.digest())
}

/// Campaign `ChildMoment` identity of an admitted child moment event.
#[must_use]
pub const fn child_moment_id(event: EventId) -> Id {
    id_from_digest(IdKind::ChildMoment, event.digest())
}

/// Control branch at the root moment, before the campaign admission is published.
#[must_use]
pub fn root_control_branch(root: &AdmittedMoment) -> BranchRef {
    BranchRef {
        name: CONTROL_BRANCH_NAME.to_owned(),
        head: root.event,
        generation: ROOT_BRANCH_GENERATION,
    }
}

/// Admits the root moment into a fresh or existing graph.
///
/// # Errors
/// Returns the Choregraph history error; the input graph is unchanged.
pub fn admit_root_moment(
    graph: &HistoryGraph,
    identity: &AdapterIdentity,
    facts: &RootMomentFacts,
) -> Result<AdmittedMoment, AdapterError> {
    let admission = graph.admit(root_moment_draft(identity, facts))?;
    Ok(AdmittedMoment {
        graph: admission.graph,
        event: admission.event,
        moment_id: root_moment_id(admission.event),
        event_payload_id: root_payload_id(facts),
    })
}

/// Admits one child moment below an existing exploration moment.
///
/// # Errors
/// Returns `CrossedCampaign` for another campaign, `History(MissingParent)` for a
/// parent outside `graph`, `NotMoment` for a control-event parent, and the
/// Choregraph error of a conflicting or duplicate event. The input graph is unchanged.
pub fn admit_child_moment(
    graph: &HistoryGraph,
    identity: &AdapterIdentity,
    campaign_id: Id,
    facts: &ChildMomentFacts,
    parent_event: EventId,
) -> Result<AdmittedMoment, AdapterError> {
    if facts.campaign_id != campaign_id {
        return Err(AdapterError::CrossedCampaign);
    }
    let parent = graph
        .event(parent_event)
        .ok_or(HistoryError::MissingParent {
            event: parent_event,
        })?;
    if parent.content().schema_identity != moment_schema() {
        return Err(AdapterError::NotMoment);
    }
    let draft = child_moment_draft(identity, facts, parent_event)?;
    let admission = graph.admit(draft)?;
    Ok(AdmittedMoment {
        graph: admission.graph,
        event: admission.event,
        moment_id: child_moment_id(admission.event),
        event_payload_id: child_payload_id(facts)?,
    })
}

/// Re-admits a recorded moment draft under its recorded event identity.
///
/// Recovery uses this so codec drift or a rewritten draft fails as an identity
/// mismatch instead of silently producing another event.
///
/// # Errors
/// Returns the Choregraph error of a mismatched, conflicting, or duplicate claim.
pub fn readmit_moment(
    graph: &HistoryGraph,
    claimed: EventId,
    draft: EventDraft,
) -> Result<HistoryGraph, AdapterError> {
    if draft.schema_identity != moment_schema() {
        return Err(AdapterError::NotMoment);
    }
    Ok(graph.admit_claimed(claimed, draft)?.graph)
}

/// Projection binding of the current graph and control branch.
///
/// # Errors
/// Returns the Choregraph adapter error of a malformed binding.
pub fn projection_binding(
    identity: &AdapterIdentity,
    graph: &HistoryGraph,
    branch: &BranchRef,
) -> Result<ProjectionBinding, AdapterError> {
    Ok(derive_projection_binding(
        graph,
        branch,
        identity.projection_schema_id,
        identity.adapter_id,
        identity.control_domain_id,
    )?)
}
