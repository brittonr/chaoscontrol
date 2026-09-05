use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_sim_core::causality::{
    candidate_set_identity, rank_candidates, step_set_identity, validate_budget, AnalysisBudget,
    AttributionObservation, AttributionReport, CauseCandidate, DdminState, InterleavingStep,
    MinimizationCandidate, MinimizationResult,
};
use serde::ser::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

pub const CAUSALITY_REQUEST_SCHEMA_VERSION: u32 = 1;
pub const CAUSALITY_RECEIPT_SCHEMA_VERSION: u32 = 1;
pub const MAX_CAUSALITY_ARTIFACT_BYTES: u64 = 4 * 1_024 * 1_024;
const MAX_SNAPSHOT_IDENTITIES: usize = 1_024;
const REQUEST_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.causality.request.v1\0";
const RECEIPT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.causality.receipt.v1\0";
const BLAKE3_PREFIX: &str = "blake3:";
const BLAKE3_HEX_BYTES: usize = 64;
const ATTRIBUTION_ATTEMPT: u32 = 0;
const REQUIRED_NON_CLAIM_COUNT: usize = 4;
const REQUIRED_NON_CLAIMS: [&str; REQUIRED_NON_CLAIM_COUNT] = [
    "probable cause ranking is not proof of a unique cause",
    "partial analysis is not complete attribution",
    "minimization is bounded by supplied replay outcomes",
    "analysis evidence is not release eligibility",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CausalityRequest {
    pub schema_version: u32,
    pub request_blake3: String,
    pub replay_verdict_blake3: String,
    pub snapshot_blake3s: Vec<String>,
    pub steps: Vec<InterleavingStep>,
    pub candidates: Vec<CauseCandidate>,
    pub budget: AnalysisBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionBinding {
    pub replay_verdict_blake3: String,
    pub snapshot_blake3s: Vec<String>,
    pub reproduced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimizationExecution {
    pub candidate: MinimizationCandidate,
    pub binding: ExecutionBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionExecution {
    pub candidate_id: String,
    pub attempt: u32,
    pub binding: ExecutionBinding,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CausalityReceipt {
    pub schema_version: u32,
    pub receipt_blake3: String,
    pub request_blake3: String,
    pub replay_verdict_blake3: String,
    pub snapshot_blake3s: Vec<String>,
    pub step_set_blake3: String,
    pub candidate_set_blake3: String,
    pub minimization: MinimizationResult,
    pub attribution: AttributionReport,
    pub minimization_executions: Vec<MinimizationExecution>,
    pub attribution_executions: Vec<AttributionExecution>,
    pub non_claims: Vec<String>,
}

pub fn read_causality_request_path(path: impl AsRef<Path>) -> EvidenceResult<CausalityRequest> {
    let path = path.as_ref();
    let bytes =
        crate::bounded_file::read_bounded_regular_bytes(path, MAX_CAUSALITY_ARTIFACT_BYTES)?;
    let request = serde_json::from_slice::<CausalityRequest>(&bytes).map_err(|error| {
        EvidenceError::new(format!(
            "{}: causality request is not a closed typed artifact: {error}",
            path.display()
        ))
    })?;
    validate_causality_request(&request)?;
    Ok(request)
}

pub fn read_causality_receipt_path(
    request: &CausalityRequest,
    path: impl AsRef<Path>,
) -> EvidenceResult<CausalityReceipt> {
    let path = path.as_ref();
    let bytes =
        crate::bounded_file::read_bounded_regular_bytes(path, MAX_CAUSALITY_ARTIFACT_BYTES)?;
    let receipt = serde_json::from_slice::<CausalityReceipt>(&bytes).map_err(|error| {
        EvidenceError::new(format!(
            "{}: causality receipt is not a closed typed artifact: {error}",
            path.display()
        ))
    })?;
    validate_causality_receipt(request, &receipt)?;
    Ok(receipt)
}

pub trait CausalityExecutor {
    fn execute_interleaving(
        &mut self,
        candidate: &MinimizationCandidate,
    ) -> EvidenceResult<ExecutionBinding>;

    fn execute_neutralization(
        &mut self,
        candidate: &CauseCandidate,
    ) -> EvidenceResult<ExecutionBinding>;
}

pub fn bind_causality_request(
    replay_verdict_blake3: impl Into<String>,
    mut snapshot_blake3s: Vec<String>,
    steps: Vec<InterleavingStep>,
    mut candidates: Vec<CauseCandidate>,
    budget: AnalysisBudget,
) -> EvidenceResult<CausalityRequest> {
    snapshot_blake3s.sort();
    candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let mut request = CausalityRequest {
        schema_version: CAUSALITY_REQUEST_SCHEMA_VERSION,
        request_blake3: String::new(),
        replay_verdict_blake3: replay_verdict_blake3.into(),
        snapshot_blake3s,
        steps,
        candidates,
        budget,
    };
    request.request_blake3 = request_identity(&request)?;
    validate_causality_request(&request)?;
    Ok(request)
}

pub fn validate_causality_request(request: &CausalityRequest) -> EvidenceResult<()> {
    require(
        request.schema_version == CAUSALITY_REQUEST_SCHEMA_VERSION,
        "unsupported causality request schema",
    )?;
    validate_digest("replay_verdict_blake3", &request.replay_verdict_blake3)?;
    validate_snapshots(&request.snapshot_blake3s)?;
    step_set_identity(&request.steps).map_err(|error| EvidenceError::new(error.to_string()))?;
    candidate_set_identity(&request.candidates)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    validate_budget(request.budget).map_err(|error| EvidenceError::new(error.to_string()))?;
    let expected = request_identity(request)?;
    require(
        request.request_blake3 == expected,
        "causality request BLAKE3 identity drifted",
    )
}

pub fn run_causality_analysis<E: CausalityExecutor>(
    request: &CausalityRequest,
    executor: &mut E,
) -> EvidenceResult<CausalityReceipt> {
    validate_causality_request(request)?;
    let mut minimizer = DdminState::new(
        request.steps.clone(),
        request.budget.minimization_executions,
    )
    .map_err(|error| EvidenceError::new(error.to_string()))?;
    let mut minimization_executions = Vec::new();
    while let Some(candidate) = minimizer
        .next_candidate()
        .map_err(|error| EvidenceError::new(error.to_string()))?
    {
        let binding = executor.execute_interleaving(&candidate)?;
        validate_execution_binding(request, &binding)?;
        minimizer
            .record_outcome(&candidate.candidate_blake3, binding.reproduced)
            .map_err(|error| EvidenceError::new(error.to_string()))?;
        minimization_executions.push(MinimizationExecution { candidate, binding });
    }
    let minimization = minimizer.result();

    let mut attribution_executions = Vec::new();
    for candidate in &request.candidates {
        let spent = u64::try_from(attribution_executions.len())
            .map_err(|_| EvidenceError::new("attribution execution count exceeds u64"))?;
        if spent >= request.budget.attribution_executions {
            break;
        }
        let binding = executor.execute_neutralization(candidate)?;
        validate_execution_binding(request, &binding)?;
        attribution_executions.push(AttributionExecution {
            candidate_id: candidate.candidate_id.clone(),
            attempt: ATTRIBUTION_ATTEMPT,
            binding,
        });
    }
    let attribution_observations = attribution_executions
        .iter()
        .map(|execution| AttributionObservation {
            candidate_id: execution.candidate_id.clone(),
            attempt: execution.attempt,
            neutralized_reproduced: execution.binding.reproduced,
        })
        .collect::<Vec<_>>();
    let attribution = rank_candidates(
        &request.candidates,
        &attribution_observations,
        request.budget.attribution_executions,
    )
    .map_err(|error| EvidenceError::new(error.to_string()))?;
    let mut receipt = CausalityReceipt {
        schema_version: CAUSALITY_RECEIPT_SCHEMA_VERSION,
        receipt_blake3: String::new(),
        request_blake3: request.request_blake3.clone(),
        replay_verdict_blake3: request.replay_verdict_blake3.clone(),
        snapshot_blake3s: request.snapshot_blake3s.clone(),
        step_set_blake3: step_set_identity(&request.steps)
            .map_err(|error| EvidenceError::new(error.to_string()))?,
        candidate_set_blake3: candidate_set_identity(&request.candidates)
            .map_err(|error| EvidenceError::new(error.to_string()))?,
        minimization,
        attribution,
        minimization_executions,
        attribution_executions,
        non_claims: required_non_claims(),
    };
    receipt.receipt_blake3 = receipt_identity(&receipt)?;
    validate_causality_receipt(request, &receipt)?;
    Ok(receipt)
}

pub fn validate_causality_receipt(
    request: &CausalityRequest,
    receipt: &CausalityReceipt,
) -> EvidenceResult<()> {
    validate_causality_request(request)?;
    require(
        receipt.schema_version == CAUSALITY_RECEIPT_SCHEMA_VERSION,
        "unsupported causality receipt schema",
    )?;
    require(
        receipt.request_blake3 == request.request_blake3
            && receipt.replay_verdict_blake3 == request.replay_verdict_blake3
            && receipt.snapshot_blake3s == request.snapshot_blake3s,
        "causality receipt input identity drifted",
    )?;
    require(
        receipt.step_set_blake3
            == step_set_identity(&request.steps)
                .map_err(|error| EvidenceError::new(error.to_string()))?
            && receipt.candidate_set_blake3
                == candidate_set_identity(&request.candidates)
                    .map_err(|error| EvidenceError::new(error.to_string()))?,
        "causality receipt set identity drifted",
    )?;
    require(
        receipt.non_claims == required_non_claims(),
        "causality receipt non-claims drifted",
    )?;

    let mut minimizer = DdminState::new(
        request.steps.clone(),
        request.budget.minimization_executions,
    )
    .map_err(|error| EvidenceError::new(error.to_string()))?;
    for execution in &receipt.minimization_executions {
        let expected = minimizer
            .next_candidate()
            .map_err(|error| EvidenceError::new(error.to_string()))?
            .ok_or_else(|| EvidenceError::new("unexpected minimization execution"))?;
        require(
            execution.candidate == expected,
            "minimization candidate sequence drifted",
        )?;
        validate_execution_binding(request, &execution.binding)?;
        minimizer
            .record_outcome(
                &execution.candidate.candidate_blake3,
                execution.binding.reproduced,
            )
            .map_err(|error| EvidenceError::new(error.to_string()))?;
    }
    require(
        minimizer
            .next_candidate()
            .map_err(|error| EvidenceError::new(error.to_string()))?
            .is_none(),
        "minimization receipt omitted planned executions",
    )?;
    require(
        minimizer.result() == receipt.minimization,
        "minimization result drifted from execution evidence",
    )?;

    let mut attribution_observations = Vec::new();
    for (expected_index, execution) in receipt.attribution_executions.iter().enumerate() {
        let expected_candidate = request.candidates.get(expected_index).ok_or_else(|| {
            EvidenceError::new("attribution receipt contains an unknown candidate execution")
        })?;
        require(
            execution.candidate_id == expected_candidate.candidate_id
                && execution.attempt == ATTRIBUTION_ATTEMPT,
            "attribution candidate or attempt order drifted",
        )?;
        validate_execution_binding(request, &execution.binding)?;
        attribution_observations.push(AttributionObservation {
            candidate_id: execution.candidate_id.clone(),
            attempt: execution.attempt,
            neutralized_reproduced: execution.binding.reproduced,
        });
    }
    let expected_attribution = rank_candidates(
        &request.candidates,
        &attribution_observations,
        request.budget.attribution_executions,
    )
    .map_err(|error| EvidenceError::new(error.to_string()))?;
    require(
        receipt.attribution == expected_attribution,
        "attribution result drifted from execution evidence",
    )?;
    require(
        receipt.receipt_blake3 == receipt_identity(receipt)?,
        "causality receipt BLAKE3 identity drifted",
    )
}

fn validate_execution_binding(
    request: &CausalityRequest,
    binding: &ExecutionBinding,
) -> EvidenceResult<()> {
    require(
        binding.replay_verdict_blake3 == request.replay_verdict_blake3
            && binding.snapshot_blake3s == request.snapshot_blake3s,
        "candidate execution identity drifted from the admitted replay inputs",
    )
}

fn validate_snapshots(snapshots: &[String]) -> EvidenceResult<()> {
    require(
        !snapshots.is_empty() && snapshots.len() <= MAX_SNAPSHOT_IDENTITIES,
        "snapshot identities are empty or exceed the supported bound",
    )?;
    let mut previous = None;
    let mut unique = BTreeSet::new();
    for snapshot in snapshots {
        validate_digest("snapshot_blake3", snapshot)?;
        require(
            unique.insert(snapshot.as_str()),
            "duplicate snapshot identity",
        )?;
        require(
            !previous.is_some_and(|prior: &str| prior >= snapshot.as_str()),
            "snapshot identities are not in canonical order",
        )?;
        previous = Some(snapshot.as_str());
    }
    Ok(())
}

fn request_identity(request: &CausalityRequest) -> EvidenceResult<String> {
    #[derive(serde::Serialize)]
    struct Material<'a> {
        schema_version: u32,
        replay_verdict_blake3: &'a str,
        snapshot_blake3s: &'a [String],
        steps: &'a [InterleavingStep],
        candidates: &'a [CauseCandidate],
        budget: AnalysisBudget,
    }
    let material = Material {
        schema_version: request.schema_version,
        replay_verdict_blake3: &request.replay_verdict_blake3,
        snapshot_blake3s: &request.snapshot_blake3s,
        steps: &request.steps,
        candidates: &request.candidates,
        budget: request.budget,
    };
    identity(REQUEST_IDENTITY_DOMAIN, &material)
}

fn receipt_identity(receipt: &CausalityReceipt) -> EvidenceResult<String> {
    #[derive(serde::Serialize)]
    struct Material<'a> {
        schema_version: u32,
        request_blake3: &'a str,
        replay_verdict_blake3: &'a str,
        snapshot_blake3s: &'a [String],
        step_set_blake3: &'a str,
        candidate_set_blake3: &'a str,
        minimization: &'a MinimizationResult,
        attribution: &'a AttributionReport,
        minimization_executions: &'a [MinimizationExecution],
        attribution_executions: &'a [AttributionExecution],
        non_claims: &'a [String],
    }
    let material = Material {
        schema_version: receipt.schema_version,
        request_blake3: &receipt.request_blake3,
        replay_verdict_blake3: &receipt.replay_verdict_blake3,
        snapshot_blake3s: &receipt.snapshot_blake3s,
        step_set_blake3: &receipt.step_set_blake3,
        candidate_set_blake3: &receipt.candidate_set_blake3,
        minimization: &receipt.minimization,
        attribution: &receipt.attribution,
        minimization_executions: &receipt.minimization_executions,
        attribution_executions: &receipt.attribution_executions,
        non_claims: &receipt.non_claims,
    };
    identity(RECEIPT_IDENTITY_DOMAIN, &material)
}

fn identity<T: Serialize>(domain: &[u8], value: &T) -> EvidenceResult<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len())
        .map_err(|_| EvidenceError::new("causality identity input exceeds u64"))?;
    hasher.update(&length.to_le_bytes());
    hasher.update(&bytes);
    Ok(format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex()))
}

fn validate_digest(field: &'static str, value: &str) -> EvidenceResult<()> {
    let Some(hex) = value.strip_prefix(BLAKE3_PREFIX) else {
        return Err(EvidenceError::new(format!(
            "{field} must use a BLAKE3 identity"
        )));
    };
    require(
        hex.len() == BLAKE3_HEX_BYTES
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')),
        "malformed lowercase BLAKE3 identity",
    )
}

fn required_non_claims() -> Vec<String> {
    REQUIRED_NON_CLAIMS
        .iter()
        .map(|item| (*item).to_string())
        .collect()
}

fn require(condition: bool, message: &'static str) -> EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(EvidenceError::new(message))
    }
}
