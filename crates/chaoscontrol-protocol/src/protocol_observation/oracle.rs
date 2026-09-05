//! Consumer-owned semantic adapter and exact result binding.

#[path = "oracle/marker.rs"]
mod marker;
use super::*;
pub use marker::{bind_marker_snapshot, validate_marker_binding, MarkerSnapshotBinding};

const RESULT_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.oracle-result.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtocolVerdict {
    Pass,
    Fail,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleDecision {
    pub verdict: ProtocolVerdict,
    pub diagnostic_refs: Vec<String>,
    pub work_items: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolOracleResult {
    pub adapter_ref: String,
    pub cohort_identity: String,
    pub result_ref: String,
    pub decision: OracleDecision,
}

/// A reviewed pure adapter. The consumer owns its source and semantic authority.
/// The composition root must bind this contract before it starts a guest.
pub trait ProtocolOracle {
    fn adapter_ref(&self) -> &str;
    fn projection_schema_ref(&self) -> &str;
    fn authority(&self) -> OracleAuthority;
    fn evaluate(
        &self,
        cohort: &CohortResult,
        work_limit: u32,
    ) -> Result<OracleDecision, ProtocolObservationError>;
}

pub fn validate_oracle_adapter<O: ProtocolOracle + ?Sized>(
    profile: &AdmittedProfile,
    oracle: &O,
) -> Result<(), ProtocolObservationError> {
    validate_profile_identity(profile)?;
    if oracle.authority() != OracleAuthority::ConsumerIndependent {
        return Err(ProtocolObservationError::RuntimeSelfOracle);
    }
    if oracle.adapter_ref() != profile.profile.oracle.adapter_ref
        || oracle.projection_schema_ref() != profile.profile.projection_schema_ref
    {
        return Err(ProtocolObservationError::OracleMismatch);
    }
    Ok(())
}

pub fn run_consumer_oracle<O: ProtocolOracle + ?Sized>(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    oracle: &O,
) -> Result<ProtocolOracleResult, ProtocolObservationError> {
    validate_oracle_adapter(profile, oracle)?;
    validate_cohort(profile, cohort)?;
    if cohort.classification != CohortClassification::Complete {
        return Err(ProtocolObservationError::CohortNotComplete);
    }
    let work_limit = profile.profile.bounds.max_oracle_work_items;
    if cohort.records.len() > work_limit as usize {
        return Err(ProtocolObservationError::OracleWorkExceeded);
    }
    let decision = oracle.evaluate(cohort, work_limit)?;
    let mut result = ProtocolOracleResult {
        adapter_ref: profile.profile.oracle.adapter_ref.clone(),
        cohort_identity: cohort.cohort_identity.clone(),
        result_ref: String::new(),
        decision,
    };
    validate_decision(profile, &result.decision)?;
    result.result_ref = result_identity(&result)?;
    Ok(result)
}

pub fn validate_oracle_result(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    result: &ProtocolOracleResult,
) -> Result<(), ProtocolObservationError> {
    validate_cohort(profile, cohort)?;
    if cohort.classification != CohortClassification::Complete {
        return Err(ProtocolObservationError::CohortNotComplete);
    }
    if result.adapter_ref != profile.profile.oracle.adapter_ref
        || result.cohort_identity != cohort.cohort_identity
    {
        return Err(ProtocolObservationError::OracleMismatch);
    }
    validate_decision(profile, &result.decision)?;
    if result.result_ref != result_identity(result)? {
        return Err(ProtocolObservationError::IdentityMismatch("oracle-result"));
    }
    Ok(())
}

fn validate_decision(
    profile: &AdmittedProfile,
    result: &OracleDecision,
) -> Result<(), ProtocolObservationError> {
    if result.work_items == 0 || result.work_items > profile.profile.bounds.max_oracle_work_items {
        return Err(ProtocolObservationError::OracleWorkExceeded);
    }
    if result.diagnostic_refs.len() > profile.profile.bounds.max_diagnostic_refs as usize {
        return Err(ProtocolObservationError::BoundExceeded(
            "oracle-diagnostics",
        ));
    }
    if result.verdict == ProtocolVerdict::Unsupported && result.diagnostic_refs.is_empty() {
        return Err(ProtocolObservationError::OracleMismatch);
    }
    if !result
        .diagnostic_refs
        .iter()
        .zip(result.diagnostic_refs.iter().skip(1))
        .all(|(left, right)| left < right)
    {
        return Err(ProtocolObservationError::NonCanonicalOrder(
            "oracle-diagnostics",
        ));
    }
    for value in &result.diagnostic_refs {
        validate_exact_reference(value, "diagnostic")?;
    }
    Ok(())
}

fn result_identity(result: &ProtocolOracleResult) -> Result<String, ProtocolObservationError> {
    let bytes = serde_json::to_vec(&(
        &result.adapter_ref,
        &result.cohort_identity,
        &result.decision,
    ))
    .map_err(|_| ProtocolObservationError::InvalidSchema)?;
    Ok(digest_reference("oracle-result", RESULT_DOMAIN, &bytes))
}
