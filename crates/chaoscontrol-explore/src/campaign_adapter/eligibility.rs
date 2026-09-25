//! Snapshot eligibility: a candidate executes only from a current restorable snapshot.
//!
//! The product `SnapshotTable` maps a Campaign moment to its retained snapshot handle,
//! the memory base set that snapshot overlays, and the journal epoch that produced it.
//! No clean-bootstrap operation is admitted, so a candidate whose parent moment has
//! no current snapshot is closed unobserved (when selected) and retired.

use std::collections::BTreeMap;

use campaign_core::{
    CandidateRecord, FrontierState, Id, IdKind, RetirementOrigin, RetirementRequest,
    SelectionDecision, UnobservedClosure,
};

use super::identity::{AdapterIdentity, AdapterReason};
use super::AdapterError;

/// Why a moment's snapshot cannot be restored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IneligibleReason {
    /// The product did not retain a snapshot for the moment.
    NotRetained,
    /// The snapshot overlays another memory base set than the current controller's.
    StaleBaseSet,
    /// The snapshot belongs to an earlier journal epoch.
    StaleEpoch,
}

impl IneligibleReason {
    /// Adapter retirement reason of this ineligibility.
    #[must_use]
    pub const fn reason(self) -> AdapterReason {
        match self {
            Self::NotRetained => AdapterReason::SnapshotNotRetained,
            Self::StaleBaseSet => AdapterReason::SnapshotStaleBaseSet,
            Self::StaleEpoch => AdapterReason::SnapshotStaleEpoch,
        }
    }
}

/// Eligibility of one moment for execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Eligibility {
    /// The moment has a current restorable snapshot.
    Eligible,
    /// The moment cannot be restored.
    Ineligible(IneligibleReason),
}

/// Current restore context of the controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotContext {
    /// Memory base set of the current controller.
    pub base_set_id: [u8; 32],
    /// Current journal epoch.
    pub epoch: u64,
}

/// One retained snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotEntry<H> {
    /// Product snapshot handle.
    pub handle: H,
    /// Memory base set the snapshot overlays.
    pub base_set_id: [u8; 32],
    /// Journal epoch that produced the snapshot.
    pub epoch: u64,
}

/// Product map from Campaign moment to retained snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotTable<H> {
    entries: BTreeMap<Id, SnapshotEntry<H>>,
}

impl<H> Default for SnapshotTable<H> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl<H> SnapshotTable<H> {
    /// Creates an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Retains the snapshot of one moment.
    ///
    /// # Errors
    /// Returns `InvalidSnapshotKey` for a non-moment key or an already retained moment.
    pub fn retain(&mut self, moment_id: Id, entry: SnapshotEntry<H>) -> Result<(), AdapterError> {
        let is_moment = matches!(moment_id.kind, IdKind::RootMoment | IdKind::ChildMoment);
        if !is_moment || self.entries.contains_key(&moment_id) {
            return Err(AdapterError::InvalidSnapshotKey);
        }
        self.entries.insert(moment_id, entry);
        Ok(())
    }

    /// Releases the snapshot of one moment; the moment stays in history.
    pub fn release(&mut self, moment_id: Id) -> Option<SnapshotEntry<H>> {
        self.entries.remove(&moment_id)
    }

    /// Number of retained snapshots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no snapshot is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Eligibility of one moment under the current restore context.
    #[must_use]
    pub fn eligibility(&self, moment_id: Id, context: SnapshotContext) -> Eligibility {
        match self.entries.get(&moment_id) {
            None => Eligibility::Ineligible(IneligibleReason::NotRetained),
            Some(entry) if entry.epoch != context.epoch => {
                Eligibility::Ineligible(IneligibleReason::StaleEpoch)
            }
            Some(entry) if entry.base_set_id != context.base_set_id => {
                Eligibility::Ineligible(IneligibleReason::StaleBaseSet)
            }
            Some(_) => Eligibility::Eligible,
        }
    }

    /// Returns the restorable snapshot of a candidate's parent moment.
    ///
    /// # Errors
    /// Returns `Ineligible` with the exact reason.
    // r[impl chaoscontrol.campaign_policy.snapshot_eligibility]
    pub fn require_executable(
        &self,
        candidate: &CandidateRecord,
        context: SnapshotContext,
    ) -> Result<&SnapshotEntry<H>, AdapterError> {
        match self.eligibility(candidate.parent_moment_id, context) {
            Eligibility::Ineligible(reason) => Err(AdapterError::Ineligible(reason)),
            Eligibility::Eligible => self
                .entries
                .get(&candidate.parent_moment_id)
                .ok_or(AdapterError::Ineligible(IneligibleReason::NotRetained)),
        }
    }
}

/// Explicit consumer retirement of an ineligible candidate.
#[must_use]
pub fn ineligible_retirement(
    identity: &AdapterIdentity,
    state: &FrontierState,
    candidate_id: Id,
    reason: IneligibleReason,
) -> RetirementRequest {
    RetirementRequest {
        campaign_id: state.admission.campaign_id,
        state_id: state.state_id,
        policy_id: identity.policy_id,
        adapter_id: identity.adapter_id,
        candidate_id,
        reason_id: identity.reason_id(reason.reason()),
        origin: RetirementOrigin::Consumer,
    }
}

/// Acknowledged unobserved closure of a published selection that cannot execute or
/// whose snapshot state was lost.
///
/// # Errors
/// Returns a Campaign identity error for a crossed selection.
pub fn unobserved_closure(
    identity: &AdapterIdentity,
    selection: &SelectionDecision,
    reason: AdapterReason,
) -> Result<UnobservedClosure, AdapterError> {
    let mut closure = UnobservedClosure {
        closure_id: Id::absent(),
        campaign_id: selection.campaign_id,
        selection_id: selection.selection_id,
        reason_id: identity.reason_id(reason),
        uncertainty_acknowledged: true,
    };
    closure.closure_id = campaign_core::derive_closure_id(&closure)?;
    Ok(closure)
}
