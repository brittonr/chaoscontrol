//! Explicit composition for bounded protocol campaigns.

use super::*;
use crate::snapshot_store::{
    validate_ref_shape, ReplayParentSnapshotRef, SnapshotStore, SnapshotStoreError,
};
use chaoscontrol_vmm::controller::SimulationController;
use chaoscontrol_vmm::vm::VmError;

const MAX_REPLAY_TICKS: u64 = 65_536;
const SNAPSHOT_LINK_DOMAIN: &str = "chaoscontrol.protocol-observation.snapshot-link.v1";

#[derive(Debug)]
pub enum SessionError {
    Protocol(ProtocolObservationError),
    Snapshot(SnapshotStoreError),
    Vm(VmError),
}
impl From<ProtocolObservationError> for SessionError {
    fn from(error: ProtocolObservationError) -> Self {
        Self::Protocol(error)
    }
}

pub struct Session<O> {
    profile: AdmittedProfile,
    oracle: O,
}
impl<O: ProtocolOracle> Session<O> {
    pub fn new(profile: AdmittedProfile, oracle: O) -> Result<Self, ProtocolObservationError> {
        validate_oracle_adapter(&profile, &oracle)?;
        Ok(Self { profile, oracle })
    }

    /// Configure every admitted VM before the caller runs guest code.
    pub fn configure(&self, controller: &mut SimulationController) -> Result<(), SessionError> {
        let vm_count = controller.num_vms();
        if vm_count == 0
            || vm_count > self.profile.profile.bounds.max_participants as usize
            || self
                .profile
                .profile
                .producers
                .iter()
                .any(|producer| producer.vm_id as usize >= vm_count)
        {
            return Err(ProtocolObservationError::UnknownParticipant.into());
        }
        for index in 0..vm_count {
            let vm = controller.vm(index);
            if vm.exit_count() != 0 || !vm.fault_engine().protocol_collection().is_pristine() {
                return Err(ProtocolObservationError::IdentityMismatch(
                    "configuration-after-execution",
                )
                .into());
            }
        }
        for index in 0..vm_count {
            controller
                .vm_mut(index)
                .fault_engine_mut()
                .configure_protocol(self.profile.clone(), &self.oracle)?;
        }
        Ok(())
    }

    pub fn collect(
        &self,
        controller: &SimulationController,
        boundary: &str,
    ) -> Result<CohortResult, ProtocolObservationError> {
        if controller.num_vms() > self.profile.profile.bounds.max_participants as usize {
            return Err(ProtocolObservationError::BoundExceeded("controller-vms"));
        }
        let collections: Vec<_> = (0..controller.num_vms())
            .map(|index| controller.vm(index).fault_engine().protocol_collection())
            .collect();
        collect_cohort(&self.profile, boundary, &collections)
    }

    pub fn evaluate(
        &self,
        cohort: &CohortResult,
        context: ProtocolEvidenceContext,
    ) -> Result<ProtocolObservationReceipt, ProtocolObservationError> {
        validate_cohort(&self.profile, cohort)?;
        let oracle = if cohort.classification == CohortClassification::Complete {
            Some(run_consumer_oracle(&self.profile, cohort, &self.oracle)?)
        } else {
            None
        };
        build_receipt(&self.profile, cohort, oracle, context)
    }

    /// Load a checked parent, restore through the existing shell, and compare exact observations.
    pub fn replay<S: SnapshotStore>(
        &self,
        controller: &mut SimulationController,
        store: &S,
        request: ReplayRequest<'_>,
    ) -> Result<CohortResult, SessionError> {
        if request.ticks == 0 || request.ticks > MAX_REPLAY_TICKS {
            return Err(ProtocolObservationError::BoundExceeded("replay-ticks").into());
        }
        validate_cohort(&self.profile, request.expected)?;
        validate_marker_binding(&self.profile, request.expected, request.binding)?;
        let reference = snapshot_binding_reference(request.parent)?;
        if reference != request.binding.parent_snapshot_ref {
            return Err(ProtocolObservationError::MarkerMismatch.into());
        }
        let artifact = store
            .get_snapshot_artifact(request.parent)
            .map_err(SessionError::Snapshot)?;
        artifact
            .snapshot
            .tick
            .checked_add(request.ticks)
            .ok_or(ProtocolObservationError::CardinalityOverflow)?;
        controller
            .restore_all(&artifact.snapshot)
            .map_err(SessionError::Vm)?;
        controller.run(request.ticks).map_err(SessionError::Vm)?;
        let replayed = self.collect(controller, &request.expected.logical_boundary_ref)?;
        validate_replay(&self.profile, request.expected, &replayed)?;
        Ok(replayed)
    }
}

pub struct ReplayRequest<'a> {
    pub parent: &'a ReplayParentSnapshotRef,
    pub expected: &'a CohortResult,
    pub binding: &'a MarkerSnapshotBinding,
    pub ticks: u64,
}

pub fn snapshot_binding_reference(
    reference: &ReplayParentSnapshotRef,
) -> Result<String, SessionError> {
    validate_ref_shape(reference).map_err(SessionError::Snapshot)?;
    let bytes =
        serde_json::to_vec(reference).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    let digest = blake3::derive_key(SNAPSHOT_LINK_DOMAIN, &bytes);
    Ok(format!("snapshot:{}", blake3::Hash::from(digest).to_hex()))
}
