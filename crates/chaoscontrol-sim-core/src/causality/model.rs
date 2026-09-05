use std::fmt;

pub const MAX_INTERLEAVING_STEPS: usize = 4_096;
pub const MAX_ATTRIBUTION_CANDIDATES: usize = 1_024;
const MAX_IDENTIFIER_BYTES: usize = 256;
const BLAKE3_HEX_BYTES: usize = 64;
const BLAKE3_PREFIX: &str = "blake3:";
const STEP_SET_DOMAIN: &[u8] = b"chaoscontrol.causality.step-set.v1\0";
const CANDIDATE_SET_DOMAIN: &[u8] = b"chaoscontrol.causality.candidate-set.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterleavingStep {
    pub step_id: String,
    pub sequence: u64,
    pub policy_blake3: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CauseClass {
    Seed,
    FaultSchedule,
    DeclaredEvent,
    VariantPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CauseCandidate {
    pub candidate_id: String,
    pub class: CauseClass,
    pub evidence_blake3: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisBudget {
    pub minimization_executions: u64,
    pub attribution_executions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CausalityError {
    pub class: &'static str,
    pub detail: String,
}

impl CausalityError {
    pub(crate) fn new(class: &'static str, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for CausalityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.class, self.detail)
    }
}

impl std::error::Error for CausalityError {}

pub fn validate_steps(steps: &[InterleavingStep]) -> Result<(), CausalityError> {
    if steps.is_empty() || steps.len() > MAX_INTERLEAVING_STEPS {
        return Err(CausalityError::new(
            "causality-step-bound",
            "interleaving steps are empty or exceed the supported bound",
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut sequences = std::collections::BTreeSet::new();
    let mut prior_sequence = None;
    for step in steps {
        validate_identifier("step_id", &step.step_id)?;
        validate_digest("policy_blake3", &step.policy_blake3)?;
        if !ids.insert(step.step_id.as_str()) {
            return Err(CausalityError::new(
                "causality-step-identity",
                "duplicate interleaving step identity",
            ));
        }
        if !sequences.insert(step.sequence)
            || prior_sequence.is_some_and(|prior| prior >= step.sequence)
        {
            return Err(CausalityError::new(
                "causality-step-order",
                "step sequences must be unique and strictly increasing",
            ));
        }
        prior_sequence = Some(step.sequence);
    }
    Ok(())
}

pub fn validate_candidates(candidates: &[CauseCandidate]) -> Result<(), CausalityError> {
    if candidates.is_empty() || candidates.len() > MAX_ATTRIBUTION_CANDIDATES {
        return Err(CausalityError::new(
            "causality-candidate-bound",
            "attribution candidates are empty or exceed the supported bound",
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut previous = None;
    for candidate in candidates {
        validate_identifier("candidate_id", &candidate.candidate_id)?;
        validate_digest("evidence_blake3", &candidate.evidence_blake3)?;
        if !ids.insert(candidate.candidate_id.as_str()) {
            return Err(CausalityError::new(
                "causality-candidate-identity",
                "duplicate attribution candidate identity",
            ));
        }
        if previous.is_some_and(|prior: &str| prior >= candidate.candidate_id.as_str()) {
            return Err(CausalityError::new(
                "causality-candidate-order",
                "attribution candidates must use canonical identity order",
            ));
        }
        previous = Some(candidate.candidate_id.as_str());
    }
    Ok(())
}

pub fn validate_budget(budget: AnalysisBudget) -> Result<(), CausalityError> {
    if budget.minimization_executions == 0 || budget.attribution_executions == 0 {
        return Err(CausalityError::new(
            "causality-budget",
            "minimization and attribution budgets must be positive",
        ));
    }
    Ok(())
}

pub fn step_set_identity(steps: &[InterleavingStep]) -> Result<String, CausalityError> {
    validate_steps(steps)?;
    let bytes = serde_json::to_vec(steps)
        .map_err(|error| CausalityError::new("causality-step-serialization", error.to_string()))?;
    Ok(domain_hash(STEP_SET_DOMAIN, &bytes))
}

pub fn candidate_set_identity(candidates: &[CauseCandidate]) -> Result<String, CausalityError> {
    validate_candidates(candidates)?;
    let bytes = serde_json::to_vec(candidates).map_err(|error| {
        CausalityError::new("causality-candidate-serialization", error.to_string())
    })?;
    Ok(domain_hash(CANDIDATE_SET_DOMAIN, &bytes))
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), CausalityError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(CausalityError::new(
            "causality-identifier",
            format!("{field} is empty or exceeds the supported bound"),
        ));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    }) {
        return Err(CausalityError::new(
            "causality-identifier",
            format!("{field} contains a non-canonical byte"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_digest(field: &'static str, value: &str) -> Result<(), CausalityError> {
    let Some(hex) = value.strip_prefix(BLAKE3_PREFIX) else {
        return Err(CausalityError::new(
            "causality-digest",
            format!("{field} must use a BLAKE3 identity"),
        ));
    };
    if hex.len() != BLAKE3_HEX_BYTES
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(CausalityError::new(
            "causality-digest",
            format!("{field} has malformed lowercase BLAKE3 hex"),
        ));
    }
    Ok(())
}

pub(crate) fn domain_hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
    format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex())
}
