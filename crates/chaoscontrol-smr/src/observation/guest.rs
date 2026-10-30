//! Intake for externally executed multi-node guests. This module never launches
//! an in-process consensus model and never manufactures execution receipts.
use super::{assemble_cohort, error, Completeness, Drain, ProtocolObservation, ProtocolProfile};
use crate::{
    canonical_value_ref, validate_digest_ref, validate_history, ChainObservation, HistoryReport,
    SmrResult, SmrWorkloadPlan,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestSource {
    pub guest_id: u32,
    pub executable_ref: String,
    pub source_revision_ref: String,
    pub adapter_ref: String,
    pub launch_contract_ref: String,
}

/// Implemented by the VMM/workload shell, not the runtime's success self-report.
/// The shell must retain the actual run artifacts identified by execution_ref.
pub trait MultiNodeGuestIntake {
    fn sources(&self) -> &[GuestSource];
    fn execution_ref(&self) -> &str;
    fn observations(&self) -> &[ProtocolObservation];
    fn drains(&self) -> &[Drain];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestHistory {
    pub execution_ref: String,
    pub sources_ref: String,
    pub cohort_ref: String,
    pub oracle_ref: String,
    pub history: HistoryReport,
}

/// Source/transport validation is not independent attestation of execution.
/// The shell must validate retained artifact hashes before calling this hook.
pub fn validate_sources(
    profile: &ProtocolProfile,
    intake: &impl MultiNodeGuestIntake,
) -> SmrResult<String> {
    validate_digest_ref("guest execution", intake.execution_ref())?;
    let guests: std::collections::BTreeSet<_> =
        profile.participants.iter().map(|p| p.guest_id).collect();
    let sources = intake.sources();
    let actual: std::collections::BTreeSet<_> = sources.iter().map(|s| s.guest_id).collect();
    if guests.len() < 2 || guests != actual || sources.len() != actual.len() {
        return Err(error(
            "guest-intake",
            "exact distinct multi-node guest sources required",
        ));
    }
    for source in sources {
        for value in [
            &source.executable_ref,
            &source.source_revision_ref,
            &source.adapter_ref,
            &source.launch_contract_ref,
        ] {
            validate_digest_ref("guest source artifact", value)?;
        }
    }
    canonical_value_ref(b"chaoscontrol.protocol-guest-sources.v1\0", &sources)
}

/// Inline projection contract: exactly one serialized ChainObservation per record.
/// Generic protocol adapters can instead consume complete cohorts directly.
// r[impl chaoscontrol.protocol_observation.oracle_boundary]
pub fn validate_guest_history(
    profile: &ProtocolProfile,
    boundary_ref: &str,
    plan: &SmrWorkloadPlan,
    intake: &impl MultiNodeGuestIntake,
) -> SmrResult<GuestHistory> {
    let sources_ref = validate_sources(profile, intake)?;
    let cohort = assemble_cohort(
        profile,
        boundary_ref,
        intake.observations(),
        intake.drains(),
    )?;
    if cohort.completeness != Completeness::Complete {
        return Err(error(
            "incomplete",
            "consumer oracle requires a complete cohort",
        ));
    }
    if intake.observations().len() as u64 > profile.bounds.oracle_work {
        return Err(error("bound", "oracle work bound exceeded"));
    }
    let mut observations: Vec<ChainObservation> = Vec::new();
    for record in intake.observations() {
        let bytes = record
            .projection
            .as_ref()
            .ok_or_else(|| error("unsupported", "chain oracle requires inline projection"))?;
        let observation: ChainObservation =
            serde_json::from_slice(bytes).map_err(|e| error("malformed", e.to_string()))?;
        if Some(observation.replica_id.as_str()) != record.participant_ref.strip_prefix("blake3:")
            || observation.event_sequence != record.source_sequence
        {
            return Err(error(
                "conflicting",
                "chain projection source differs from envelope",
            ));
        }
        observations.push(observation);
    }
    let history = validate_history(plan, &observations)?;
    Ok(GuestHistory {
        execution_ref: intake.execution_ref().into(),
        sources_ref,
        cohort_ref: cohort.cohort_ref,
        oracle_ref: profile.independent_oracle_ref.clone(),
        history,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceClass {
    Instrumentation,
    GuestExecution,
}

/// Neither intake nor a cohort proves deterministic or snapshot-backed replay.
// r[impl chaoscontrol.protocol_observation.evidence]
pub fn validate_claim(class: EvidenceClass, claim: &str) -> SmrResult<()> {
    match (class, claim) {
        (_, "bounded-instrumentation")
        | (EvidenceClass::GuestExecution, "bounded-guest-observations") => Ok(()),
        _ => Err(error(
            "claim",
            "unsupported proof, replay, or execution promotion",
        )),
    }
}
