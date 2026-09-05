use super::model::{
    observation_set_identity, validate_assembled, AssembledObservation, FindabilityError,
    MAX_EXACT_F64_INTEGER,
};
use std::collections::BTreeMap;

pub const FINDABILITY_REPORT_SCHEMA_VERSION: u32 = 1;
const REQUIRED_ASSUMPTION_COUNT: usize = 5;
pub const REQUIRED_ASSUMPTIONS: [&str; REQUIRED_ASSUMPTION_COUNT] = [
    "constant discovery rate within one generation",
    "one first-bug instance per subtree",
    "independence groups represent independent trials",
    "observation gaps are absent",
    "statistical confidence is not proof that a bug is absent",
];
const MINIMUM_FITTED_SUBTREES: usize = 2;
const MODEL_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.findability.model.v1\0";
const REPORT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.findability.report.v1\0";
const BLAKE3_PREFIX: &str = "blake3:";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindabilityPolicy {
    pub prior_shape: f64,
    pub prior_rate: f64,
    pub confidence_target: f64,
    pub maximum_projected_runs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindabilityStatus {
    Fitted,
    NoBugObserved,
    InsufficientSamples,
    IndependenceViolation,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExponentialFit {
    pub first_bug_count: usize,
    pub total_survival_time: u64,
    pub bug_rate: f64,
    pub mean_time_to_bug: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LomaxProjection {
    pub prior_shape: f64,
    pub prior_rate: f64,
    pub posterior_shape: f64,
    pub posterior_rate: f64,
    pub mean_subtree_exposure: f64,
    pub p_survival_next_run: f64,
    pub confidence_target: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_additional_runs: Option<u64>,
    pub projection_capped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependenceAssessment {
    pub supported: bool,
    pub baked_in_subtrees: Vec<String>,
    pub correlated_groups: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindabilityReport {
    pub schema_version: u32,
    pub generation_id: String,
    pub status: FindabilityStatus,
    pub observation_set_blake3: String,
    pub model_blake3: String,
    pub subtree_count: usize,
    pub first_bug_count: usize,
    pub total_survival_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exponential: Option<ExponentialFit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lomax: Option<LomaxProjection>,
    pub independence: IndependenceAssessment,
    pub assumptions: Vec<String>,
    pub report_blake3: String,
}

pub fn fit_findability(
    observations: &[AssembledObservation],
    policy: &FindabilityPolicy,
) -> Result<FindabilityReport, FindabilityError> {
    validate_assembled(observations)?;
    validate_policy(policy)?;
    let subtree_count = observations.len();
    let first_bug_count = observations
        .iter()
        .filter(|observation| observation.first_bug_at.is_some())
        .count();
    let total_survival_time = observations.iter().try_fold(0_u64, |total, observation| {
        total.checked_add(observation.exposure).ok_or_else(|| {
            FindabilityError::new("findability-time", "total survival time overflow")
        })
    })?;
    if total_survival_time > MAX_EXACT_F64_INTEGER {
        return Err(FindabilityError::new(
            "findability-time",
            "total survival time is not exactly representable by the model",
        ));
    }
    let observation_set_blake3 = observation_set_identity(observations)?;
    let model_blake3 = model_identity(policy)?;
    let independence = assess_independence(observations);
    let exponential = exponential_fit(first_bug_count, total_survival_time)?;

    let status = if first_bug_count == 0 {
        FindabilityStatus::NoBugObserved
    } else if subtree_count < MINIMUM_FITTED_SUBTREES {
        FindabilityStatus::InsufficientSamples
    } else if !independence.supported {
        FindabilityStatus::IndependenceViolation
    } else {
        FindabilityStatus::Fitted
    };
    let lomax = if status == FindabilityStatus::Fitted {
        Some(lomax_projection(
            first_bug_count,
            total_survival_time,
            subtree_count,
            policy,
        )?)
    } else {
        None
    };
    let generation_id = observations
        .first()
        .map(|observation| observation.generation_id.clone())
        .ok_or_else(|| FindabilityError::new("observation-empty", "missing generation"))?;
    let mut report = FindabilityReport {
        schema_version: FINDABILITY_REPORT_SCHEMA_VERSION,
        generation_id,
        status,
        observation_set_blake3,
        model_blake3,
        subtree_count,
        first_bug_count,
        total_survival_time,
        exponential,
        lomax,
        independence,
        assumptions: REQUIRED_ASSUMPTIONS
            .iter()
            .map(|assumption| (*assumption).to_string())
            .collect(),
        report_blake3: String::new(),
    };
    report.report_blake3 = report_identity(&report)?;
    Ok(report)
}

pub fn validate_report(
    report: &FindabilityReport,
    observations: &[AssembledObservation],
    policy: &FindabilityPolicy,
) -> Result<(), FindabilityError> {
    let expected = fit_findability(observations, policy)?;
    if report != &expected {
        return Err(FindabilityError::new(
            "findability-report-identity",
            "report input, model, output, or BLAKE3 identity drifted",
        ));
    }
    Ok(())
}

fn validate_policy(policy: &FindabilityPolicy) -> Result<(), FindabilityError> {
    if !policy.prior_shape.is_finite() || policy.prior_shape <= 0.0 {
        return Err(FindabilityError::new(
            "findability-policy",
            "prior_shape must be finite and positive",
        ));
    }
    if !policy.prior_rate.is_finite() || policy.prior_rate <= 0.0 {
        return Err(FindabilityError::new(
            "findability-policy",
            "prior_rate must be finite and positive",
        ));
    }
    if !policy.confidence_target.is_finite()
        || policy.confidence_target <= 0.0
        || policy.confidence_target >= 1.0
    {
        return Err(FindabilityError::new(
            "findability-policy",
            "confidence_target must be finite and strictly between zero and one",
        ));
    }
    if policy.maximum_projected_runs == 0 || policy.maximum_projected_runs > MAX_EXACT_F64_INTEGER {
        return Err(FindabilityError::new(
            "findability-policy",
            "maximum_projected_runs must be positive and exactly representable",
        ));
    }
    Ok(())
}

fn exponential_fit(
    first_bug_count: usize,
    total_survival_time: u64,
) -> Result<Option<ExponentialFit>, FindabilityError> {
    if total_survival_time == 0 {
        return Err(FindabilityError::new(
            "findability-time",
            "total survival time must be positive",
        ));
    }
    if first_bug_count == 0 {
        return Ok(None);
    }
    let bugs = first_bug_count as f64;
    let time = total_survival_time as f64;
    let bug_rate = bugs / time;
    let mean_time_to_bug = time / bugs;
    if !bug_rate.is_finite() || !mean_time_to_bug.is_finite() {
        return Err(FindabilityError::new(
            "findability-fit",
            "exponential fit produced a non-finite value",
        ));
    }
    Ok(Some(ExponentialFit {
        first_bug_count,
        total_survival_time,
        bug_rate,
        mean_time_to_bug,
    }))
}

fn assess_independence(observations: &[AssembledObservation]) -> IndependenceAssessment {
    let bugged = observations
        .iter()
        .filter(|observation| observation.first_bug_at.is_some())
        .map(|observation| observation.subtree_id.clone())
        .collect::<Vec<_>>();
    let baked_in_subtrees =
        if observations.len() >= MINIMUM_FITTED_SUBTREES && bugged.len() == observations.len() {
            bugged
        } else {
            Vec::new()
        };
    let mut groups = BTreeMap::new();
    for observation in observations {
        *groups
            .entry(observation.independence_group.as_str())
            .or_insert(0_usize) += 1;
    }
    let correlated_groups = groups
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(group, _)| group.to_string())
        .collect::<Vec<_>>();
    IndependenceAssessment {
        supported: baked_in_subtrees.is_empty() && correlated_groups.is_empty(),
        baked_in_subtrees,
        correlated_groups,
    }
}

fn lomax_projection(
    first_bug_count: usize,
    total_survival_time: u64,
    subtree_count: usize,
    policy: &FindabilityPolicy,
) -> Result<LomaxProjection, FindabilityError> {
    let posterior_shape = policy.prior_shape + first_bug_count as f64;
    let posterior_rate = policy.prior_rate + total_survival_time as f64;
    let mean_subtree_exposure = total_survival_time as f64 / subtree_count as f64;
    let p_survival_next_run =
        (posterior_rate / (posterior_rate + mean_subtree_exposure)).powf(posterior_shape);
    let target_survival = 1.0 - policy.confidence_target;
    let required_exposure = posterior_rate * (target_survival.powf(-1.0 / posterior_shape) - 1.0);
    let projected = (required_exposure / mean_subtree_exposure).ceil();
    if !posterior_shape.is_finite()
        || !posterior_rate.is_finite()
        || !mean_subtree_exposure.is_finite()
        || !p_survival_next_run.is_finite()
        || !required_exposure.is_finite()
        || !projected.is_finite()
        || projected < 0.0
    {
        return Err(FindabilityError::new(
            "findability-projection",
            "Lomax projection produced a non-finite or negative value",
        ));
    }
    let maximum = policy.maximum_projected_runs as f64;
    let projection_capped = projected > maximum;
    let projected_additional_runs = if projection_capped {
        None
    } else {
        Some(projected as u64)
    };
    Ok(LomaxProjection {
        prior_shape: policy.prior_shape,
        prior_rate: policy.prior_rate,
        posterior_shape,
        posterior_rate,
        mean_subtree_exposure,
        p_survival_next_run,
        confidence_target: policy.confidence_target,
        projected_additional_runs,
        projection_capped,
    })
}

fn model_identity(policy: &FindabilityPolicy) -> Result<String, FindabilityError> {
    let bytes = serde_json::to_vec(policy)
        .map_err(|error| FindabilityError::new("model-serialization", error.to_string()))?;
    Ok(domain_hash(MODEL_IDENTITY_DOMAIN, &bytes))
}

fn report_identity(report: &FindabilityReport) -> Result<String, FindabilityError> {
    #[derive(serde::Serialize)]
    struct Material<'a> {
        schema_version: u32,
        generation_id: &'a str,
        status: FindabilityStatus,
        observation_set_blake3: &'a str,
        model_blake3: &'a str,
        subtree_count: usize,
        first_bug_count: usize,
        total_survival_time: u64,
        exponential: &'a Option<ExponentialFit>,
        lomax: &'a Option<LomaxProjection>,
        independence: &'a IndependenceAssessment,
        assumptions: &'a [String],
    }
    let material = Material {
        schema_version: report.schema_version,
        generation_id: &report.generation_id,
        status: report.status,
        observation_set_blake3: &report.observation_set_blake3,
        model_blake3: &report.model_blake3,
        subtree_count: report.subtree_count,
        first_bug_count: report.first_bug_count,
        total_survival_time: report.total_survival_time,
        exponential: &report.exponential,
        lomax: &report.lomax,
        independence: &report.independence,
        assumptions: &report.assumptions,
    };
    let bytes = serde_json::to_vec(&material)
        .map_err(|error| FindabilityError::new("report-serialization", error.to_string()))?;
    Ok(domain_hash(REPORT_IDENTITY_DOMAIN, &bytes))
}

fn domain_hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
    format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex())
}
