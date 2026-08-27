use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vm_cohort_core::{CohortPlan, CohortState, ProfileRef, ReceiptRef, ResourceRef};
use vm_cohort_kvm::KvmRuntimeProfile;

/// ChaosControl-owned compatibility facts that are not portable snapshot bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChaosCompatibilityFacts {
    /// VM Cohort profile identity.
    pub profile_ref: ProfileRef,
    /// Kernel artifact identity.
    pub kernel_ref: ResourceRef,
    /// Guest image identity.
    pub guest_image_ref: ResourceRef,
    /// Disk format identity.
    pub disk_format_ref: ResourceRef,
    /// ChaosControl runtime build identity.
    pub runtime_ref: ResourceRef,
    /// Exact adapter identity.
    pub adapter_ref: ResourceRef,
}

/// Complete mapped checkpoint and cohort plan.
#[derive(Clone, Debug)]
pub struct MappedChaosCohort {
    /// Shared cohort plan.
    pub plan: CohortPlan,
    /// Selected KVM profile.
    pub kvm_profile: KvmRuntimeProfile,
    /// Exact effective memory bytes.
    pub memory: Vec<u8>,
    /// Exact effective disk bytes.
    pub disk: Arc<[u8]>,
    /// ChaosControl snapshot identity.
    pub snapshot_ref: ReceiptRef,
}

/// ChaosControl observation of one VM Cohort-backed run.
#[derive(Clone, Debug)]
pub struct ChaosCohortOutcome {
    /// Final shared lifecycle state.
    pub state: CohortState,
    /// Shared mechanism receipt.
    pub mechanism_receipt_ref: ReceiptRef,
    /// ChaosControl fault authority was not transferred.
    pub fault_authority_granted: bool,
    /// ChaosControl replay authority was not transferred.
    pub replay_authority_granted: bool,
    /// Release authority was not transferred.
    pub release_authority_granted: bool,
}

/// Adapter error with stable bounded classes.
#[derive(Debug)]
pub enum AdapterError {
    /// Mapping or profile input was denied.
    Admission(&'static str),
    /// Serialization failed.
    Serialization(serde_json::Error),
    /// Shared core denied the supplied facts.
    Core(&'static str),
    /// Shared KVM shell failed.
    Kvm(String),
    /// ChaosControl exact snapshot restore failed.
    Snapshot(String),
}

impl core::fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Admission(message) => write!(formatter, "adapter admission denied: {message}"),
            Self::Serialization(error) => {
                write!(formatter, "adapter serialization failed: {error}")
            }
            Self::Core(message) => write!(formatter, "VM Cohort core denied: {message}"),
            Self::Kvm(message) => write!(formatter, "VM Cohort KVM failed: {message}"),
            Self::Snapshot(message) => {
                write!(formatter, "ChaosControl snapshot restore failed: {message}")
            }
        }
    }
}

impl std::error::Error for AdapterError {}

impl From<serde_json::Error> for AdapterError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}
