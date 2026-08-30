//! Pure, bounded state-machine-replication chain workload semantics.
//!
//! This crate does not perform I/O, read clocks, inspect consensus internals, or
//! control faults. Product shells supply admitted facts and retain authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

pub mod phenomena;

pub const PROFILE_SCHEMA: &str = "chaoscontrol.smr-workload-profile.v1";
pub const RECEIPT_SCHEMA: &str = "chaoscontrol.smr-workload-receipt.v1";
pub const GENESIS_DOMAIN: &[u8] = b"chaoscontrol.smr-chain.genesis.v1\0";
pub const TRANSITION_DOMAIN: &[u8] = b"chaoscontrol.smr-chain.transition.v1\0";
pub const PROFILE_DOMAIN: &[u8] = b"chaoscontrol.smr-chain.profile.v1\0";
pub const HISTORY_REPORT_DOMAIN: &[u8] = b"chaoscontrol.smr-chain.history-report.v1\0";
pub const REPLAY_COMPARISON_DOMAIN: &[u8] = b"chaoscontrol.smr-chain.replay-comparison.v1\0";
pub const DIGEST_PREFIX: &str = "blake3:";
pub const DIGEST_HEX_LENGTH: usize = 64;
pub const DIGEST_BYTE_LENGTH: usize = 32;
pub const MAX_REFERENCE_LENGTH: usize = 512;
pub const SUPPORTED_MAXIMUM_COMMANDS: u64 = 65_536;
pub const SUPPORTED_MAXIMUM_COMMAND_BYTES: u64 = 1_024 * 1_024;
pub const SUPPORTED_MAXIMUM_CLIENTS: u64 = 4_096;
pub const SUPPORTED_MAXIMUM_CONCURRENCY: u64 = 4_096;
pub const SUPPORTED_MAXIMUM_VIRTUAL_PROGRESS: u64 = 1_000_000_000;
pub const SUPPORTED_MAXIMUM_TRACE_EVENTS: u64 = 1_000_000;
pub const SUPPORTED_MAXIMUM_FAULT_ACTIONS: u64 = 65_536;
pub const SUPPORTED_MAXIMUM_REPLAY_EVENTS: u64 = 1_000_000;
pub const SUPPORTED_MAXIMUM_EVIDENCE_BYTES: u64 = 16 * 1_024 * 1_024;
pub const SUPPORTED_MAXIMUM_REDUCTION_ATTEMPTS: u64 = 65_536;
pub const MIN_COMMAND_INDEX: u64 = 1;
pub const NO_FAULT_CONTROL_ID: &str = "no-fault-control";
pub const SUPPORTED_MAXIMUM_OPTIONAL_FEATURES: usize = 64;
pub const SUPPORTED_FAULT_STAGE_COUNT: usize = 6;
pub const REQUIRED_NON_CLAIMS: [&str; 7] = [
    "not universal SMR correctness",
    "not consensus correctness",
    "not durability proof",
    "not linearizability proof",
    "not Byzantine tolerance proof",
    "not security proof",
    "not release eligibility",
];

/// Requirement evidence markers for the accepted Cairn specification.
/// r[chaoscontrol.smr_chain.profile]
/// r[chaoscontrol.smr_chain.transition]
/// r[chaoscontrol.smr_chain.history]
/// r[chaoscontrol.smr_chain.safety]
/// r[chaoscontrol.smr_chain.liveness]
/// r[chaoscontrol.smr_chain.indefinite_outcomes]
/// r[chaoscontrol.smr_chain.adapter]
/// r[chaoscontrol.smr_chain.fault_campaign]
/// r[chaoscontrol.smr_chain.replay]
/// r[chaoscontrol.smr_chain.evidence]
/// r[chaoscontrol.smr_chain.boundary]
/// r[chaoscontrol.smr_chain.validation]
pub const REQUIREMENT_MARKERS: [&str; 12] = [
    "chaoscontrol.smr_chain.profile",
    "chaoscontrol.smr_chain.transition",
    "chaoscontrol.smr_chain.history",
    "chaoscontrol.smr_chain.safety",
    "chaoscontrol.smr_chain.liveness",
    "chaoscontrol.smr_chain.indefinite_outcomes",
    "chaoscontrol.smr_chain.adapter",
    "chaoscontrol.smr_chain.fault_campaign",
    "chaoscontrol.smr_chain.replay",
    "chaoscontrol.smr_chain.evidence",
    "chaoscontrol.smr_chain.boundary",
    "chaoscontrol.smr_chain.validation",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmrError {
    pub class: &'static str,
    pub detail: String,
}

impl SmrError {
    fn new(class: &'static str, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for SmrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.class, self.detail)
    }
}

impl std::error::Error for SmrError {}

pub type SmrResult<T> = Result<T, SmrError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationMode {
    Lossless,
    Sampled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadBounds {
    pub maximum_commands: u64,
    pub maximum_command_bytes: u64,
    pub maximum_clients: u32,
    pub maximum_concurrency: u32,
    pub maximum_virtual_progress: u64,
    pub maximum_trace_events: u64,
    pub maximum_fault_actions: u64,
    pub maximum_replay_events: u64,
    pub maximum_evidence_bytes: u64,
    pub maximum_reduction_attempts: u64,
}

impl WorkloadBounds {
    fn validate(&self) -> SmrResult<()> {
        let values = [
            ("maximum_commands", self.maximum_commands),
            ("maximum_command_bytes", self.maximum_command_bytes),
            ("maximum_clients", u64::from(self.maximum_clients)),
            ("maximum_concurrency", u64::from(self.maximum_concurrency)),
            ("maximum_virtual_progress", self.maximum_virtual_progress),
            ("maximum_trace_events", self.maximum_trace_events),
            ("maximum_fault_actions", self.maximum_fault_actions),
            ("maximum_replay_events", self.maximum_replay_events),
            ("maximum_evidence_bytes", self.maximum_evidence_bytes),
            (
                "maximum_reduction_attempts",
                self.maximum_reduction_attempts,
            ),
        ];
        let supported_maxima = [
            SUPPORTED_MAXIMUM_COMMANDS,
            SUPPORTED_MAXIMUM_COMMAND_BYTES,
            SUPPORTED_MAXIMUM_CLIENTS,
            SUPPORTED_MAXIMUM_CONCURRENCY,
            SUPPORTED_MAXIMUM_VIRTUAL_PROGRESS,
            SUPPORTED_MAXIMUM_TRACE_EVENTS,
            SUPPORTED_MAXIMUM_FAULT_ACTIONS,
            SUPPORTED_MAXIMUM_REPLAY_EVENTS,
            SUPPORTED_MAXIMUM_EVIDENCE_BYTES,
            SUPPORTED_MAXIMUM_REDUCTION_ATTEMPTS,
        ];
        for ((name, value), supported_maximum) in values.into_iter().zip(supported_maxima) {
            if value == 0 || value > supported_maximum {
                return Err(SmrError::new(
                    "profile-bound",
                    format!("{name} must be positive and no greater than {supported_maximum}"),
                ));
            }
        }
        if self.maximum_concurrency > self.maximum_clients {
            return Err(SmrError::new(
                "profile-bound",
                "maximum_concurrency exceeds maximum_clients",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivenessProfile {
    pub profile_id: String,
    pub required_progress: u64,
    pub virtual_progress_horizon: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmrWorkloadProfile {
    pub schema: String,
    pub profile_id: String,
    pub initial_state_ref: String,
    pub observation_mode: ObservationMode,
    pub bounds: WorkloadBounds,
    pub liveness: LivenessProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmrWorkloadPlan {
    pub profile_ref: String,
    pub genesis_digest: String,
    pub profile: SmrWorkloadProfile,
}

pub fn parse_and_admit_profile_json(
    input: &[u8],
    maximum_profile_bytes: usize,
) -> SmrResult<SmrWorkloadPlan> {
    if input.len() > maximum_profile_bytes {
        return Err(SmrError::new(
            "profile-input-bound",
            "profile JSON exceeds the external input bound",
        ));
    }
    let profile: SmrWorkloadProfile = serde_json::from_slice(input).map_err(|error| {
        SmrError::new(
            "profile-json",
            format!("profile JSON does not match the fail-closed projection: {error}"),
        )
    })?;
    admit_profile(&profile)
}

pub fn admit_profile(profile: &SmrWorkloadProfile) -> SmrResult<SmrWorkloadPlan> {
    if profile.schema != PROFILE_SCHEMA {
        return Err(SmrError::new(
            "profile-schema",
            format!("unsupported profile schema {}", profile.schema),
        ));
    }
    validate_identifier("profile_id", &profile.profile_id)?;
    validate_digest_ref("initial_state_ref", &profile.initial_state_ref)?;
    validate_identifier("liveness.profile_id", &profile.liveness.profile_id)?;
    if profile.liveness.required_progress == 0 {
        return Err(SmrError::new(
            "profile-liveness",
            "required_progress must be positive",
        ));
    }
    if profile.liveness.virtual_progress_horizon == 0
        || profile.liveness.virtual_progress_horizon > profile.bounds.maximum_virtual_progress
    {
        return Err(SmrError::new(
            "profile-liveness",
            "liveness horizon is zero or exceeds the workload bound",
        ));
    }
    profile.bounds.validate()?;
    let canonical = serde_json::to_vec(profile).map_err(|error| {
        SmrError::new(
            "profile-serialization",
            format!("cannot encode profile: {error}"),
        )
    })?;
    let profile_ref = domain_hash(PROFILE_DOMAIN, &[&canonical]);
    let genesis_digest = chain_genesis(&profile_ref, &profile.initial_state_ref)?;
    Ok(SmrWorkloadPlan {
        profile_ref,
        genesis_digest,
        profile: profile.clone(),
    })
}

pub fn chain_genesis(profile_ref: &str, initial_state_ref: &str) -> SmrResult<String> {
    validate_digest_ref("profile_ref", profile_ref)?;
    validate_digest_ref("initial_state_ref", initial_state_ref)?;
    Ok(domain_hash(
        GENESIS_DOMAIN,
        &[profile_ref.as_bytes(), initial_state_ref.as_bytes()],
    ))
}

pub fn chain_transition(
    profile_ref: &str,
    command_index: u64,
    prior_digest: &str,
    command: &[u8],
) -> SmrResult<String> {
    validate_digest_ref("profile_ref", profile_ref)?;
    validate_digest_ref("prior_digest", prior_digest)?;
    if command_index < MIN_COMMAND_INDEX {
        return Err(SmrError::new(
            "transition-index",
            "command index must start at one",
        ));
    }
    let command_length = u64::try_from(command.len()).map_err(|_| {
        SmrError::new(
            "transition-length",
            "command length does not fit canonical u64 framing",
        )
    })?;
    Ok(domain_hash(
        TRANSITION_DOMAIN,
        &[
            profile_ref.as_bytes(),
            &command_index.to_be_bytes(),
            prior_digest.as_bytes(),
            &command_length.to_be_bytes(),
            command,
        ],
    ))
}

fn domain_hash(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    for part in parts {
        let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
        hasher.update(&length.to_be_bytes());
        hasher.update(part);
    }
    format!("{DIGEST_PREFIX}{}", hasher.finalize().to_hex())
}

fn validate_digest_ref(name: &str, value: &str) -> SmrResult<()> {
    if value.len() > MAX_REFERENCE_LENGTH {
        return Err(SmrError::new(
            "reference-length",
            format!("{name} exceeds the maximum reference length"),
        ));
    }
    let Some(hex) = value.strip_prefix(DIGEST_PREFIX) else {
        return Err(SmrError::new(
            "reference-digest",
            format!("{name} must use a BLAKE3 reference"),
        ));
    };
    if hex.len() != DIGEST_HEX_LENGTH
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(SmrError::new(
            "reference-digest",
            format!("{name} has malformed lowercase BLAKE3 hex"),
        ));
    }
    Ok(())
}

fn validate_identifier(name: &str, value: &str) -> SmrResult<()> {
    if value.is_empty() || value.len() > MAX_REFERENCE_LENGTH {
        return Err(SmrError::new(
            "identifier",
            format!("{name} is empty or too long"),
        ));
    }
    if !value
        .as_bytes()
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(SmrError::new(
            "identifier",
            format!("{name} contains a non-canonical byte"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalOutcome {
    Acknowledged,
    DefinitelyRejected,
    Indefinite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalAttempt {
    pub event_sequence: u64,
    pub operation_id: String,
    pub attempt: u32,
    pub command_ref: String,
    pub outcome: ProposalOutcome,
}

pub fn validate_proposal_attempts(
    attempts: &[ProposalAttempt],
    bounds: &WorkloadBounds,
) -> SmrResult<()> {
    if u64::try_from(attempts.len()).unwrap_or(u64::MAX) > bounds.maximum_trace_events {
        return Err(SmrError::new(
            "proposal-bound",
            "proposal trace exceeds maximum_trace_events",
        ));
    }
    let mut last_sequence = None;
    let mut operations: BTreeMap<&str, (&str, u32, ProposalOutcome)> = BTreeMap::new();
    for attempt in attempts {
        validate_identifier("operation_id", &attempt.operation_id)?;
        validate_digest_ref("command_ref", &attempt.command_ref)?;
        if last_sequence.is_some_and(|prior| attempt.event_sequence <= prior) {
            return Err(SmrError::new(
                "proposal-order",
                "proposal event sequence is not strictly increasing",
            ));
        }
        last_sequence = Some(attempt.event_sequence);
        match operations.get(attempt.operation_id.as_str()) {
            None => {
                if attempt.attempt != 0 {
                    return Err(SmrError::new(
                        "proposal-attempt",
                        "the first proposal attempt must be zero",
                    ));
                }
                operations.insert(
                    &attempt.operation_id,
                    (&attempt.command_ref, attempt.attempt, attempt.outcome),
                );
            }
            Some((command_ref, prior_attempt, prior_outcome)) => {
                if *prior_outcome != ProposalOutcome::Indefinite {
                    return Err(SmrError::new(
                        "proposal-terminal",
                        "a terminal proposal outcome cannot be retried",
                    ));
                }
                if *command_ref != attempt.command_ref {
                    return Err(SmrError::new(
                        "proposal-identity",
                        "a retry changed its command identity",
                    ));
                }
                if attempt.attempt != prior_attempt.saturating_add(1) {
                    return Err(SmrError::new(
                        "proposal-attempt",
                        "proposal attempts are not contiguous",
                    ));
                }
                operations.insert(
                    &attempt.operation_id,
                    (&attempt.command_ref, attempt.attempt, attempt.outcome),
                );
            }
        }
    }
    if u64::try_from(operations.len()).unwrap_or(u64::MAX) > bounds.maximum_commands {
        return Err(SmrError::new(
            "proposal-bound",
            "logical operations exceed maximum_commands",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainObservation {
    pub event_sequence: u64,
    pub profile_ref: String,
    pub replica_id: String,
    pub command_index: u64,
    pub prior_digest: String,
    pub digest: String,
    pub application_state_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyViolationClass {
    ChangedObservation,
    ChainLink,
    Divergence,
    LosslessGap,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyViolation {
    pub class: SafetyViolationClass,
    pub replica_id: Option<String>,
    pub command_index: u64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyVerdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryReport {
    pub verdict: SafetyVerdict,
    pub observations_admitted: u64,
    pub exact_duplicates: u64,
    pub sampled_gaps: u64,
    pub lagging_replicas: Vec<String>,
    pub violations: Vec<SafetyViolation>,
    pub safety_prefixes: Vec<SafetyVerdict>,
}

pub fn validate_history(
    plan: &SmrWorkloadPlan,
    observations: &[ChainObservation],
) -> SmrResult<HistoryReport> {
    let event_count = u64::try_from(observations.len()).unwrap_or(u64::MAX);
    if event_count > plan.profile.bounds.maximum_trace_events {
        return Err(SmrError::new(
            "history-bound",
            "observation trace exceeds maximum_trace_events",
        ));
    }
    let mut last_sequence: BTreeMap<String, u64> = BTreeMap::new();
    let mut per_replica: BTreeMap<String, BTreeMap<u64, ChainObservation>> = BTreeMap::new();
    let mut maximum_index: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_index: BTreeMap<u64, (String, String)> = BTreeMap::new();
    let mut violations = Vec::new();
    let mut safety_prefixes = Vec::new();
    let mut exact_duplicates = 0_u64;
    let mut sampled_gaps = 0_u64;

    for observation in observations {
        validate_observation_shape(plan, observation)?;
        if last_sequence
            .get(&observation.replica_id)
            .is_some_and(|prior| observation.event_sequence <= *prior)
        {
            return Err(SmrError::new(
                "history-order",
                "replica observation sequence is not strictly increasing",
            ));
        }
        last_sequence.insert(observation.replica_id.clone(), observation.event_sequence);
        let replica = per_replica
            .entry(observation.replica_id.clone())
            .or_default();
        if let Some(existing) = replica.get(&observation.command_index) {
            if observations_have_equal_facts(existing, observation) {
                exact_duplicates = exact_duplicates.saturating_add(1);
                safety_prefixes.push(if violations.is_empty() {
                    SafetyVerdict::Pass
                } else {
                    SafetyVerdict::Fail
                });
                continue;
            }
            violations.push(SafetyViolation {
                class: SafetyViolationClass::ChangedObservation,
                replica_id: Some(observation.replica_id.clone()),
                command_index: observation.command_index,
                detail: "replica changed a previously observed command index".to_string(),
            });
        }
        if let Some(maximum) = maximum_index.get(&observation.replica_id).copied() {
            if observation.command_index < maximum {
                violations.push(SafetyViolation {
                    class: SafetyViolationClass::Rollback,
                    replica_id: Some(observation.replica_id.clone()),
                    command_index: observation.command_index,
                    detail: format!("replica rolled back from command index {maximum}"),
                });
            } else if observation.command_index > maximum.saturating_add(1) {
                match plan.profile.observation_mode {
                    ObservationMode::Lossless => violations.push(SafetyViolation {
                        class: SafetyViolationClass::LosslessGap,
                        replica_id: Some(observation.replica_id.clone()),
                        command_index: observation.command_index,
                        detail: format!(
                            "lossless observer omitted indices after command index {maximum}"
                        ),
                    }),
                    ObservationMode::Sampled => {
                        sampled_gaps = sampled_gaps.saturating_add(1);
                    }
                }
            }
        } else if observation.command_index > MIN_COMMAND_INDEX {
            match plan.profile.observation_mode {
                ObservationMode::Lossless => violations.push(SafetyViolation {
                    class: SafetyViolationClass::LosslessGap,
                    replica_id: Some(observation.replica_id.clone()),
                    command_index: observation.command_index,
                    detail: "lossless observer did not start at command index one".to_string(),
                }),
                ObservationMode::Sampled => sampled_gaps = sampled_gaps.saturating_add(1),
            }
        }

        if observation.command_index == MIN_COMMAND_INDEX {
            if observation.prior_digest != plan.genesis_digest {
                violations.push(SafetyViolation {
                    class: SafetyViolationClass::ChainLink,
                    replica_id: Some(observation.replica_id.clone()),
                    command_index: observation.command_index,
                    detail: "first observation does not link to admitted genesis".to_string(),
                });
            }
        } else if let Some(previous) = replica.get(&observation.command_index.saturating_sub(1)) {
            if observation.prior_digest != previous.digest {
                violations.push(SafetyViolation {
                    class: SafetyViolationClass::ChainLink,
                    replica_id: Some(observation.replica_id.clone()),
                    command_index: observation.command_index,
                    detail: "observation does not link to the preceding replica digest".to_string(),
                });
            }
        }

        if let Some((digest, state_ref)) = by_index.get(&observation.command_index) {
            if digest != &observation.digest || state_ref != &observation.application_state_ref {
                violations.push(SafetyViolation {
                    class: SafetyViolationClass::Divergence,
                    replica_id: Some(observation.replica_id.clone()),
                    command_index: observation.command_index,
                    detail: "replicas disagree on digest or canonical application state"
                        .to_string(),
                });
            }
        } else {
            by_index.insert(
                observation.command_index,
                (
                    observation.digest.clone(),
                    observation.application_state_ref.clone(),
                ),
            );
        }
        maximum_index
            .entry(observation.replica_id.clone())
            .and_modify(|value| *value = (*value).max(observation.command_index))
            .or_insert(observation.command_index);
        replica.insert(observation.command_index, observation.clone());
        safety_prefixes.push(if violations.is_empty() {
            SafetyVerdict::Pass
        } else {
            SafetyVerdict::Fail
        });
    }

    let global_maximum = maximum_index.values().copied().max().unwrap_or_default();
    let lagging_replicas = maximum_index
        .iter()
        .filter(|(_, value)| **value < global_maximum)
        .map(|(replica, _)| replica.clone())
        .collect();
    let observations_admitted = per_replica
        .values()
        .map(|replica| u64::try_from(replica.len()).unwrap_or(u64::MAX))
        .sum();
    Ok(HistoryReport {
        verdict: if violations.is_empty() {
            SafetyVerdict::Pass
        } else {
            SafetyVerdict::Fail
        },
        observations_admitted,
        exact_duplicates,
        sampled_gaps,
        lagging_replicas,
        violations,
        safety_prefixes,
    })
}

fn observations_have_equal_facts(left: &ChainObservation, right: &ChainObservation) -> bool {
    left.profile_ref == right.profile_ref
        && left.replica_id == right.replica_id
        && left.command_index == right.command_index
        && left.prior_digest == right.prior_digest
        && left.digest == right.digest
        && left.application_state_ref == right.application_state_ref
}

pub fn validate_observation_transition(
    plan: &SmrWorkloadPlan,
    observation: &ChainObservation,
    command: &[u8],
) -> SmrResult<()> {
    validate_observation_shape(plan, observation)?;
    if u64::try_from(command.len()).unwrap_or(u64::MAX) > plan.profile.bounds.maximum_command_bytes
    {
        return Err(SmrError::new(
            "observer-command-bound",
            "committed command exceeds maximum_command_bytes",
        ));
    }
    let expected = chain_transition(
        &plan.profile_ref,
        observation.command_index,
        &observation.prior_digest,
        command,
    )?;
    if observation.digest != expected {
        return Err(SmrError::new(
            "observer-conformance",
            "observation digest was not produced by the supplied committed command",
        ));
    }
    Ok(())
}

fn validate_observation_shape(
    plan: &SmrWorkloadPlan,
    observation: &ChainObservation,
) -> SmrResult<()> {
    if observation.profile_ref != plan.profile_ref {
        return Err(SmrError::new(
            "history-profile",
            "observation profile does not match the admitted plan",
        ));
    }
    validate_identifier("replica_id", &observation.replica_id)?;
    if observation.command_index < MIN_COMMAND_INDEX
        || observation.command_index > plan.profile.bounds.maximum_commands
    {
        return Err(SmrError::new(
            "history-index",
            "observation command index is outside the profile bound",
        ));
    }
    validate_digest_ref("prior_digest", &observation.prior_digest)?;
    validate_digest_ref("digest", &observation.digest)?;
    validate_digest_ref("application_state_ref", &observation.application_state_ref)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StabilizationFacts {
    pub profile_id: String,
    pub quorum_available: bool,
    pub lifecycle_ready: bool,
    pub disruptive_faults_active: bool,
    pub virtual_progress_start: u64,
    pub virtual_progress_end: u64,
    pub committed_before: u64,
    pub committed_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LivenessVerdict {
    Pass,
    Fail,
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivenessReport {
    pub verdict: LivenessVerdict,
    pub progress: u64,
    pub blockers: Vec<String>,
}

pub fn evaluate_liveness(
    plan: &SmrWorkloadPlan,
    facts: Option<&StabilizationFacts>,
) -> SmrResult<LivenessReport> {
    let Some(facts) = facts else {
        return Ok(LivenessReport {
            verdict: LivenessVerdict::NotEvaluated,
            progress: 0,
            blockers: vec!["stabilization facts are absent".to_string()],
        });
    };
    if facts.profile_id != plan.profile.liveness.profile_id {
        return Err(SmrError::new(
            "liveness-profile",
            "stabilization facts use a different liveness profile",
        ));
    }
    if facts.virtual_progress_end < facts.virtual_progress_start
        || facts.committed_after < facts.committed_before
    {
        return Err(SmrError::new(
            "liveness-order",
            "virtual progress or committed count rolled back",
        ));
    }
    let mut blockers = Vec::new();
    if !facts.quorum_available {
        blockers.push("quorum is unavailable".to_string());
    }
    if !facts.lifecycle_ready {
        blockers.push("consumer lifecycle is not ready".to_string());
    }
    if facts.disruptive_faults_active {
        blockers.push("disruptive faults remain active".to_string());
    }
    let elapsed = facts
        .virtual_progress_end
        .saturating_sub(facts.virtual_progress_start);
    if elapsed > plan.profile.liveness.virtual_progress_horizon {
        return Err(SmrError::new(
            "liveness-bound",
            "stabilization observation exceeds its virtual progress horizon",
        ));
    }
    let progress = facts.committed_after.saturating_sub(facts.committed_before);
    if !blockers.is_empty() {
        return Ok(LivenessReport {
            verdict: LivenessVerdict::NotEvaluated,
            progress,
            blockers,
        });
    }
    if progress >= plan.profile.liveness.required_progress {
        return Ok(LivenessReport {
            verdict: LivenessVerdict::Pass,
            progress,
            blockers,
        });
    }
    if elapsed < plan.profile.liveness.virtual_progress_horizon {
        blockers.push("virtual progress horizon is not exhausted".to_string());
        return Ok(LivenessReport {
            verdict: LivenessVerdict::NotEvaluated,
            progress,
            blockers,
        });
    }
    Ok(LivenessReport {
        verdict: LivenessVerdict::Fail,
        progress,
        blockers,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleState {
    Starting,
    Ready,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalStatus {
    Completed,
    Failed,
    BoundReached,
    Cancelled,
}

/// A semantic consumer contract. Implementations expose application facts only.
pub trait SmrConsumerAdapter {
    fn lifecycle(&self) -> LifecycleState;
    fn propose(&mut self, operation_id: &str, command: &[u8]) -> ProposalOutcome;
    fn observations(&self) -> &[ChainObservation];
    fn observation_mode(&self) -> ObservationMode;
    fn dropped_observations(&self) -> u64;
    fn terminal_status(&self) -> Option<TerminalStatus>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FaultClass {
    Network,
    Process,
    Storage,
    Scheduler,
    Clock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedFaultClass {
    pub class: FaultClass,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultCampaignProfile {
    pub campaign_id: String,
    pub includes_no_fault_control: bool,
    pub classes: Vec<WeightedFaultClass>,
    pub optional_features: Vec<String>,
    pub command_count: u64,
    pub client_count: u32,
    pub client_concurrency: u32,
    pub maximum_concurrent_faults: u32,
    pub maximum_actions: u64,
    pub virtual_duration: u64,
    pub terminal_rule: String,
}

pub fn validate_fault_campaign(
    campaign: &FaultCampaignProfile,
    bounds: &WorkloadBounds,
) -> SmrResult<()> {
    validate_identifier("campaign_id", &campaign.campaign_id)?;
    validate_identifier("terminal_rule", &campaign.terminal_rule)?;
    if !campaign.includes_no_fault_control {
        return Err(SmrError::new(
            "fault-control",
            "every evidence campaign requires a no-fault control",
        ));
    }
    if campaign.classes.is_empty()
        || campaign.command_count == 0
        || campaign.client_count == 0
        || campaign.client_concurrency == 0
        || campaign.maximum_concurrent_faults == 0
        || campaign.maximum_actions == 0
        || campaign.virtual_duration == 0
    {
        return Err(SmrError::new(
            "fault-bound",
            "fault classes and campaign bounds must be finite and non-empty",
        ));
    }
    if campaign.command_count > bounds.maximum_commands
        || campaign.client_count > bounds.maximum_clients
        || campaign.client_concurrency > bounds.maximum_concurrency
        || campaign.client_concurrency > campaign.client_count
        || u64::from(campaign.maximum_concurrent_faults) > campaign.maximum_actions
        || campaign.maximum_actions > bounds.maximum_fault_actions
        || campaign.virtual_duration > bounds.maximum_virtual_progress
    {
        return Err(SmrError::new(
            "fault-bound",
            "campaign exceeds workload fault or virtual progress bounds",
        ));
    }
    let mut classes = BTreeSet::new();
    for class in &campaign.classes {
        if class.weight == 0 || !classes.insert(class.class) {
            return Err(SmrError::new(
                "fault-class",
                "fault class weights must be positive and classes unique",
            ));
        }
    }
    if campaign.optional_features.len() > SUPPORTED_MAXIMUM_OPTIONAL_FEATURES {
        return Err(SmrError::new(
            "swarm-feature",
            "optional feature count exceeds the supported bound",
        ));
    }
    let mut features = BTreeSet::new();
    for feature in &campaign.optional_features {
        validate_identifier("optional_feature", feature)?;
        if !features.insert(feature) {
            return Err(SmrError::new(
                "swarm-feature",
                "optional feature names must be unique",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwarmSelection {
    pub seed: u64,
    pub selected_features: Vec<String>,
    pub selected_fault_classes: Vec<FaultClass>,
    pub unexplored_features: Vec<String>,
    pub unexplored_fault_classes: Vec<FaultClass>,
    pub weights: Vec<WeightedFaultClass>,
}

pub fn select_swarm(campaign: &FaultCampaignProfile, seed: u64) -> SwarmSelection {
    let mut selected_features = Vec::new();
    let mut unexplored_features = Vec::new();
    for feature in &campaign.optional_features {
        if seeded_choice(seed, feature.as_bytes()) {
            selected_features.push(feature.clone());
        } else {
            unexplored_features.push(feature.clone());
        }
    }
    let mut selected_fault_classes = Vec::new();
    let mut unexplored_fault_classes = Vec::new();
    let total_weight: u64 = campaign
        .classes
        .iter()
        .map(|weighted| u64::from(weighted.weight))
        .sum();
    for weighted in &campaign.classes {
        let label = format!("{:?}", weighted.class);
        if seeded_weighted_choice(seed, label.as_bytes(), weighted.weight, total_weight) {
            selected_fault_classes.push(weighted.class);
        } else {
            unexplored_fault_classes.push(weighted.class);
        }
    }
    SwarmSelection {
        seed,
        selected_features,
        selected_fault_classes,
        unexplored_features,
        unexplored_fault_classes,
        weights: campaign.classes.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCommand {
    pub operation_id: String,
    pub client_id: String,
    pub command_index: u64,
    pub bytes: Vec<u8>,
    pub command_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignPlan {
    pub campaign_id: String,
    pub no_fault_control_id: String,
    pub selection: SwarmSelection,
    pub commands: Vec<GeneratedCommand>,
    pub maximum_client_concurrency: u32,
    pub virtual_duration: u64,
    pub terminal_rule: String,
}

pub fn plan_campaign(
    workload: &SmrWorkloadPlan,
    campaign: &FaultCampaignProfile,
    seed: u64,
) -> SmrResult<CampaignPlan> {
    validate_fault_campaign(campaign, &workload.profile.bounds)?;
    let command_count = usize::try_from(campaign.command_count).map_err(|_| {
        SmrError::new(
            "campaign-command-bound",
            "command count does not fit the current platform",
        )
    })?;
    let client_count = u64::from(campaign.client_count);
    let mut commands = Vec::with_capacity(command_count);
    for offset in 0..command_count {
        let command_index = u64::try_from(offset)
            .unwrap_or(u64::MAX)
            .saturating_add(MIN_COMMAND_INDEX);
        let client_index = command_index.saturating_sub(MIN_COMMAND_INDEX) % client_count;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"chaoscontrol.smr-chain.generated-command.v1\0");
        hasher.update(&seed.to_be_bytes());
        hasher.update(&command_index.to_be_bytes());
        let bytes = hasher.finalize().as_bytes().to_vec();
        let command_ref = domain_hash(b"chaoscontrol.smr-chain.command-ref.v1\0", &[&bytes]);
        commands.push(GeneratedCommand {
            operation_id: format!("operation-{command_index}"),
            client_id: format!("client-{client_index}"),
            command_index,
            bytes,
            command_ref,
        });
    }
    Ok(CampaignPlan {
        campaign_id: campaign.campaign_id.clone(),
        no_fault_control_id: NO_FAULT_CONTROL_ID.to_string(),
        selection: select_swarm(campaign, seed),
        commands,
        maximum_client_concurrency: campaign.client_concurrency,
        virtual_duration: campaign.virtual_duration,
        terminal_rule: campaign.terminal_rule.clone(),
    })
}

fn seeded_choice(seed: u64, label: &[u8]) -> bool {
    let digest = seeded_choice_digest(seed, label);
    digest[0] & 1 == 1
}

fn seeded_weighted_choice(seed: u64, label: &[u8], weight: u32, total_weight: u64) -> bool {
    if total_weight == 0 {
        return false;
    }
    let digest = seeded_choice_digest(seed, label);
    let mut prefix = [0_u8; std::mem::size_of::<u64>()];
    prefix.copy_from_slice(&digest[..std::mem::size_of::<u64>()]);
    u64::from_be_bytes(prefix) % total_weight < u64::from(weight)
}

fn seeded_choice_digest(seed: u64, label: &[u8]) -> [u8; DIGEST_BYTE_LENGTH] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chaoscontrol.smr-chain.swarm-choice.v1\0");
    hasher.update(&seed.to_be_bytes());
    hasher.update(label);
    *hasher.finalize().as_bytes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FaultStage {
    Selected,
    Applicable,
    Applied,
    Observed,
    Rejected,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultOutcome {
    pub action_id: String,
    pub class: FaultClass,
    pub stages: Vec<FaultStage>,
    pub effect_record_ref: Option<String>,
}

pub fn validate_fault_outcomes(
    outcomes: &[FaultOutcome],
    bounds: &WorkloadBounds,
) -> SmrResult<()> {
    if u64::try_from(outcomes.len()).unwrap_or(u64::MAX) > bounds.maximum_fault_actions {
        return Err(SmrError::new(
            "fault-bound",
            "fault outcomes exceed maximum_fault_actions",
        ));
    }
    let mut identities = BTreeSet::new();
    for outcome in outcomes {
        validate_identifier("fault.action_id", &outcome.action_id)?;
        if !identities.insert(&outcome.action_id) {
            return Err(SmrError::new(
                "fault-identity",
                "fault action identities must be unique",
            ));
        }
        if outcome.stages.len() > SUPPORTED_FAULT_STAGE_COUNT {
            return Err(SmrError::new(
                "fault-stage",
                "fault outcome stage count exceeds the supported bound",
            ));
        }
        let stages: BTreeSet<_> = outcome.stages.iter().copied().collect();
        if stages.len() != outcome.stages.len()
            || outcome.stages.windows(2).any(|pair| pair[0] >= pair[1])
            || (stages.contains(&FaultStage::Rejected) && stages.contains(&FaultStage::Unsupported))
        {
            return Err(SmrError::new(
                "fault-stage",
                "fault outcome stages must be unique, ordered, and have one terminal path",
            ));
        }
        if (stages.contains(&FaultStage::Applied) && !stages.contains(&FaultStage::Applicable))
            || (stages.contains(&FaultStage::Observed) && !stages.contains(&FaultStage::Applied))
            || ((stages.contains(&FaultStage::Rejected)
                || stages.contains(&FaultStage::Unsupported))
                && (stages.contains(&FaultStage::Applied)
                    || stages.contains(&FaultStage::Observed)))
        {
            return Err(SmrError::new(
                "fault-stage",
                "fault outcome stages do not form an admitted effect path",
            ));
        }
        let claims_effect =
            stages.contains(&FaultStage::Applied) || stages.contains(&FaultStage::Observed);
        let supports_effect = stages.contains(&FaultStage::Applicable)
            && stages.contains(&FaultStage::Applied)
            && stages.contains(&FaultStage::Observed)
            && outcome.effect_record_ref.is_some();
        if supports_effect {
            validate_digest_ref(
                "fault.effect_record_ref",
                outcome.effect_record_ref.as_deref().unwrap_or_default(),
            )?;
        } else if claims_effect || outcome.effect_record_ref.is_some() {
            return Err(SmrError::new(
                "fault-effect",
                "effect evidence requires applicable, applied, observed, and an admitted record",
            ));
        }
    }
    Ok(())
}

pub fn fault_supports_effect_claim(outcome: &FaultOutcome) -> bool {
    outcome.stages.contains(&FaultStage::Applicable)
        && outcome.stages.contains(&FaultStage::Applied)
        && outcome.stages.contains(&FaultStage::Observed)
        && outcome
            .effect_record_ref
            .as_deref()
            .is_some_and(|reference| validate_digest_ref("effect_record_ref", reference).is_ok())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRun {
    pub operation_ids: Vec<String>,
    pub proposal_outcomes: Vec<ProposalAttempt>,
    pub observations: Vec<ChainObservation>,
    pub safety_prefixes: Vec<SafetyVerdict>,
    pub liveness: LivenessReport,
    pub terminal_status: TerminalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayComparison {
    pub accepted: bool,
    pub first_mismatch: Option<String>,
}

pub fn history_report_ref(report: &HistoryReport) -> SmrResult<String> {
    canonical_value_ref(HISTORY_REPORT_DOMAIN, report)
}

pub fn replay_comparison_ref(comparison: &ReplayComparison) -> SmrResult<String> {
    canonical_value_ref(REPLAY_COMPARISON_DOMAIN, comparison)
}

fn canonical_value_ref<T: Serialize>(domain: &[u8], value: &T) -> SmrResult<String> {
    let canonical = serde_json::to_vec(value).map_err(|error| {
        SmrError::new(
            "canonical-serialization",
            format!("cannot serialize canonical value: {error}"),
        )
    })?;
    Ok(domain_hash(domain, &[&canonical]))
}

pub fn compare_replay_bounded(
    expected: &SemanticRun,
    actual: &SemanticRun,
    bounds: &WorkloadBounds,
) -> SmrResult<ReplayComparison> {
    for (name, run) in [("expected", expected), ("actual", actual)] {
        let event_count = run
            .operation_ids
            .len()
            .saturating_add(run.proposal_outcomes.len())
            .saturating_add(run.observations.len())
            .saturating_add(run.safety_prefixes.len());
        if u64::try_from(event_count).unwrap_or(u64::MAX) > bounds.maximum_replay_events {
            return Err(SmrError::new(
                "replay-bound",
                format!("{name} semantic replay exceeds maximum_replay_events"),
            ));
        }
    }
    Ok(compare_replay(expected, actual))
}

pub fn compare_replay(expected: &SemanticRun, actual: &SemanticRun) -> ReplayComparison {
    let checks = [
        (
            "operation-identities",
            expected.operation_ids == actual.operation_ids,
        ),
        (
            "proposal-outcomes",
            expected.proposal_outcomes == actual.proposal_outcomes,
        ),
        ("observations", expected.observations == actual.observations),
        (
            "safety-prefixes",
            expected.safety_prefixes == actual.safety_prefixes,
        ),
        ("liveness", expected.liveness == actual.liveness),
        (
            "terminal-verdict",
            expected.terminal_status == actual.terminal_status,
        ),
    ];
    for (name, matches) in checks {
        if !matches {
            return ReplayComparison {
                accepted: false,
                first_mismatch: Some(name.to_string()),
            };
        }
    }
    ReplayComparison {
        accepted: true,
        first_mismatch: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReductionInput {
    pub commands: Vec<String>,
    pub clients: Vec<String>,
    pub fault_actions: Vec<String>,
    pub schedule_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReductionStatus {
    Reduced,
    Irreducible,
    BoundReached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReductionReport {
    pub status: ReductionStatus,
    pub attempts: u64,
    pub reduced: ReductionInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReductionDimension {
    Commands,
    Clients,
    FaultActions,
    ScheduleActions,
}

const REDUCTION_DIMENSION_COUNT: usize = 4;
const REDUCTION_DIMENSIONS: [ReductionDimension; REDUCTION_DIMENSION_COUNT] = [
    ReductionDimension::Commands,
    ReductionDimension::Clients,
    ReductionDimension::FaultActions,
    ReductionDimension::ScheduleActions,
];

pub fn reduce_failure<F>(
    input: &ReductionInput,
    maximum_attempts: u64,
    preserves_failure: F,
) -> ReductionReport
where
    F: Fn(&ReductionInput) -> bool,
{
    let mut reduced = input.clone();
    let mut attempts = 0_u64;
    let mut changed = false;
    loop {
        let mut pass_changed = false;
        for dimension in REDUCTION_DIMENSIONS {
            let field_length = reduction_field(&reduced, dimension).len();
            for index in (0..field_length).rev() {
                if attempts >= maximum_attempts {
                    return ReductionReport {
                        status: ReductionStatus::BoundReached,
                        attempts,
                        reduced,
                    };
                }
                attempts = attempts.saturating_add(1);
                let mut candidate = reduced.clone();
                reduction_field_mut(&mut candidate, dimension).remove(index);
                if preserves_failure(&candidate) {
                    reduced = candidate;
                    changed = true;
                    pass_changed = true;
                }
            }
        }
        if !pass_changed {
            return ReductionReport {
                status: if changed {
                    ReductionStatus::Reduced
                } else {
                    ReductionStatus::Irreducible
                },
                attempts,
                reduced,
            };
        }
    }
}

fn reduction_field(input: &ReductionInput, dimension: ReductionDimension) -> &[String] {
    match dimension {
        ReductionDimension::Commands => &input.commands,
        ReductionDimension::Clients => &input.clients,
        ReductionDimension::FaultActions => &input.fault_actions,
        ReductionDimension::ScheduleActions => &input.schedule_actions,
    }
}

fn reduction_field_mut(
    input: &mut ReductionInput,
    dimension: ReductionDimension,
) -> &mut Vec<String> {
    match dimension {
        ReductionDimension::Commands => &mut input.commands,
        ReductionDimension::Clients => &mut input.clients,
        ReductionDimension::FaultActions => &mut input.fault_actions,
        ReductionDimension::ScheduleActions => &mut input.schedule_actions,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmrEvidenceReceipt {
    pub schema: String,
    pub profile_ref: String,
    pub build_ref: String,
    pub adapter_ref: String,
    pub observer_ref: String,
    pub schedule_ref: String,
    pub history_ref: String,
    pub replay_ref: String,
    pub observation_mode: ObservationMode,
    pub dropped_observations: u64,
    pub seed: u64,
    pub swarm: SwarmSelection,
    pub fault_outcomes: Vec<FaultOutcome>,
    pub history: HistoryReport,
    pub stabilization_facts: Option<StabilizationFacts>,
    pub liveness: LivenessReport,
    pub bounds: WorkloadBounds,
    pub replay: ReplayComparison,
    pub terminal_status: TerminalStatus,
    pub non_claims: Vec<String>,
}

pub fn validate_evidence_receipt(
    receipt: &SmrEvidenceReceipt,
    plan: &SmrWorkloadPlan,
) -> SmrResult<()> {
    if receipt.schema != RECEIPT_SCHEMA {
        return Err(SmrError::new(
            "evidence-schema",
            "unsupported SMR evidence receipt schema",
        ));
    }
    if receipt.profile_ref != plan.profile_ref || receipt.bounds != plan.profile.bounds {
        return Err(SmrError::new(
            "evidence-profile",
            "receipt profile or bounds differ from the admitted plan",
        ));
    }
    for (name, reference) in [
        ("build_ref", &receipt.build_ref),
        ("adapter_ref", &receipt.adapter_ref),
        ("observer_ref", &receipt.observer_ref),
        ("schedule_ref", &receipt.schedule_ref),
        ("history_ref", &receipt.history_ref),
        ("replay_ref", &receipt.replay_ref),
    ] {
        validate_digest_ref(name, reference)?;
    }
    validate_fault_outcomes(&receipt.fault_outcomes, &receipt.bounds)?;
    if receipt.history_ref != history_report_ref(&receipt.history)? {
        return Err(SmrError::new(
            "evidence-history-drift",
            "history report identity does not match the retained report",
        ));
    }
    if receipt.replay_ref != replay_comparison_ref(&receipt.replay)? {
        return Err(SmrError::new(
            "evidence-replay-drift",
            "replay comparison identity does not match the retained result",
        ));
    }
    if receipt.observation_mode == ObservationMode::Lossless && receipt.dropped_observations != 0 {
        return Err(SmrError::new(
            "evidence-observer",
            "lossless observer reports dropped events",
        ));
    }
    if !receipt.replay.accepted {
        return Err(SmrError::new(
            "evidence-replay",
            "receipt replay comparison is not accepted",
        ));
    }
    match receipt.stabilization_facts.as_ref() {
        Some(facts) => {
            if evaluate_liveness(plan, Some(facts))? != receipt.liveness {
                return Err(SmrError::new(
                    "evidence-liveness",
                    "receipt liveness result does not match its stabilization facts",
                ));
            }
        }
        None if receipt.liveness.verdict != LivenessVerdict::NotEvaluated => {
            return Err(SmrError::new(
                "evidence-liveness",
                "evaluated liveness requires explicit stabilization facts",
            ));
        }
        None => {}
    }
    let encoded_length = serde_json::to_vec(receipt)
        .map_err(|error| {
            SmrError::new(
                "evidence-serialization",
                format!("cannot encode evidence receipt: {error}"),
            )
        })?
        .len();
    if u64::try_from(encoded_length).unwrap_or(u64::MAX) > receipt.bounds.maximum_evidence_bytes {
        return Err(SmrError::new(
            "evidence-bound",
            "serialized receipt exceeds maximum_evidence_bytes",
        ));
    }
    for required in REQUIRED_NON_CLAIMS {
        if !receipt.non_claims.iter().any(|claim| claim == required) {
            return Err(SmrError::new(
                "evidence-overclaim",
                format!("receipt omits required non-claim: {required}"),
            ));
        }
    }
    Ok(())
}

/// Run one positive and one negative pure fixture without infrastructure.
pub fn smr_chain_selftest() -> SmrResult<()> {
    const SELFTEST_REF: &str =
        "blake3:0000000000000000000000000000000000000000000000000000000000000000";
    const SELFTEST_COMMANDS: u64 = 4;
    const SELFTEST_COMMAND_BYTES: u64 = 1_024;
    const SELFTEST_CLIENTS: u32 = 2;
    const SELFTEST_CONCURRENCY: u32 = 2;
    const SELFTEST_PROGRESS: u64 = 32;
    const SELFTEST_EVENTS: u64 = 32;
    const SELFTEST_FAULTS: u64 = 8;
    const SELFTEST_EVIDENCE_BYTES: u64 = 65_536;
    const SELFTEST_REDUCTION_ATTEMPTS: u64 = 16;
    const SELFTEST_HORIZON: u64 = 8;
    let plan = admit_profile(&SmrWorkloadProfile {
        schema: PROFILE_SCHEMA.to_string(),
        profile_id: "smr-chain-selftest".to_string(),
        initial_state_ref: SELFTEST_REF.to_string(),
        observation_mode: ObservationMode::Lossless,
        bounds: WorkloadBounds {
            maximum_commands: SELFTEST_COMMANDS,
            maximum_command_bytes: SELFTEST_COMMAND_BYTES,
            maximum_clients: SELFTEST_CLIENTS,
            maximum_concurrency: SELFTEST_CONCURRENCY,
            maximum_virtual_progress: SELFTEST_PROGRESS,
            maximum_trace_events: SELFTEST_EVENTS,
            maximum_fault_actions: SELFTEST_FAULTS,
            maximum_replay_events: SELFTEST_EVENTS,
            maximum_evidence_bytes: SELFTEST_EVIDENCE_BYTES,
            maximum_reduction_attempts: SELFTEST_REDUCTION_ATTEMPTS,
        },
        liveness: LivenessProfile {
            profile_id: "selftest-recovery".to_string(),
            required_progress: 1,
            virtual_progress_horizon: SELFTEST_HORIZON,
        },
    })?;
    let digest = chain_transition(
        &plan.profile_ref,
        MIN_COMMAND_INDEX,
        &plan.genesis_digest,
        b"selftest-command",
    )?;
    let base = ChainObservation {
        event_sequence: 1,
        profile_ref: plan.profile_ref.clone(),
        replica_id: "replica-a".to_string(),
        command_index: MIN_COMMAND_INDEX,
        prior_digest: plan.genesis_digest.clone(),
        digest: digest.clone(),
        application_state_ref: digest.clone(),
    };
    let mut peer = base.clone();
    peer.event_sequence = 2;
    peer.replica_id = "replica-b".to_string();
    let valid = validate_history(&plan, &[base.clone(), peer.clone()])?;
    if valid.verdict != SafetyVerdict::Pass {
        return Err(SmrError::new(
            "selftest-positive",
            "equal replica transitions did not pass",
        ));
    }
    peer.application_state_ref = plan.genesis_digest.clone();
    let invalid = validate_history(&plan, &[base, peer])?;
    if invalid.verdict != SafetyVerdict::Fail
        || !invalid
            .violations
            .iter()
            .any(|violation| violation.class == SafetyViolationClass::Divergence)
    {
        return Err(SmrError::new(
            "selftest-negative",
            "divergent application state did not fail",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE_ID: &str = "smr-chain-test";
    const LIVENESS_ID: &str = "recovered-quorum";
    const REPLICA_A: &str = "replica-a";
    const REPLICA_B: &str = "replica-b";
    const OPERATION_ID: &str = "operation-a";
    const COMMAND_COUNT: u64 = 8;
    const MAXIMUM_COMMAND_BYTES: u64 = 1_024;
    const CLIENT_COUNT: u32 = 2;
    const CONCURRENCY: u32 = 2;
    const VIRTUAL_PROGRESS: u64 = 100;
    const TRACE_EVENTS: u64 = 128;
    const FAULT_ACTIONS: u64 = 16;
    const REPLAY_EVENTS: u64 = 128;
    const EVIDENCE_BYTES: u64 = 65_536;
    const REDUCTION_ATTEMPTS: u64 = 64;
    const MAXIMUM_PROFILE_BYTES: usize = 4_096;
    const PROPERTY_CASES: u64 = 32;
    const REQUIRED_PROGRESS: u64 = 1;
    const LIVENESS_HORIZON: u64 = 20;
    const INCOMPLETE_LIVENESS_HORIZON: u64 = LIVENESS_HORIZON - 1;
    const FIRST_SEQUENCE: u64 = 1;
    const SECOND_SEQUENCE: u64 = 2;
    const THIRD_SEQUENCE: u64 = 3;
    const FIRST_INDEX: u64 = 1;
    const SECOND_INDEX: u64 = 2;
    const FIRST_ATTEMPT: u32 = 0;
    const SECOND_ATTEMPT: u32 = 1;
    const SEED: u64 = 42;

    fn digest(label: &[u8]) -> String {
        domain_hash(b"chaoscontrol.smr-chain.test.v1\0", &[label])
    }

    fn profile(mode: ObservationMode) -> SmrWorkloadProfile {
        SmrWorkloadProfile {
            schema: PROFILE_SCHEMA.to_string(),
            profile_id: PROFILE_ID.to_string(),
            initial_state_ref: digest(b"initial"),
            observation_mode: mode,
            bounds: WorkloadBounds {
                maximum_commands: COMMAND_COUNT,
                maximum_command_bytes: MAXIMUM_COMMAND_BYTES,
                maximum_clients: CLIENT_COUNT,
                maximum_concurrency: CONCURRENCY,
                maximum_virtual_progress: VIRTUAL_PROGRESS,
                maximum_trace_events: TRACE_EVENTS,
                maximum_fault_actions: FAULT_ACTIONS,
                maximum_replay_events: REPLAY_EVENTS,
                maximum_evidence_bytes: EVIDENCE_BYTES,
                maximum_reduction_attempts: REDUCTION_ATTEMPTS,
            },
            liveness: LivenessProfile {
                profile_id: LIVENESS_ID.to_string(),
                required_progress: REQUIRED_PROGRESS,
                virtual_progress_horizon: LIVENESS_HORIZON,
            },
        }
    }

    fn observations(plan: &SmrWorkloadPlan) -> Vec<ChainObservation> {
        let command_one = b"command-one";
        let command_two = b"command-two";
        let first = chain_transition(
            &plan.profile_ref,
            FIRST_INDEX,
            &plan.genesis_digest,
            command_one,
        )
        .expect("first transition");
        let second = chain_transition(&plan.profile_ref, SECOND_INDEX, &first, command_two)
            .expect("second transition");
        let state_one = digest(b"state-one");
        let state_two = digest(b"state-two");
        vec![
            ChainObservation {
                event_sequence: FIRST_SEQUENCE,
                profile_ref: plan.profile_ref.clone(),
                replica_id: REPLICA_A.to_string(),
                command_index: FIRST_INDEX,
                prior_digest: plan.genesis_digest.clone(),
                digest: first.clone(),
                application_state_ref: state_one.clone(),
            },
            ChainObservation {
                event_sequence: SECOND_SEQUENCE,
                profile_ref: plan.profile_ref.clone(),
                replica_id: REPLICA_B.to_string(),
                command_index: FIRST_INDEX,
                prior_digest: plan.genesis_digest.clone(),
                digest: first,
                application_state_ref: state_one,
            },
            ChainObservation {
                event_sequence: THIRD_SEQUENCE,
                profile_ref: plan.profile_ref.clone(),
                replica_id: REPLICA_A.to_string(),
                command_index: SECOND_INDEX,
                prior_digest: chain_transition(
                    &plan.profile_ref,
                    FIRST_INDEX,
                    &plan.genesis_digest,
                    command_one,
                )
                .expect("recomputed first transition"),
                digest: second,
                application_state_ref: state_two,
            },
        ]
    }

    #[test]
    fn profile_and_framing_are_deterministic_and_fail_closed() {
        let admitted = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let repeated = admit_profile(&profile(ObservationMode::Lossless)).expect("same profile");
        assert_eq!(admitted, repeated);
        let first = chain_transition(
            &admitted.profile_ref,
            FIRST_INDEX,
            &admitted.genesis_digest,
            b"ab",
        )
        .expect("framed transition");
        let different_boundary = chain_transition(
            &admitted.profile_ref,
            FIRST_INDEX,
            &admitted.genesis_digest,
            b"a\0b",
        )
        .expect("different framed transition");
        assert_ne!(first, different_boundary);

        let mut invalid = profile(ObservationMode::Lossless);
        invalid.bounds.maximum_trace_events = 0;
        assert_eq!(
            admit_profile(&invalid).expect_err("zero bound").class,
            "profile-bound"
        );
        assert_eq!(
            chain_transition(
                &admitted.profile_ref,
                0,
                &admitted.genesis_digest,
                b"command"
            )
            .expect_err("index zero")
            .class,
            "transition-index"
        );

        let encoded =
            serde_json::to_vec(&profile(ObservationMode::Lossless)).expect("profile JSON");
        assert_eq!(
            parse_and_admit_profile_json(&encoded, MAXIMUM_PROFILE_BYTES)
                .expect("external profile"),
            admitted
        );
        let mut unknown: serde_json::Value =
            serde_json::from_slice(&encoded).expect("profile value");
        unknown["ambient_timeout"] = serde_json::json!(1);
        let encoded_unknown = serde_json::to_vec(&unknown).expect("unknown JSON");
        assert_eq!(
            parse_and_admit_profile_json(&encoded_unknown, MAXIMUM_PROFILE_BYTES)
                .expect_err("unknown field")
                .class,
            "profile-json"
        );
    }

    #[test]
    fn transition_property_cases_preserve_position_and_content_identity() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let mut prior = plan.genesis_digest.clone();
        for command_index in MIN_COMMAND_INDEX..=PROPERTY_CASES {
            let command = command_index.to_be_bytes();
            let digest = chain_transition(&plan.profile_ref, command_index, &prior, &command)
                .expect("property transition");
            assert_eq!(
                digest,
                chain_transition(&plan.profile_ref, command_index, &prior, &command)
                    .expect("repeated property transition")
            );
            assert_ne!(
                digest,
                chain_transition(
                    &plan.profile_ref,
                    command_index.saturating_add(1),
                    &prior,
                    &command,
                )
                .expect("position-sensitive transition")
            );
            prior = digest;
        }
    }

    #[test]
    fn history_accepts_lag_and_rejects_divergence_rollback_and_gaps() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let valid = observations(&plan);
        let report = validate_history(&plan, &valid).expect("valid history");
        assert_eq!(report.verdict, SafetyVerdict::Pass);
        assert_eq!(report.lagging_replicas, vec![REPLICA_B.to_string()]);
        validate_observation_transition(&plan, &valid[0], b"command-one")
            .expect("observer conformance");
        assert_eq!(
            validate_observation_transition(&plan, &valid[0], b"fabricated-command")
                .expect_err("fabricated expected state")
                .class,
            "observer-conformance"
        );

        let mut repeated = valid.clone();
        let mut exact_duplicate = repeated[0].clone();
        exact_duplicate.event_sequence = repeated.last().expect("last").event_sequence + 1;
        repeated.push(exact_duplicate);
        let repeated_report = validate_history(&plan, &repeated).expect("exact duplicate");
        assert_eq!(repeated_report.exact_duplicates, 1);
        assert_eq!(repeated_report.verdict, SafetyVerdict::Pass);

        let mut divergent = valid.clone();
        divergent[1].digest = digest(b"divergent");
        let mut later_match = divergent[2].clone();
        later_match.event_sequence = divergent.last().expect("last").event_sequence + 1;
        later_match.replica_id = REPLICA_B.to_string();
        divergent.push(later_match);
        let report = validate_history(&plan, &divergent).expect("classified divergence");
        assert_eq!(report.verdict, SafetyVerdict::Fail);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.class == SafetyViolationClass::Divergence));

        let mut rollback = valid.clone();
        let mut old = rollback[0].clone();
        old.event_sequence = rollback.last().expect("last").event_sequence + 1;
        old.application_state_ref = digest(b"rollback-state");
        rollback.push(old);
        let report = validate_history(&plan, &rollback).expect("classified rollback");
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.class == SafetyViolationClass::Rollback));

        let mut gap = valid;
        gap[2].command_index += 1;
        let report = validate_history(&plan, &gap).expect("classified lossless gap");
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.class == SafetyViolationClass::LosslessGap));
    }

    #[test]
    fn sampled_gap_reduces_coverage_without_fabricating_divergence() {
        let plan = admit_profile(&profile(ObservationMode::Sampled)).expect("sampled profile");
        let mut sampled = observations(&plan);
        sampled.retain(|observation| observation.replica_id == REPLICA_A);
        sampled[1].command_index += 1;
        sampled[1].event_sequence = SECOND_SEQUENCE;
        let report = validate_history(&plan, &sampled).expect("sampled history");
        assert_eq!(report.sampled_gaps, 1);
        assert!(!report
            .violations
            .iter()
            .any(|violation| violation.class == SafetyViolationClass::Divergence));
    }

    #[test]
    fn proposal_identity_survives_indefinite_retry() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let command_ref = digest(b"command");
        let attempts = vec![
            ProposalAttempt {
                event_sequence: FIRST_SEQUENCE,
                operation_id: OPERATION_ID.to_string(),
                attempt: FIRST_ATTEMPT,
                command_ref: command_ref.clone(),
                outcome: ProposalOutcome::Indefinite,
            },
            ProposalAttempt {
                event_sequence: SECOND_SEQUENCE,
                operation_id: OPERATION_ID.to_string(),
                attempt: SECOND_ATTEMPT,
                command_ref,
                outcome: ProposalOutcome::Acknowledged,
            },
        ];
        validate_proposal_attempts(&attempts, &plan.profile.bounds).expect("stable retry");
        let mut terminal_retry = attempts.clone();
        terminal_retry[0].outcome = ProposalOutcome::Acknowledged;
        assert_eq!(
            validate_proposal_attempts(&terminal_retry, &plan.profile.bounds)
                .expect_err("terminal retry")
                .class,
            "proposal-terminal"
        );
        let mut changed = attempts;
        changed[1].command_ref = digest(b"changed-command");
        assert_eq!(
            validate_proposal_attempts(&changed, &plan.profile.bounds)
                .expect_err("changed retry")
                .class,
            "proposal-identity"
        );
    }

    #[test]
    fn liveness_requires_named_stabilization_and_virtual_progress() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let absent = evaluate_liveness(&plan, None).expect("absence is bounded");
        assert_eq!(absent.verdict, LivenessVerdict::NotEvaluated);
        let active_partition = StabilizationFacts {
            profile_id: LIVENESS_ID.to_string(),
            quorum_available: false,
            lifecycle_ready: true,
            disruptive_faults_active: true,
            virtual_progress_start: 0,
            virtual_progress_end: LIVENESS_HORIZON,
            committed_before: 0,
            committed_after: 0,
        };
        let report = evaluate_liveness(&plan, Some(&active_partition)).expect("not evaluated");
        assert_eq!(report.verdict, LivenessVerdict::NotEvaluated);
        let incomplete = StabilizationFacts {
            profile_id: LIVENESS_ID.to_string(),
            quorum_available: true,
            lifecycle_ready: true,
            disruptive_faults_active: false,
            virtual_progress_start: 0,
            virtual_progress_end: INCOMPLETE_LIVENESS_HORIZON,
            committed_before: 0,
            committed_after: 0,
        };
        assert_eq!(
            evaluate_liveness(&plan, Some(&incomplete))
                .expect("incomplete horizon")
                .verdict,
            LivenessVerdict::NotEvaluated
        );
        let recovered = StabilizationFacts {
            profile_id: LIVENESS_ID.to_string(),
            quorum_available: true,
            lifecycle_ready: true,
            disruptive_faults_active: false,
            virtual_progress_start: 0,
            virtual_progress_end: LIVENESS_HORIZON,
            committed_before: 0,
            committed_after: REQUIRED_PROGRESS,
        };
        assert_eq!(
            evaluate_liveness(&plan, Some(&recovered))
                .expect("recovered")
                .verdict,
            LivenessVerdict::Pass
        );
        let stalled = StabilizationFacts {
            committed_after: 0,
            ..recovered
        };
        assert_eq!(
            evaluate_liveness(&plan, Some(&stalled))
                .expect("stalled recovery")
                .verdict,
            LivenessVerdict::Fail
        );
    }

    #[test]
    fn campaigns_retain_choices_and_require_effect_records() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let campaign = FaultCampaignProfile {
            campaign_id: "campaign-a".to_string(),
            includes_no_fault_control: true,
            classes: vec![
                WeightedFaultClass {
                    class: FaultClass::Network,
                    weight: 3,
                },
                WeightedFaultClass {
                    class: FaultClass::Scheduler,
                    weight: 1,
                },
            ],
            optional_features: vec!["retry".to_string(), "snapshot".to_string()],
            command_count: COMMAND_COUNT,
            client_count: CLIENT_COUNT,
            client_concurrency: CONCURRENCY,
            maximum_concurrent_faults: 1,
            maximum_actions: FAULT_ACTIONS,
            virtual_duration: VIRTUAL_PROGRESS,
            terminal_rule: "bounded-completion".to_string(),
        };
        validate_fault_campaign(&campaign, &plan.profile.bounds).expect("campaign");
        let mut missing_control = campaign.clone();
        missing_control.includes_no_fault_control = false;
        assert_eq!(
            validate_fault_campaign(&missing_control, &plan.profile.bounds)
                .expect_err("missing control")
                .class,
            "fault-control"
        );
        assert_eq!(select_swarm(&campaign, SEED), select_swarm(&campaign, SEED));
        let campaign_plan = plan_campaign(&plan, &campaign, SEED).expect("campaign plan");
        assert_eq!(
            campaign_plan,
            plan_campaign(&plan, &campaign, SEED).expect("repeated campaign plan")
        );
        assert_eq!(
            u64::try_from(campaign_plan.commands.len()).expect("command count"),
            COMMAND_COUNT
        );
        assert_eq!(campaign_plan.no_fault_control_id, NO_FAULT_CONTROL_ID);
        let selected_only = FaultOutcome {
            action_id: "fault-a".to_string(),
            class: FaultClass::Network,
            stages: vec![FaultStage::Selected],
            effect_record_ref: None,
        };
        assert!(!fault_supports_effect_claim(&selected_only));
        let effect = FaultOutcome {
            action_id: "fault-b".to_string(),
            class: FaultClass::Network,
            stages: vec![
                FaultStage::Selected,
                FaultStage::Applicable,
                FaultStage::Applied,
                FaultStage::Observed,
            ],
            effect_record_ref: Some(digest(b"effect")),
        };
        assert!(fault_supports_effect_claim(&effect));
        let mut missing_effect_record = effect;
        missing_effect_record.effect_record_ref = None;
        assert_eq!(
            validate_fault_outcomes(&[missing_effect_record], &plan.profile.bounds)
                .expect_err("missing effect record")
                .class,
            "fault-effect"
        );
    }

    #[test]
    fn replay_stops_at_first_mismatch_and_reduction_is_bounded() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let liveness = LivenessReport {
            verdict: LivenessVerdict::Pass,
            progress: REQUIRED_PROGRESS,
            blockers: Vec::new(),
        };
        let expected = SemanticRun {
            operation_ids: vec![OPERATION_ID.to_string()],
            proposal_outcomes: Vec::new(),
            observations: Vec::new(),
            safety_prefixes: vec![SafetyVerdict::Pass],
            liveness,
            terminal_status: TerminalStatus::Completed,
        };
        assert!(
            compare_replay_bounded(&expected, &expected, &plan.profile.bounds)
                .expect("bounded replay")
                .accepted
        );
        let mut over_bound = expected.clone();
        let over_bound_count = usize::try_from(plan.profile.bounds.maximum_replay_events)
            .expect("replay event bound")
            + 1;
        over_bound.operation_ids = vec![OPERATION_ID.to_string(); over_bound_count];
        assert_eq!(
            compare_replay_bounded(&over_bound, &expected, &plan.profile.bounds)
                .expect_err("replay bound")
                .class,
            "replay-bound"
        );
        let mut changed = expected.clone();
        changed.safety_prefixes = vec![SafetyVerdict::Fail];
        assert_eq!(
            compare_replay(&expected, &changed).first_mismatch,
            Some("safety-prefixes".to_string())
        );
        let input = ReductionInput {
            commands: vec!["safe".to_string(), "failure".to_string()],
            clients: vec!["client-a".to_string()],
            fault_actions: vec!["fault-a".to_string()],
            schedule_actions: vec!["schedule-a".to_string()],
        };
        let report = reduce_failure(&input, REDUCTION_ATTEMPTS, |candidate| {
            candidate
                .commands
                .iter()
                .any(|command| command == "failure")
        });
        assert_eq!(report.status, ReductionStatus::Reduced);
        assert_eq!(report.reduced.commands, vec!["failure".to_string()]);
    }

    #[test]
    fn evidence_rejects_missing_non_claim_and_selected_only_promotion() {
        let plan = admit_profile(&profile(ObservationMode::Lossless)).expect("valid profile");
        let history = validate_history(&plan, &observations(&plan)).expect("history");
        let swarm = SwarmSelection {
            seed: SEED,
            selected_features: Vec::new(),
            selected_fault_classes: Vec::new(),
            unexplored_features: Vec::new(),
            unexplored_fault_classes: Vec::new(),
            weights: Vec::new(),
        };
        let replay = ReplayComparison {
            accepted: true,
            first_mismatch: None,
        };
        let history_ref = history_report_ref(&history).expect("history identity");
        let replay_ref = replay_comparison_ref(&replay).expect("replay identity");
        let mut receipt = SmrEvidenceReceipt {
            schema: RECEIPT_SCHEMA.to_string(),
            profile_ref: plan.profile_ref.clone(),
            build_ref: digest(b"build"),
            adapter_ref: digest(b"adapter"),
            observer_ref: digest(b"observer"),
            schedule_ref: digest(b"schedule"),
            history_ref,
            replay_ref,
            observation_mode: ObservationMode::Lossless,
            dropped_observations: 0,
            seed: SEED,
            swarm,
            fault_outcomes: vec![FaultOutcome {
                action_id: "fault-selected".to_string(),
                class: FaultClass::Network,
                stages: vec![FaultStage::Selected],
                effect_record_ref: None,
            }],
            history,
            stabilization_facts: None,
            liveness: LivenessReport {
                verdict: LivenessVerdict::NotEvaluated,
                progress: 0,
                blockers: vec!["no stabilization window".to_string()],
            },
            bounds: plan.profile.bounds.clone(),
            replay,
            terminal_status: TerminalStatus::Completed,
            non_claims: REQUIRED_NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_string())
                .collect(),
        };
        validate_evidence_receipt(&receipt, &plan).expect("bounded evidence");
        let mut missing_build = receipt.clone();
        missing_build.build_ref.clear();
        assert_eq!(
            validate_evidence_receipt(&missing_build, &plan)
                .expect_err("missing build")
                .class,
            "reference-digest"
        );
        let mut missing_liveness_facts = receipt.clone();
        missing_liveness_facts.liveness = LivenessReport {
            verdict: LivenessVerdict::Pass,
            progress: REQUIRED_PROGRESS,
            blockers: Vec::new(),
        };
        assert_eq!(
            validate_evidence_receipt(&missing_liveness_facts, &plan)
                .expect_err("missing liveness facts")
                .class,
            "evidence-liveness"
        );
        let mut history_drift = receipt.clone();
        history_drift.history_ref = digest(b"changed-history");
        assert_eq!(
            validate_evidence_receipt(&history_drift, &plan)
                .expect_err("history digest drift")
                .class,
            "evidence-history-drift"
        );
        let mut replay_mismatch = receipt.clone();
        replay_mismatch.replay.accepted = false;
        replay_mismatch.replay.first_mismatch = Some("observations".to_string());
        replay_mismatch.replay_ref =
            replay_comparison_ref(&replay_mismatch.replay).expect("mismatch identity");
        assert_eq!(
            validate_evidence_receipt(&replay_mismatch, &plan)
                .expect_err("replay mismatch")
                .class,
            "evidence-replay"
        );
        receipt.non_claims.pop();
        assert_eq!(
            validate_evidence_receipt(&receipt, &plan)
                .expect_err("missing non-claim")
                .class,
            "evidence-overclaim"
        );
    }
}
