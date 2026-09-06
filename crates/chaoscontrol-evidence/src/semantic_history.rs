//! Pure semantic history admission and bounded linearizability checking.
//!
//! Filesystem access, process execution, persistence, and network transport do
//! not belong in this module. r[impl chaoscontrol.semantic_history.boundary]

use sha2::Digest;

pub const SEMANTIC_HISTORY_SCHEMA_VERSION: u64 = 2;
pub const SEMANTIC_REPORT_SCHEMA_VERSION: u64 = 1;
pub const JEPSEN_ADAPTER_SCHEMA_VERSION: u64 = 1;
pub const LEGACY_HISTORY_SCHEMA_VERSION: u64 = 1;
pub const DEFAULT_MAX_OPERATIONS: usize = 64;
pub const DEFAULT_MAX_STATES: usize = 50_000;
pub const DEFAULT_MAX_BRANCHES: usize = 100_000;
pub const DEFAULT_MAX_DEPTH: usize = 64;
pub const DEFAULT_MAX_MEMO_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_REDUCTIONS: usize = 256;

const HISTORY_ID_DOMAIN: &[u8] = b"chaoscontrol.semantic-history.v2";
const MODEL_STATE_DOMAIN: &[u8] = b"chaoscontrol.semantic-model-state.v1";
const MEMO_DOMAIN: &[u8] = b"chaoscontrol.semantic-search-memo.v1";
const BLAKE3_PREFIX: &str = "blake3:";
const SHA256_PREFIX: &str = "sha256:";
const REQUIRED_SCOPE: &str = "finite admitted history within declared bounds";
const REFERENCE_TOOL_ID: &str = "jepsen-compatible-reference.v1";
const NATIVE_TOOL_ID: &str = "chaoscontrol-semantic-checker.v1";
const NON_CLAIMS: [&str; 9] = [
    "does not prove system correctness",
    "does not prove checker soundness",
    "does not prove exhaustive schedule coverage",
    "does not prove deterministic replay",
    "does not prove fault effect from selection",
    "does not prove durability or transactions",
    "does not prove security",
    "does not prove release readiness",
    "does not infer causation from temporal overlap",
];
const FORBIDDEN_SCOPE_FRAGMENTS: [&str; 6] = [
    "system correct",
    "checker sound",
    "exhaustive",
    "production ready",
    "proves replay",
    "caused by",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticErrorClass {
    Schema,
    Identity,
    EventOrder,
    Pairing,
    Duplicate,
    Retry,
    Outcome,
    Completeness,
    Model,
    Bounds,
    LegacyPromotion,
    ReferenceDisagreement,
    ClaimBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticError {
    pub class: SemanticErrorClass,
    pub message: String,
}

impl SemanticError {
    fn new(class: SemanticErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }
}

impl ::std::fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(formatter, "{:?}: {}", self.class, self.message)
    }
}

impl std::error::Error for SemanticError {}

pub type SemanticResult<T> = Result<T, SemanticError>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SemanticValue {
    Null,
    Bool(bool),
    Integer(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(tag = "function", rename_all = "snake_case")]
pub enum OperationInput {
    Read,
    Write {
        value: SemanticValue,
    },
    CompareAndSwap {
        expected: SemanticValue,
        replacement: SemanticValue,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionOutcome {
    Ok,
    Fail,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveOutcome {
    Ok,
    Fail,
    Info,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingFinalization {
    pub policy_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InvocationEvent {
    pub event_index: usize,
    pub logical_operation_id: String,
    pub attempt_id: String,
    pub retry_of_attempt: Option<String>,
    pub process: String,
    pub object_key: String,
    pub input: OperationInput,
    pub controller_time_ns: u64,
    pub source_artifact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompletionEvent {
    pub event_index: usize,
    pub attempt_id: String,
    pub outcome: CompletionOutcome,
    pub output: Option<SemanticValue>,
    pub controller_time_ns: u64,
    pub finalization: Option<PendingFinalization>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultEffectPhase {
    Selected,
    Applied,
    Observed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultEvent {
    pub event_index: usize,
    pub fault_attempt_id: String,
    pub phase: FaultEffectPhase,
    pub effect_record_ref: Option<String>,
    pub controller_time_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleEvent {
    pub event_index: usize,
    pub phase: String,
    pub controller_time_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticEvent {
    Invoke(InvocationEvent),
    Complete(CompletionEvent),
    Fault(FaultEvent),
    Lifecycle(LifecycleEvent),
}

impl SemanticEvent {
    pub fn event_index(&self) -> usize {
        match self {
            Self::Invoke(event) => event.event_index,
            Self::Complete(event) => event.event_index,
            Self::Fault(event) => event.event_index,
            Self::Lifecycle(event) => event.event_index,
        }
    }

    pub fn controller_time_ns(&self) -> u64 {
        match self {
            Self::Invoke(event) => event.controller_time_ns,
            Self::Complete(event) => event.controller_time_ns,
            Self::Fault(event) => event.controller_time_ns,
            Self::Lifecycle(event) => event.controller_time_ns,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchBounds {
    pub max_operations: usize,
    pub max_states: usize,
    pub max_branches: usize,
    pub max_depth: usize,
    pub max_memo_bytes: usize,
    pub max_reductions: usize,
}

impl Default for SearchBounds {
    fn default() -> Self {
        Self {
            max_operations: DEFAULT_MAX_OPERATIONS,
            max_states: DEFAULT_MAX_STATES,
            max_branches: DEFAULT_MAX_BRANCHES,
            max_depth: DEFAULT_MAX_DEPTH,
            max_memo_bytes: DEFAULT_MAX_MEMO_BYTES,
            max_reductions: DEFAULT_MAX_REDUCTIONS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CheckerProfile {
    pub profile_id: String,
    pub model: String,
    pub independent_key_decomposition: bool,
    pub bounds: SearchBounds,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompletenessAccounting {
    pub invocations: usize,
    pub completions: usize,
    pub pending: usize,
    pub applied_faults: usize,
    pub observed_faults: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticHistory {
    pub schema_version: u64,
    pub workload_profile: String,
    pub source_artifact: String,
    pub profile: CheckerProfile,
    pub completeness: CompletenessAccounting,
    pub events: Vec<SemanticEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdmittedOperation {
    pub logical_operation_id: String,
    pub attempt_id: String,
    pub process: String,
    pub object_key: String,
    pub input: OperationInput,
    pub output: Option<SemanticValue>,
    pub outcome: EffectiveOutcome,
    pub invocation_event_index: usize,
    pub completion_event_index: Option<usize>,
    pub invocation_time_ns: u64,
    pub completion_time_ns: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdmittedHistory {
    pub history_id: String,
    pub workload_profile: String,
    pub source_artifact: String,
    pub profile: CheckerProfile,
    pub operations: Vec<AdmittedOperation>,
    pub environment_events: Vec<SemanticEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InvocationRecord {
    event: InvocationEvent,
    completion: Option<CompletionEvent>,
}

fn require(
    condition: bool,
    class: SemanticErrorClass,
    message: impl Into<String>,
) -> SemanticResult<()> {
    if condition {
        Ok(())
    } else {
        Err(SemanticError::new(class, message))
    }
}

fn require_nonempty(value: &str, field: &str) -> SemanticResult<()> {
    require(
        !value.trim().is_empty(),
        SemanticErrorClass::Schema,
        format!("{field} must not be empty"),
    )
}

fn hash_len_delimited(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("bounded semantic input length must fit u64");
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
}

fn canonical_history_identity(history: &SemanticHistory) -> SemanticResult<String> {
    let bytes = serde_json::to_vec(history).map_err(|error| {
        SemanticError::new(
            SemanticErrorClass::Identity,
            format!("semantic history canonicalization failed: {error}"),
        )
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(HISTORY_ID_DOMAIN);
    hash_len_delimited(&mut hasher, &bytes);
    Ok(format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex()))
}

/// Admit and pair a v2 semantic event history without I/O.
/// r[impl chaoscontrol.semantic_history.schema]
/// r[impl chaoscontrol.semantic_history.outcomes]
pub fn admit_history(history: &SemanticHistory) -> SemanticResult<AdmittedHistory> {
    require(
        history.schema_version == SEMANTIC_HISTORY_SCHEMA_VERSION,
        SemanticErrorClass::Schema,
        format!("semantic history schema_version must be {SEMANTIC_HISTORY_SCHEMA_VERSION}"),
    )?;
    require_nonempty(&history.workload_profile, "workload_profile")?;
    require_nonempty(&history.source_artifact, "source_artifact")?;
    require_nonempty(&history.profile.profile_id, "profile.profile_id")?;
    require_nonempty(&history.profile.model, "profile.model")?;
    validate_bounds(&history.profile.bounds)?;

    let mut records = std::collections::BTreeMap::<String, InvocationRecord>::new();
    let mut logical_latest = std::collections::BTreeMap::<String, String>::new();
    let mut environment_events = Vec::new();
    let mut prior_time = 0;

    for (expected_index, event) in history.events.iter().enumerate() {
        require(
            event.event_index() == expected_index,
            SemanticErrorClass::EventOrder,
            format!(
                "event index {} must equal canonical position {expected_index}",
                event.event_index()
            ),
        )?;
        require(
            expected_index == 0 || event.controller_time_ns() >= prior_time,
            SemanticErrorClass::EventOrder,
            format!("event {expected_index} moves controller time backward"),
        )?;
        prior_time = event.controller_time_ns();

        match event {
            SemanticEvent::Invoke(invocation) => {
                validate_invocation(
                    invocation,
                    &records,
                    &logical_latest,
                    &history.source_artifact,
                )?;
                require(
                    !records.contains_key(&invocation.attempt_id),
                    SemanticErrorClass::Duplicate,
                    format!("duplicate attempt id {}", invocation.attempt_id),
                )?;
                if let Some(prior_attempt) = logical_latest.get(&invocation.logical_operation_id) {
                    require(
                        invocation.retry_of_attempt.as_ref() == Some(prior_attempt),
                        SemanticErrorClass::Retry,
                        format!(
                            "logical operation {} repeats without binding its prior attempt",
                            invocation.logical_operation_id
                        ),
                    )?;
                }
                logical_latest.insert(
                    invocation.logical_operation_id.clone(),
                    invocation.attempt_id.clone(),
                );
                records.insert(
                    invocation.attempt_id.clone(),
                    InvocationRecord {
                        event: invocation.clone(),
                        completion: None,
                    },
                );
            }
            SemanticEvent::Complete(completion) => {
                validate_completion(completion)?;
                let record = records.get_mut(&completion.attempt_id).ok_or_else(|| {
                    SemanticError::new(
                        SemanticErrorClass::Pairing,
                        format!(
                            "completion {} has no prior invocation",
                            completion.attempt_id
                        ),
                    )
                })?;
                require(
                    record.completion.is_none(),
                    SemanticErrorClass::Duplicate,
                    format!(
                        "attempt {} has duplicate completions",
                        completion.attempt_id
                    ),
                )?;
                require(
                    completion.controller_time_ns >= record.event.controller_time_ns,
                    SemanticErrorClass::Pairing,
                    format!(
                        "completion {} precedes its invocation",
                        completion.attempt_id
                    ),
                )?;
                record.completion = Some(completion.clone());
            }
            SemanticEvent::Fault(fault) => {
                validate_fault_event(fault)?;
                environment_events.push(event.clone());
            }
            SemanticEvent::Lifecycle(lifecycle) => {
                require_nonempty(&lifecycle.phase, "lifecycle.phase")?;
                environment_events.push(event.clone());
            }
        }
    }

    let mut operations = Vec::with_capacity(records.len());
    for record in records.values() {
        let (output, outcome, completion_event_index, completion_time_ns) =
            if let Some(completion) = &record.completion {
                (
                    completion.output.clone(),
                    match completion.outcome {
                        CompletionOutcome::Ok => EffectiveOutcome::Ok,
                        CompletionOutcome::Fail => EffectiveOutcome::Fail,
                        CompletionOutcome::Info => EffectiveOutcome::Info,
                    },
                    Some(completion.event_index),
                    Some(completion.controller_time_ns),
                )
            } else {
                (None, EffectiveOutcome::Pending, None, None)
            };
        operations.push(AdmittedOperation {
            logical_operation_id: record.event.logical_operation_id.clone(),
            attempt_id: record.event.attempt_id.clone(),
            process: record.event.process.clone(),
            object_key: record.event.object_key.clone(),
            input: record.event.input.clone(),
            output,
            outcome,
            invocation_event_index: record.event.event_index,
            completion_event_index,
            invocation_time_ns: record.event.controller_time_ns,
            completion_time_ns,
        });
    }
    operations.sort_by_key(|operation| operation.invocation_event_index);
    validate_completeness(history, &operations, &environment_events)?;

    Ok(AdmittedHistory {
        history_id: canonical_history_identity(history)?,
        workload_profile: history.workload_profile.clone(),
        source_artifact: history.source_artifact.clone(),
        profile: history.profile.clone(),
        operations,
        environment_events,
    })
}

fn validate_bounds(bounds: &SearchBounds) -> SemanticResult<()> {
    require(
        bounds.max_operations > 0
            && bounds.max_states > 0
            && bounds.max_branches > 0
            && bounds.max_depth > 0
            && bounds.max_memo_bytes > 0,
        SemanticErrorClass::Bounds,
        "all search bounds except max_reductions must be nonzero",
    )
}

fn validate_invocation(
    invocation: &InvocationEvent,
    records: &std::collections::BTreeMap<String, InvocationRecord>,
    logical_latest: &std::collections::BTreeMap<String, String>,
    source_artifact: &str,
) -> SemanticResult<()> {
    require_nonempty(&invocation.logical_operation_id, "logical_operation_id")?;
    require_nonempty(&invocation.attempt_id, "attempt_id")?;
    require_nonempty(&invocation.process, "process")?;
    require_nonempty(&invocation.object_key, "object_key")?;
    require_nonempty(&invocation.source_artifact, "source_artifact")?;
    require(
        invocation.source_artifact.starts_with(BLAKE3_PREFIX)
            && invocation.source_artifact == source_artifact,
        SemanticErrorClass::Identity,
        "invocation source_artifact must match the history BLAKE3 identity",
    )?;
    if let Some(retry_of) = &invocation.retry_of_attempt {
        let prior = records.get(retry_of).ok_or_else(|| {
            SemanticError::new(
                SemanticErrorClass::Retry,
                format!("retry references unknown prior attempt {retry_of}"),
            )
        })?;
        require(
            retry_of != &invocation.attempt_id,
            SemanticErrorClass::Retry,
            "retry attempt identity must be distinct",
        )?;
        require(
            prior.event.logical_operation_id == invocation.logical_operation_id
                && prior.event.process == invocation.process
                && prior.event.object_key == invocation.object_key
                && prior.event.input == invocation.input,
            SemanticErrorClass::Retry,
            format!(
                "retry {} changes logical operation identity or content",
                invocation.attempt_id
            ),
        )?;
        require(
            logical_latest.get(&invocation.logical_operation_id) == Some(retry_of),
            SemanticErrorClass::Retry,
            "retry must bind the latest attempt for its logical operation",
        )?;
    }
    Ok(())
}

fn validate_fault_event(fault: &FaultEvent) -> SemanticResult<()> {
    require_nonempty(&fault.fault_attempt_id, "fault.fault_attempt_id")?;
    match fault.phase {
        FaultEffectPhase::Selected => require(
            fault.effect_record_ref.is_none(),
            SemanticErrorClass::Outcome,
            "selected fault events cannot claim an effect record",
        ),
        FaultEffectPhase::Applied | FaultEffectPhase::Observed => {
            let effect_record_ref = fault.effect_record_ref.as_deref().unwrap_or_default();
            require(
                effect_record_ref.starts_with(BLAKE3_PREFIX),
                SemanticErrorClass::Identity,
                "applied and observed fault events require an admitted BLAKE3 effect record",
            )
        }
    }
}

fn validate_completion(completion: &CompletionEvent) -> SemanticResult<()> {
    require_nonempty(&completion.attempt_id, "completion.attempt_id")?;
    match completion.outcome {
        CompletionOutcome::Info => {
            if let Some(finalization) = &completion.finalization {
                require_nonempty(&finalization.policy_id, "finalization.policy_id")?;
                require_nonempty(&finalization.reason, "finalization.reason")?;
            }
        }
        CompletionOutcome::Ok | CompletionOutcome::Fail => {
            require(
                completion.finalization.is_none(),
                SemanticErrorClass::Outcome,
                "only info completions can contain pending finalization evidence",
            )?;
        }
    }
    Ok(())
}

fn validate_completeness(
    history: &SemanticHistory,
    operations: &[AdmittedOperation],
    environment_events: &[SemanticEvent],
) -> SemanticResult<()> {
    let completions = operations
        .iter()
        .filter(|operation| operation.completion_event_index.is_some())
        .count();
    let pending = operations
        .iter()
        .filter(|operation| operation.outcome == EffectiveOutcome::Pending)
        .count();
    let applied_faults = environment_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                SemanticEvent::Fault(FaultEvent {
                    phase: FaultEffectPhase::Applied,
                    ..
                })
            )
        })
        .count();
    let observed_faults = environment_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                SemanticEvent::Fault(FaultEvent {
                    phase: FaultEffectPhase::Observed,
                    ..
                })
            )
        })
        .count();
    let actual = CompletenessAccounting {
        invocations: operations.len(),
        completions,
        pending,
        applied_faults,
        observed_faults,
    };
    require(
        history.completeness == actual,
        SemanticErrorClass::Completeness,
        format!(
            "completeness accounting mismatch: declared {:?}, actual {:?}",
            history.completeness, actual
        ),
    )
}

/// Return the canonical v2 identity after complete admission.
/// r[impl chaoscontrol.semantic_history.identity]
pub fn semantic_history_identity(history: &SemanticHistory) -> SemanticResult<String> {
    Ok(admit_history(history)?.history_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    Register,
    CompareAndSwapRegister,
    CrossKeyRegister,
}

impl ModelKind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "register" => Some(Self::Register),
            "compare-and-swap-register" => Some(Self::CompareAndSwapRegister),
            "cross-key-register" => Some(Self::CrossKeyRegister),
            _ => None,
        }
    }

    fn key_isolated(self) -> bool {
        !matches!(self, Self::CrossKeyRegister)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
struct ModelState {
    values: std::collections::BTreeMap<String, SemanticValue>,
}

impl ModelState {
    fn new() -> Self {
        Self {
            values: std::collections::BTreeMap::new(),
        }
    }

    fn value(&self, key: &str) -> SemanticValue {
        self.values.get(key).cloned().unwrap_or(SemanticValue::Null)
    }

    fn identity(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("model state serialization must succeed");
        let mut hasher = blake3::Hasher::new();
        hasher.update(MODEL_STATE_DOMAIN);
        hash_len_delimited(&mut hasher, &bytes);
        format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex())
    }
}

/// Apply one operation to a pure first-party object model.
/// r[impl chaoscontrol.semantic_history.models]
fn transition(
    model: ModelKind,
    state: &ModelState,
    operation: &AdmittedOperation,
) -> Option<ModelState> {
    let current = state.value(&operation.object_key);
    let mut next = state.clone();
    match (&operation.input, model) {
        (OperationInput::Read, _) => (operation.output.as_ref() == Some(&current)).then_some(next),
        (OperationInput::Write { value }, ModelKind::Register)
        | (OperationInput::Write { value }, ModelKind::CompareAndSwapRegister)
        | (OperationInput::Write { value }, ModelKind::CrossKeyRegister) => {
            next.values
                .insert(operation.object_key.clone(), value.clone());
            Some(next)
        }
        (
            OperationInput::CompareAndSwap {
                expected,
                replacement,
            },
            ModelKind::CompareAndSwapRegister,
        ) => {
            let success = current == *expected;
            if operation.output != Some(SemanticValue::Bool(success)) {
                return None;
            }
            if success {
                next.values
                    .insert(operation.object_key.clone(), replacement.clone());
            }
            Some(next)
        }
        (OperationInput::CompareAndSwap { .. }, ModelKind::Register)
        | (OperationInput::CompareAndSwap { .. }, ModelKind::CrossKeyRegister) => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearizabilityVerdict {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownClass {
    PendingOperations,
    MaxOperations,
    UnsupportedModel,
    DecompositionNotAdmitted,
    MaxStates,
    MaxBranches,
    MaxDepth,
    MaxMemoBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnknownBlocker {
    pub class: UnknownClass,
    pub bound_name: Option<String>,
    pub bound_value: Option<usize>,
    pub observed: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinearizationStep {
    pub attempt_id: String,
    pub took_effect: bool,
    pub state_before: String,
    pub state_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InvalidWitness {
    pub remaining_attempt_ids: Vec<String>,
    pub model_state: String,
    pub failure_class: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReductionStatus {
    NotApplicable,
    LocallyReduced,
    BudgetLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReducedHistory {
    pub status: ReductionStatus,
    pub retained_attempt_ids: Vec<String>,
    pub attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStatus {
    NotRun,
    Agreement,
    Disagreement,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticCheckReport {
    pub schema_version: u64,
    pub history_id: String,
    pub checker_id: String,
    pub model: String,
    pub profile_id: String,
    pub bounds: SearchBounds,
    pub completeness: CompletenessAccounting,
    pub verdict: LinearizabilityVerdict,
    pub witness: Vec<LinearizationStep>,
    pub invalid: Option<InvalidWitness>,
    pub unknown: Option<UnknownBlocker>,
    pub reduction: ReducedHistory,
    pub reference_status: ReferenceStatus,
    pub scope: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SearchAttempt {
    Valid(Vec<LinearizationStep>),
    Invalid(InvalidWitness),
    Unknown(UnknownBlocker),
}

struct SearchContext<'a> {
    model: ModelKind,
    operations: &'a [AdmittedOperation],
    predecessors: Vec<std::collections::BTreeSet<usize>>,
    bounds: &'a SearchBounds,
    states_seen: usize,
    branches_seen: usize,
    memo_bytes: usize,
    invalid_memo: std::collections::BTreeSet<String>,
}

impl<'a> SearchContext<'a> {
    fn search(&mut self) -> SearchAttempt {
        let remaining = (0..self.operations.len()).collect();
        self.visit(ModelState::new(), remaining, 0)
    }

    fn visit(
        &mut self,
        state: ModelState,
        remaining: std::collections::BTreeSet<usize>,
        depth: usize,
    ) -> SearchAttempt {
        if remaining.is_empty() {
            return SearchAttempt::Valid(Vec::new());
        }
        if depth >= self.bounds.max_depth {
            return SearchAttempt::Unknown(bound_blocker(
                UnknownClass::MaxDepth,
                "max_depth",
                self.bounds.max_depth,
                depth,
            ));
        }
        if self.states_seen >= self.bounds.max_states {
            return SearchAttempt::Unknown(bound_blocker(
                UnknownClass::MaxStates,
                "max_states",
                self.bounds.max_states,
                self.states_seen,
            ));
        }
        self.states_seen += 1;

        let memo_key = search_memo_key(&state, &remaining);
        if self.invalid_memo.contains(&memo_key) {
            return SearchAttempt::Invalid(invalid_witness(self.operations, &remaining, &state));
        }

        let candidates: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|index| self.predecessors[*index].is_disjoint(&remaining))
            .collect();
        let mut first_unknown = None;

        for index in candidates {
            let operation = &self.operations[index];
            let mut decisions = vec![true];
            if operation.outcome == EffectiveOutcome::Info {
                decisions.insert(0, false);
            }
            for took_effect in decisions {
                if self.branches_seen >= self.bounds.max_branches {
                    first_unknown.get_or_insert_with(|| {
                        bound_blocker(
                            UnknownClass::MaxBranches,
                            "max_branches",
                            self.bounds.max_branches,
                            self.branches_seen,
                        )
                    });
                    continue;
                }
                self.branches_seen += 1;
                let next_state = if took_effect {
                    let Some(next) = transition(self.model, &state, operation) else {
                        continue;
                    };
                    next
                } else {
                    state.clone()
                };
                let mut next_remaining = remaining.clone();
                let removed = next_remaining.remove(&index);
                assert!(removed, "search candidate must be present");
                match self.visit(next_state.clone(), next_remaining, depth + 1) {
                    SearchAttempt::Valid(mut suffix) => {
                        let mut witness = vec![LinearizationStep {
                            attempt_id: operation.attempt_id.clone(),
                            took_effect,
                            state_before: state.identity(),
                            state_after: next_state.identity(),
                        }];
                        witness.append(&mut suffix);
                        return SearchAttempt::Valid(witness);
                    }
                    SearchAttempt::Unknown(blocker) => {
                        first_unknown.get_or_insert(blocker);
                    }
                    SearchAttempt::Invalid(_) => {}
                }
            }
        }

        if let Some(blocker) = first_unknown {
            return SearchAttempt::Unknown(blocker);
        }
        let added_bytes = memo_key.len();
        if self.memo_bytes.saturating_add(added_bytes) > self.bounds.max_memo_bytes {
            return SearchAttempt::Unknown(bound_blocker(
                UnknownClass::MaxMemoBytes,
                "max_memo_bytes",
                self.bounds.max_memo_bytes,
                self.memo_bytes.saturating_add(added_bytes),
            ));
        }
        self.memo_bytes += added_bytes;
        self.invalid_memo.insert(memo_key);
        SearchAttempt::Invalid(invalid_witness(self.operations, &remaining, &state))
    }
}

fn bound_blocker(class: UnknownClass, name: &str, value: usize, observed: usize) -> UnknownBlocker {
    UnknownBlocker {
        class,
        bound_name: Some(name.to_string()),
        bound_value: Some(value),
        observed: Some(observed),
        message: format!("search exhausted {name}={value} at observed={observed}"),
    }
}

fn invalid_witness(
    operations: &[AdmittedOperation],
    remaining: &std::collections::BTreeSet<usize>,
    state: &ModelState,
) -> InvalidWitness {
    InvalidWitness {
        remaining_attempt_ids: remaining
            .iter()
            .map(|index| operations[*index].attempt_id.clone())
            .collect(),
        model_state: state.identity(),
        failure_class: "no_legal_model_transition".to_string(),
    }
}

fn search_memo_key(state: &ModelState, remaining: &std::collections::BTreeSet<usize>) -> String {
    let state_bytes = serde_json::to_vec(state).expect("model state serialization must succeed");
    let remaining_bytes = serde_json::to_vec(remaining).expect("remaining serialization succeeds");
    let mut hasher = blake3::Hasher::new();
    hasher.update(MEMO_DOMAIN);
    hash_len_delimited(&mut hasher, &state_bytes);
    hash_len_delimited(&mut hasher, &remaining_bytes);
    hasher.finalize().to_hex().to_string()
}

fn build_predecessors(operations: &[AdmittedOperation]) -> Vec<std::collections::BTreeSet<usize>> {
    let mut predecessors = vec![std::collections::BTreeSet::new(); operations.len()];
    for (before_index, before) in operations.iter().enumerate() {
        let Some(before_completion) = before.completion_time_ns else {
            continue;
        };
        for (after_index, after) in operations.iter().enumerate() {
            if before_index != after_index && before_completion < after.invocation_time_ns {
                predecessors[after_index].insert(before_index);
            }
        }
    }
    predecessors
}

fn search_operations(
    model: ModelKind,
    operations: &[AdmittedOperation],
    bounds: &SearchBounds,
) -> SearchAttempt {
    let searchable: Vec<AdmittedOperation> = operations
        .iter()
        .filter(|operation| operation.outcome != EffectiveOutcome::Fail)
        .cloned()
        .collect();
    let predecessors = build_predecessors(&searchable);
    SearchContext {
        model,
        operations: &searchable,
        predecessors,
        bounds,
        states_seen: 0,
        branches_seen: 0,
        memo_bytes: 0,
        invalid_memo: std::collections::BTreeSet::new(),
    }
    .search()
}

fn check_with_decomposition(model: ModelKind, history: &AdmittedHistory) -> SearchAttempt {
    if !history.profile.independent_key_decomposition {
        return search_operations(model, &history.operations, &history.profile.bounds);
    }
    if !model.key_isolated() {
        return SearchAttempt::Unknown(UnknownBlocker {
            class: UnknownClass::DecompositionNotAdmitted,
            bound_name: None,
            bound_value: None,
            observed: None,
            message: "model does not declare independent-key isolation".to_string(),
        });
    }
    let mut by_key = std::collections::BTreeMap::<String, Vec<AdmittedOperation>>::new();
    for operation in &history.operations {
        by_key
            .entry(operation.object_key.clone())
            .or_default()
            .push(operation.clone());
    }
    let mut witness = Vec::new();
    for operations in by_key.values() {
        match search_operations(model, operations, &history.profile.bounds) {
            SearchAttempt::Valid(mut key_witness) => witness.append(&mut key_witness),
            other => return other,
        }
    }
    witness.sort_by_key(|step| {
        history
            .operations
            .iter()
            .position(|operation| operation.attempt_id == step.attempt_id)
            .unwrap_or(usize::MAX)
    });
    SearchAttempt::Valid(witness)
}

/// Check one admitted finite history under explicit bounds.
/// r[impl chaoscontrol.semantic_history.linearizability]
/// r[impl chaoscontrol.semantic_history.witness]
pub fn check_linearizability(history: &SemanticHistory) -> SemanticResult<SemanticCheckReport> {
    let admitted = admit_history(history)?;
    let completeness = history.completeness.clone();
    let pending_count = admitted
        .operations
        .iter()
        .filter(|operation| operation.outcome == EffectiveOutcome::Pending)
        .count();
    let attempt = if admitted.operations.len() > admitted.profile.bounds.max_operations {
        SearchAttempt::Unknown(bound_blocker(
            UnknownClass::MaxOperations,
            "max_operations",
            admitted.profile.bounds.max_operations,
            admitted.operations.len(),
        ))
    } else if pending_count > 0 {
        SearchAttempt::Unknown(UnknownBlocker {
            class: UnknownClass::PendingOperations,
            bound_name: None,
            bound_value: None,
            observed: Some(pending_count),
            message: format!("history contains {pending_count} unfinalized pending operations"),
        })
    } else if let Some(model) = ModelKind::parse(&admitted.profile.model) {
        check_with_decomposition(model, &admitted)
    } else {
        SearchAttempt::Unknown(UnknownBlocker {
            class: UnknownClass::UnsupportedModel,
            bound_name: None,
            bound_value: None,
            observed: None,
            message: format!("unsupported model {}", admitted.profile.model),
        })
    };

    let (verdict, witness, invalid, unknown) = match attempt {
        SearchAttempt::Valid(witness) => (LinearizabilityVerdict::Valid, witness, None, None),
        SearchAttempt::Invalid(invalid) => (
            LinearizabilityVerdict::Invalid,
            Vec::new(),
            Some(invalid),
            None,
        ),
        SearchAttempt::Unknown(unknown) => (
            LinearizabilityVerdict::Unknown,
            Vec::new(),
            None,
            Some(unknown),
        ),
    };
    let reduction = if verdict == LinearizabilityVerdict::Invalid {
        reduce_invalid_history(&admitted)
    } else {
        ReducedHistory {
            status: ReductionStatus::NotApplicable,
            retained_attempt_ids: Vec::new(),
            attempts: 0,
        }
    };
    let report = SemanticCheckReport {
        schema_version: SEMANTIC_REPORT_SCHEMA_VERSION,
        history_id: admitted.history_id,
        checker_id: NATIVE_TOOL_ID.to_string(),
        model: admitted.profile.model.clone(),
        profile_id: admitted.profile.profile_id.clone(),
        bounds: admitted.profile.bounds.clone(),
        completeness,
        verdict,
        witness,
        invalid,
        unknown,
        reduction,
        reference_status: ReferenceStatus::NotRun,
        scope: REQUIRED_SCOPE.to_string(),
        non_claims: NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    };
    validate_semantic_report(&report, history)?;
    Ok(report)
}

fn reduce_invalid_history(history: &AdmittedHistory) -> ReducedHistory {
    let Some(model) = ModelKind::parse(&history.profile.model) else {
        return ReducedHistory {
            status: ReductionStatus::NotApplicable,
            retained_attempt_ids: Vec::new(),
            attempts: 0,
        };
    };
    let mut retained = history.operations.clone();
    let mut attempts = 0;
    let mut cursor = 0;
    while cursor < retained.len() && attempts < history.profile.bounds.max_reductions {
        let mut candidate = retained.clone();
        candidate.remove(cursor);
        attempts += 1;
        if matches!(
            search_operations(model, &candidate, &history.profile.bounds),
            SearchAttempt::Invalid(_)
        ) {
            retained = candidate;
        } else {
            cursor += 1;
        }
    }
    let budget_limited = cursor < retained.len();
    ReducedHistory {
        status: if budget_limited {
            ReductionStatus::BudgetLimited
        } else {
            ReductionStatus::LocallyReduced
        },
        retained_attempt_ids: retained
            .iter()
            .map(|operation| operation.attempt_id.clone())
            .collect(),
        attempts,
    }
}

/// Validate evidence identity, terminal shape, and claim boundaries.
/// r[impl chaoscontrol.semantic_history.evidence]
pub fn validate_semantic_report(
    report: &SemanticCheckReport,
    history: &SemanticHistory,
) -> SemanticResult<()> {
    let admitted = admit_history(history)?;
    require(
        report.schema_version == SEMANTIC_REPORT_SCHEMA_VERSION,
        SemanticErrorClass::Schema,
        "semantic report schema version mismatch",
    )?;
    require(
        report.history_id == admitted.history_id
            && report.checker_id == NATIVE_TOOL_ID
            && report.model == admitted.profile.model
            && report.profile_id == admitted.profile.profile_id
            && report.bounds == admitted.profile.bounds
            && report.completeness == history.completeness,
        SemanticErrorClass::Identity,
        "semantic report does not bind the admitted history, profile, or bounds",
    )?;
    let terminal_shape = match report.verdict {
        LinearizabilityVerdict::Valid => {
            report.invalid.is_none() && report.unknown.is_none() && !report.witness.is_empty()
        }
        LinearizabilityVerdict::Invalid => {
            report.invalid.is_some() && report.unknown.is_none() && report.witness.is_empty()
        }
        LinearizabilityVerdict::Unknown => {
            report.invalid.is_none() && report.unknown.is_some() && report.witness.is_empty()
        }
    };
    require(
        terminal_shape,
        SemanticErrorClass::Outcome,
        "semantic report terminal fields do not match its verdict",
    )?;
    let scope = report.scope.to_ascii_lowercase();
    require(
        report.scope == REQUIRED_SCOPE
            && FORBIDDEN_SCOPE_FRAGMENTS
                .iter()
                .all(|fragment| !scope.contains(fragment)),
        SemanticErrorClass::ClaimBoundary,
        "semantic report scope promotes a bounded verdict",
    )?;
    for required in NON_CLAIMS {
        require(
            report.non_claims.iter().any(|claim| claim == required),
            SemanticErrorClass::ClaimBoundary,
            format!("semantic report is missing non-claim: {required}"),
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LegacyHistoryEvidence {
    pub schema_version: u64,
    pub history_sha256: String,
    pub completion_order_verdict: String,
    pub limitation: String,
}

/// Preserve v1 SHA-256 transport identity without promoting its verdict.
/// r[impl chaoscontrol.semantic_history.compatibility]
pub fn read_legacy_history_value(
    value: &serde_json::Value,
) -> SemanticResult<LegacyHistoryEvidence> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        SemanticError::new(
            SemanticErrorClass::Schema,
            format!("legacy history serialization failed: {error}"),
        )
    })?;
    let mut hasher = ::sha2::Sha256::new();
    hasher.update(bytes);
    Ok(LegacyHistoryEvidence {
        schema_version: LEGACY_HISTORY_SCHEMA_VERSION,
        history_sha256: format!("{SHA256_PREFIX}{:x}", hasher.finalize()),
        completion_order_verdict: value
            .get("verdict")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        limitation: "legacy completion-order evidence; not linearizability evidence".to_string(),
    })
}

pub fn reject_legacy_promotion(_legacy: &LegacyHistoryEvidence) -> SemanticResult<()> {
    Err(SemanticError::new(
        SemanticErrorClass::LegacyPromotion,
        "history v1 completion-order evidence cannot satisfy a v2 linearizability claim",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceEventType {
    Invoke,
    Ok,
    Fail,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceEvent {
    pub index: usize,
    pub process: String,
    pub event_type: ReferenceEventType,
    pub function: String,
    pub key: String,
    pub value: serde_json::Value,
    pub attempt_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JepsenCompatibleHistory {
    pub schema_version: u64,
    pub source_history_id: String,
    pub events: Vec<ReferenceEvent>,
}

/// Export an admitted history into an explicit Jepsen-compatible event form.
/// r[impl chaoscontrol.semantic_history.reference_conformance]
pub fn export_jepsen_compatible(
    history: &SemanticHistory,
) -> SemanticResult<JepsenCompatibleHistory> {
    let admitted = admit_history(history)?;
    let mut events = Vec::new();
    for event in &history.events {
        match event {
            SemanticEvent::Invoke(invocation) => events.push(ReferenceEvent {
                index: invocation.event_index,
                process: invocation.process.clone(),
                event_type: ReferenceEventType::Invoke,
                function: operation_function(&invocation.input).to_string(),
                key: invocation.object_key.clone(),
                value: serde_json::to_value(&invocation.input).map_err(|error| {
                    SemanticError::new(
                        SemanticErrorClass::Schema,
                        format!("reference invocation conversion failed: {error}"),
                    )
                })?,
                attempt_id: invocation.attempt_id.clone(),
            }),
            SemanticEvent::Complete(completion) => {
                let operation = admitted
                    .operations
                    .iter()
                    .find(|operation| operation.attempt_id == completion.attempt_id)
                    .ok_or_else(|| {
                        SemanticError::new(
                            SemanticErrorClass::Pairing,
                            "admitted completion lost its paired operation",
                        )
                    })?;
                events.push(ReferenceEvent {
                    index: completion.event_index,
                    process: operation.process.clone(),
                    event_type: match completion.outcome {
                        CompletionOutcome::Ok => ReferenceEventType::Ok,
                        CompletionOutcome::Fail => ReferenceEventType::Fail,
                        CompletionOutcome::Info => ReferenceEventType::Info,
                    },
                    function: operation_function(&operation.input).to_string(),
                    key: operation.object_key.clone(),
                    value: serde_json::to_value(&completion.output).map_err(|error| {
                        SemanticError::new(
                            SemanticErrorClass::Schema,
                            format!("reference completion conversion failed: {error}"),
                        )
                    })?,
                    attempt_id: completion.attempt_id.clone(),
                });
            }
            SemanticEvent::Fault(_) | SemanticEvent::Lifecycle(_) => {}
        }
    }
    Ok(JepsenCompatibleHistory {
        schema_version: JEPSEN_ADAPTER_SCHEMA_VERSION,
        source_history_id: admitted.history_id,
        events,
    })
}

pub fn validate_jepsen_compatible(history: &JepsenCompatibleHistory) -> SemanticResult<()> {
    require(
        history.schema_version == JEPSEN_ADAPTER_SCHEMA_VERSION,
        SemanticErrorClass::Schema,
        "reference adapter schema version mismatch",
    )?;
    require(
        history.source_history_id.starts_with(BLAKE3_PREFIX),
        SemanticErrorClass::Identity,
        "reference history must bind a v2 BLAKE3 history identity",
    )?;
    for (expected, event) in history.events.iter().enumerate() {
        require(
            event.index >= expected,
            SemanticErrorClass::EventOrder,
            "reference events are not in source order",
        )?;
        require_nonempty(&event.attempt_id, "reference.attempt_id")?;
    }
    Ok(())
}

/// Import the operation portion of a Jepsen-compatible event stream.
///
/// The returned v2 history records the adapter source identity. Environment
/// events remain outside this adapter and must be joined through admitted
/// ChaosControl evidence. r[impl chaoscontrol.semantic_history.reference_conformance]
pub fn import_jepsen_compatible(
    reference: &JepsenCompatibleHistory,
    workload_profile: &str,
    source_artifact: &str,
    profile: CheckerProfile,
) -> SemanticResult<SemanticHistory> {
    validate_jepsen_compatible(reference)?;
    require_nonempty(workload_profile, "workload_profile")?;
    require_nonempty(source_artifact, "source_artifact")?;
    let mut ordered = reference.events.clone();
    ordered.sort_by_key(|event| event.index);
    let mut events = Vec::with_capacity(ordered.len());
    for (event_index, event) in ordered.iter().enumerate() {
        match event.event_type {
            ReferenceEventType::Invoke => {
                let input: OperationInput =
                    serde_json::from_value(event.value.clone()).map_err(|error| {
                        SemanticError::new(
                            SemanticErrorClass::Schema,
                            format!("reference invocation value is malformed: {error}"),
                        )
                    })?;
                require(
                    event.function == operation_function(&input),
                    SemanticErrorClass::Model,
                    "reference invocation function and typed input disagree",
                )?;
                events.push(SemanticEvent::Invoke(InvocationEvent {
                    event_index,
                    logical_operation_id: format!("logical:{}", event.attempt_id),
                    attempt_id: event.attempt_id.clone(),
                    retry_of_attempt: None,
                    process: event.process.clone(),
                    object_key: event.key.clone(),
                    input,
                    controller_time_ns: u64::try_from(event.index).map_err(|_| {
                        SemanticError::new(
                            SemanticErrorClass::Bounds,
                            "reference event index does not fit controller time",
                        )
                    })?,
                    source_artifact: source_artifact.to_string(),
                }));
            }
            ReferenceEventType::Ok | ReferenceEventType::Fail | ReferenceEventType::Info => {
                let output: Option<SemanticValue> = serde_json::from_value(event.value.clone())
                    .map_err(|error| {
                        SemanticError::new(
                            SemanticErrorClass::Schema,
                            format!("reference completion value is malformed: {error}"),
                        )
                    })?;
                events.push(SemanticEvent::Complete(CompletionEvent {
                    event_index,
                    attempt_id: event.attempt_id.clone(),
                    outcome: match event.event_type {
                        ReferenceEventType::Ok => CompletionOutcome::Ok,
                        ReferenceEventType::Fail => CompletionOutcome::Fail,
                        ReferenceEventType::Info => CompletionOutcome::Info,
                        ReferenceEventType::Invoke => {
                            unreachable!("invoke branch is handled before completion conversion")
                        }
                    },
                    output,
                    controller_time_ns: u64::try_from(event.index).map_err(|_| {
                        SemanticError::new(
                            SemanticErrorClass::Bounds,
                            "reference event index does not fit controller time",
                        )
                    })?,
                    finalization: None,
                }));
            }
        }
    }
    let invocations = events
        .iter()
        .filter(|event| matches!(event, SemanticEvent::Invoke(_)))
        .count();
    let completions = events
        .iter()
        .filter(|event| matches!(event, SemanticEvent::Complete(_)))
        .count();
    let imported = SemanticHistory {
        schema_version: SEMANTIC_HISTORY_SCHEMA_VERSION,
        workload_profile: workload_profile.to_string(),
        source_artifact: source_artifact.to_string(),
        profile,
        completeness: CompletenessAccounting {
            invocations,
            completions,
            pending: invocations.saturating_sub(completions),
            applied_faults: 0,
            observed_faults: 0,
        },
        events,
    };
    admit_history(&imported)?;
    Ok(imported)
}

fn operation_function(input: &OperationInput) -> &'static str {
    match input {
        OperationInput::Read => "read",
        OperationInput::Write { .. } => "write",
        OperationInput::CompareAndSwap { .. } => "compare-and-swap",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceVerdict {
    pub tool_id: String,
    pub history_id: String,
    pub verdict: LinearizabilityVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceStatus {
    Agreement,
    Disagreement,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConformanceReport {
    pub status: ConformanceStatus,
    pub history_id: String,
    pub native_tool_id: String,
    pub reference_tool_id: String,
    pub native_verdict: LinearizabilityVerdict,
    pub reference_verdict: LinearizabilityVerdict,
    pub promotion_blocked: bool,
}

pub fn classify_reference_conformance(
    native: &SemanticCheckReport,
    reference: &ReferenceVerdict,
) -> SemanticResult<ConformanceReport> {
    require(
        native.history_id == reference.history_id,
        SemanticErrorClass::Identity,
        "native and reference reports bind different histories",
    )?;
    require(
        reference.tool_id == REFERENCE_TOOL_ID,
        SemanticErrorClass::ReferenceDisagreement,
        format!("reference tool must be pinned to {REFERENCE_TOOL_ID}"),
    )?;
    let agreement = native.verdict == reference.verdict;
    Ok(ConformanceReport {
        status: if agreement {
            ConformanceStatus::Agreement
        } else {
            ConformanceStatus::Disagreement
        },
        history_id: native.history_id.clone(),
        native_tool_id: native.checker_id.clone(),
        reference_tool_id: reference.tool_id.clone(),
        native_verdict: native.verdict,
        reference_verdict: reference.verdict,
        promotion_blocked: !agreement,
    })
}

pub fn bind_reference_conformance(
    report: &SemanticCheckReport,
    conformance: &ConformanceReport,
) -> SemanticResult<SemanticCheckReport> {
    require(
        report.history_id == conformance.history_id
            && report.checker_id == conformance.native_tool_id
            && report.verdict == conformance.native_verdict,
        SemanticErrorClass::Identity,
        "reference conformance does not bind the semantic report",
    )?;
    let mut bound = report.clone();
    bound.reference_status = match conformance.status {
        ConformanceStatus::Agreement => ReferenceStatus::Agreement,
        ConformanceStatus::Disagreement => ReferenceStatus::Disagreement,
    };
    Ok(bound)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineKind {
    Invocation,
    Completion,
    Lifecycle,
    FaultSelected,
    FaultApplied,
    FaultObserved,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimelineEntry {
    pub event_index: usize,
    pub controller_time_ns: u64,
    pub kind: TimelineKind,
    pub identity: String,
    pub witness_member: bool,
    pub latency_ns: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticTimeline {
    pub history_id: String,
    pub verdict: LinearizabilityVerdict,
    pub entries: Vec<TimelineEntry>,
    pub non_causation: String,
}

/// Project operations, lifecycle, admitted fault effects, latency, and witness
/// membership from one pure source. r[impl chaoscontrol.semantic_history.timeline]
pub fn semantic_timeline(
    history: &SemanticHistory,
    report: &SemanticCheckReport,
) -> SemanticResult<SemanticTimeline> {
    validate_semantic_report(report, history)?;
    let admitted = admit_history(history)?;
    let witness: std::collections::BTreeSet<&str> = report
        .witness
        .iter()
        .map(|step| step.attempt_id.as_str())
        .collect();
    let mut entries = Vec::new();
    for event in &history.events {
        match event {
            SemanticEvent::Invoke(invocation) => entries.push(TimelineEntry {
                event_index: invocation.event_index,
                controller_time_ns: invocation.controller_time_ns,
                kind: TimelineKind::Invocation,
                identity: invocation.attempt_id.clone(),
                witness_member: witness.contains(invocation.attempt_id.as_str()),
                latency_ns: None,
            }),
            SemanticEvent::Complete(completion) => {
                let operation = admitted
                    .operations
                    .iter()
                    .find(|operation| operation.attempt_id == completion.attempt_id)
                    .ok_or_else(|| {
                        SemanticError::new(
                            SemanticErrorClass::Pairing,
                            "timeline completion lost its admitted invocation",
                        )
                    })?;
                entries.push(TimelineEntry {
                    event_index: completion.event_index,
                    controller_time_ns: completion.controller_time_ns,
                    kind: TimelineKind::Completion,
                    identity: completion.attempt_id.clone(),
                    witness_member: witness.contains(completion.attempt_id.as_str()),
                    latency_ns: Some(
                        completion
                            .controller_time_ns
                            .saturating_sub(operation.invocation_time_ns),
                    ),
                });
            }
            SemanticEvent::Fault(fault) => entries.push(TimelineEntry {
                event_index: fault.event_index,
                controller_time_ns: fault.controller_time_ns,
                kind: match fault.phase {
                    FaultEffectPhase::Selected => TimelineKind::FaultSelected,
                    FaultEffectPhase::Applied => TimelineKind::FaultApplied,
                    FaultEffectPhase::Observed => TimelineKind::FaultObserved,
                },
                identity: fault.fault_attempt_id.clone(),
                witness_member: false,
                latency_ns: None,
            }),
            SemanticEvent::Lifecycle(lifecycle) => entries.push(TimelineEntry {
                event_index: lifecycle.event_index,
                controller_time_ns: lifecycle.controller_time_ns,
                kind: TimelineKind::Lifecycle,
                identity: lifecycle.phase.clone(),
                witness_member: false,
                latency_ns: None,
            }),
        }
    }
    Ok(SemanticTimeline {
        history_id: report.history_id.clone(),
        verdict: report.verdict,
        entries,
        non_causation: "temporal overlap is not evidence of causation".to_string(),
    })
}

pub fn render_timeline_text(timeline: &SemanticTimeline) -> String {
    let mut output = format!(
        "history={} verdict={:?}\n",
        timeline.history_id, timeline.verdict
    );
    for entry in &timeline.entries {
        output.push_str(&format!(
            "{} {} {:?} {} witness={} latency_ns={}\n",
            entry.event_index,
            entry.controller_time_ns,
            entry.kind,
            entry.identity,
            entry.witness_member,
            entry
                .latency_ns
                .map_or_else(|| "-".to_string(), |value| value.to_string())
        ));
    }
    output.push_str(&timeline.non_causation);
    output.push('\n');
    output
}

pub fn render_timeline_json(timeline: &SemanticTimeline) -> SemanticResult<String> {
    serde_json::to_string_pretty(timeline).map_err(|error| {
        SemanticError::new(
            SemanticErrorClass::Schema,
            format!("timeline JSON rendering failed: {error}"),
        )
    })
}

pub fn render_timeline_html(timeline: &SemanticTimeline) -> String {
    let mut output = format!(
        "<section data-history=\"{}\"><h1>Semantic timeline</h1><p>verdict={:?}</p><ol>",
        escape_html(&timeline.history_id),
        timeline.verdict
    );
    for entry in &timeline.entries {
        output.push_str(&format!(
            "<li data-event=\"{}\" data-witness=\"{}\">{} {:?} {}</li>",
            entry.event_index,
            entry.witness_member,
            entry.controller_time_ns,
            entry.kind,
            escape_html(&entry.identity)
        ));
    }
    output.push_str(&format!(
        "</ol><p>{}</p></section>",
        escape_html(&timeline.non_causation)
    ));
    output
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn invocation(
    event_index: usize,
    attempt_id: &str,
    logical_operation_id: &str,
    process: &str,
    object_key: &str,
    input: OperationInput,
    controller_time_ns: u64,
) -> SemanticEvent {
    SemanticEvent::Invoke(InvocationEvent {
        event_index,
        logical_operation_id: logical_operation_id.to_string(),
        attempt_id: attempt_id.to_string(),
        retry_of_attempt: None,
        process: process.to_string(),
        object_key: object_key.to_string(),
        input,
        controller_time_ns,
        source_artifact: format!("{BLAKE3_PREFIX}fixture-source"),
    })
}

fn completion(
    event_index: usize,
    attempt_id: &str,
    outcome: CompletionOutcome,
    output: Option<SemanticValue>,
    controller_time_ns: u64,
) -> SemanticEvent {
    SemanticEvent::Complete(CompletionEvent {
        event_index,
        attempt_id: attempt_id.to_string(),
        outcome,
        output,
        controller_time_ns,
        finalization: None,
    })
}

fn fixture_profile(model: &str) -> CheckerProfile {
    CheckerProfile {
        profile_id: "semantic-fixture-profile".to_string(),
        model: model.to_string(),
        independent_key_decomposition: false,
        bounds: SearchBounds::default(),
    }
}

fn fixture_history(events: Vec<SemanticEvent>, model: &str) -> SemanticHistory {
    let invocations = events
        .iter()
        .filter(|event| matches!(event, SemanticEvent::Invoke(_)))
        .count();
    let completions = events
        .iter()
        .filter(|event| matches!(event, SemanticEvent::Complete(_)))
        .count();
    SemanticHistory {
        schema_version: SEMANTIC_HISTORY_SCHEMA_VERSION,
        workload_profile: "semantic-fixture".to_string(),
        source_artifact: format!("{BLAKE3_PREFIX}fixture-source"),
        profile: fixture_profile(model),
        completeness: CompletenessAccounting {
            invocations,
            completions,
            pending: invocations.saturating_sub(completions),
            applied_faults: 0,
            observed_faults: 0,
        },
        events,
    }
}

fn valid_concurrent_fixture() -> SemanticHistory {
    const INVOKE_FIRST_NS: u64 = 10;
    const INVOKE_SECOND_NS: u64 = 20;
    const COMPLETE_SECOND_NS: u64 = 30;
    const COMPLETE_FIRST_NS: u64 = 40;
    const INVOKE_READ_NS: u64 = 50;
    const COMPLETE_READ_NS: u64 = 60;
    const FIRST_VALUE: i64 = 1;
    const SECOND_VALUE: i64 = 2;
    fixture_history(
        vec![
            invocation(
                0,
                "attempt-write-first",
                "logical-write-first",
                "client-a",
                "register-a",
                OperationInput::Write {
                    value: SemanticValue::Integer(FIRST_VALUE),
                },
                INVOKE_FIRST_NS,
            ),
            invocation(
                1,
                "attempt-write-second",
                "logical-write-second",
                "client-b",
                "register-a",
                OperationInput::Write {
                    value: SemanticValue::Integer(SECOND_VALUE),
                },
                INVOKE_SECOND_NS,
            ),
            completion(
                2,
                "attempt-write-second",
                CompletionOutcome::Ok,
                None,
                COMPLETE_SECOND_NS,
            ),
            completion(
                3,
                "attempt-write-first",
                CompletionOutcome::Ok,
                None,
                COMPLETE_FIRST_NS,
            ),
            invocation(
                4,
                "attempt-read",
                "logical-read",
                "client-c",
                "register-a",
                OperationInput::Read,
                INVOKE_READ_NS,
            ),
            completion(
                5,
                "attempt-read",
                CompletionOutcome::Ok,
                Some(SemanticValue::Integer(FIRST_VALUE)),
                COMPLETE_READ_NS,
            ),
        ],
        "register",
    )
}

fn invalid_stale_read_fixture() -> SemanticHistory {
    const INVOKE_WRITE_NS: u64 = 10;
    const COMPLETE_WRITE_NS: u64 = 20;
    const INVOKE_READ_NS: u64 = 30;
    const COMPLETE_READ_NS: u64 = 40;
    const WRITTEN_VALUE: i64 = 1;
    fixture_history(
        vec![
            invocation(
                0,
                "attempt-write",
                "logical-write",
                "client-a",
                "register-a",
                OperationInput::Write {
                    value: SemanticValue::Integer(WRITTEN_VALUE),
                },
                INVOKE_WRITE_NS,
            ),
            completion(
                1,
                "attempt-write",
                CompletionOutcome::Ok,
                None,
                COMPLETE_WRITE_NS,
            ),
            invocation(
                2,
                "attempt-read",
                "logical-read",
                "client-b",
                "register-a",
                OperationInput::Read,
                INVOKE_READ_NS,
            ),
            completion(
                3,
                "attempt-read",
                CompletionOutcome::Ok,
                Some(SemanticValue::Null),
                COMPLETE_READ_NS,
            ),
        ],
        "register",
    )
}

/// Run a deterministic positive and negative conformance corpus without I/O.
/// r[verify chaoscontrol.semantic_history.validation]
pub fn semantic_history_selftest() -> SemanticResult<String> {
    let valid_history = valid_concurrent_fixture();
    let valid = check_linearizability(&valid_history)?;
    require(
        valid.verdict == LinearizabilityVerdict::Valid,
        SemanticErrorClass::Model,
        "valid concurrent fixture was not linearizable",
    )?;
    let invalid = check_linearizability(&invalid_stale_read_fixture())?;
    require(
        invalid.verdict == LinearizabilityVerdict::Invalid,
        SemanticErrorClass::Model,
        "stale-read fixture was not rejected",
    )?;
    let adapter = export_jepsen_compatible(&valid_history)?;
    validate_jepsen_compatible(&adapter)?;
    let agreement = classify_reference_conformance(
        &valid,
        &ReferenceVerdict {
            tool_id: REFERENCE_TOOL_ID.to_string(),
            history_id: valid.history_id.clone(),
            verdict: LinearizabilityVerdict::Valid,
        },
    )?;
    require(
        agreement.status == ConformanceStatus::Agreement && !agreement.promotion_blocked,
        SemanticErrorClass::ReferenceDisagreement,
        "reference agreement fixture did not agree",
    )?;
    let disagreement = classify_reference_conformance(
        &valid,
        &ReferenceVerdict {
            tool_id: REFERENCE_TOOL_ID.to_string(),
            history_id: valid.history_id.clone(),
            verdict: LinearizabilityVerdict::Invalid,
        },
    )?;
    require(
        disagreement.status == ConformanceStatus::Disagreement && disagreement.promotion_blocked,
        SemanticErrorClass::ReferenceDisagreement,
        "reference disagreement did not block promotion",
    )?;
    let bound_disagreement = bind_reference_conformance(&valid, &disagreement)?;
    require(
        bound_disagreement.reference_status == ReferenceStatus::Disagreement,
        SemanticErrorClass::ReferenceDisagreement,
        "semantic report did not retain reference disagreement",
    )?;
    let timeline = semantic_timeline(&valid_history, &valid)?;
    require(
        render_timeline_text(&timeline).contains("attempt-read")
            && render_timeline_json(&timeline)?.contains("attempt-read")
            && render_timeline_html(&timeline).contains("attempt-read"),
        SemanticErrorClass::Schema,
        "timeline renderers lost shared semantic entries",
    )?;
    Ok(valid.history_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_completion_order_is_not_forced() {
        let history = valid_concurrent_fixture();
        let report = check_linearizability(&history).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Valid);
        assert_eq!(report.witness.len(), history.completeness.invocations);
        assert_eq!(report.witness[0].attempt_id, "attempt-write-second");
    }

    #[test]
    fn stale_read_is_invalid_and_reduced() {
        let report = check_linearizability(&invalid_stale_read_fixture()).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Invalid);
        assert!(report.invalid.is_some());
        assert_eq!(report.reduction.status, ReductionStatus::LocallyReduced);
        assert!(!report.reduction.retained_attempt_ids.is_empty());
    }

    #[test]
    fn orphan_completion_and_duplicate_event_index_fail_closed() {
        const COMPLETION_NS: u64 = 10;
        let orphan = fixture_history(
            vec![completion(
                0,
                "missing-attempt",
                CompletionOutcome::Ok,
                None,
                COMPLETION_NS,
            )],
            "register",
        );
        assert_eq!(
            admit_history(&orphan).unwrap_err().class,
            SemanticErrorClass::Pairing
        );

        let mut wrong_index = valid_concurrent_fixture();
        if let SemanticEvent::Invoke(event) = &mut wrong_index.events[1] {
            event.event_index = 0;
        }
        assert_eq!(
            admit_history(&wrong_index).unwrap_err().class,
            SemanticErrorClass::EventOrder
        );
    }

    #[test]
    fn retry_must_preserve_logical_and_attempt_identity() {
        const FIRST_INVOKE_NS: u64 = 10;
        const RETRY_INVOKE_NS: u64 = 20;
        const VALUE: i64 = 7;
        let mut retry = fixture_history(
            vec![
                invocation(
                    0,
                    "attempt-first",
                    "logical-first",
                    "client-a",
                    "register-a",
                    OperationInput::Write {
                        value: SemanticValue::Integer(VALUE),
                    },
                    FIRST_INVOKE_NS,
                ),
                invocation(
                    1,
                    "attempt-retry",
                    "logical-changed",
                    "client-a",
                    "register-a",
                    OperationInput::Write {
                        value: SemanticValue::Integer(VALUE),
                    },
                    RETRY_INVOKE_NS,
                ),
            ],
            "register",
        );
        if let SemanticEvent::Invoke(event) = &mut retry.events[1] {
            event.retry_of_attempt = Some("attempt-first".to_string());
        }
        assert_eq!(
            admit_history(&retry).unwrap_err().class,
            SemanticErrorClass::Retry
        );
    }

    #[test]
    fn identity_is_transport_order_independent_and_content_sensitive() {
        let history = valid_concurrent_fixture();
        let identity = semantic_history_identity(&history).unwrap();
        let value = serde_json::to_value(&history).unwrap();
        let reparsed: SemanticHistory = serde_json::from_value(value).unwrap();
        assert_eq!(semantic_history_identity(&reparsed).unwrap(), identity);

        let mut changed = history;
        if let SemanticEvent::Complete(event) = &mut changed.events[2] {
            event.outcome = CompletionOutcome::Info;
        }
        assert_ne!(semantic_history_identity(&changed).unwrap(), identity);
    }

    #[test]
    fn info_outcome_can_take_effect_or_be_omitted() {
        const INVOKE_INFO_NS: u64 = 10;
        const COMPLETE_INFO_NS: u64 = 20;
        const INVOKE_READ_NS: u64 = 30;
        const COMPLETE_READ_NS: u64 = 40;
        const VALUE: i64 = 9;
        let history = fixture_history(
            vec![
                invocation(
                    0,
                    "attempt-info",
                    "logical-info",
                    "client-a",
                    "register-a",
                    OperationInput::Write {
                        value: SemanticValue::Integer(VALUE),
                    },
                    INVOKE_INFO_NS,
                ),
                completion(
                    1,
                    "attempt-info",
                    CompletionOutcome::Info,
                    None,
                    COMPLETE_INFO_NS,
                ),
                invocation(
                    2,
                    "attempt-read",
                    "logical-read",
                    "client-b",
                    "register-a",
                    OperationInput::Read,
                    INVOKE_READ_NS,
                ),
                completion(
                    3,
                    "attempt-read",
                    CompletionOutcome::Ok,
                    Some(SemanticValue::Integer(VALUE)),
                    COMPLETE_READ_NS,
                ),
            ],
            "register",
        );
        let report = check_linearizability(&history).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Valid);
        assert!(report
            .witness
            .iter()
            .any(|step| step.attempt_id == "attempt-info" && step.took_effect));
    }

    #[test]
    fn pending_and_bound_exhaustion_are_unknown() {
        const INVOKE_NS: u64 = 10;
        let pending = fixture_history(
            vec![invocation(
                0,
                "attempt-pending",
                "logical-pending",
                "client-a",
                "register-a",
                OperationInput::Read,
                INVOKE_NS,
            )],
            "register",
        );
        let report = check_linearizability(&pending).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Unknown);
        assert_eq!(
            report.unknown.unwrap().class,
            UnknownClass::PendingOperations
        );

        let mut bounded = valid_concurrent_fixture();
        bounded.profile.bounds.max_states = 1;
        let report = check_linearizability(&bounded).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Unknown);
        assert_eq!(report.unknown.unwrap().class, UnknownClass::MaxStates);
    }

    #[test]
    fn compare_and_swap_model_checks_expected_state() {
        const INVOKE_WRITE_NS: u64 = 10;
        const COMPLETE_WRITE_NS: u64 = 20;
        const INVOKE_CAS_NS: u64 = 30;
        const COMPLETE_CAS_NS: u64 = 40;
        const OLD_VALUE: i64 = 1;
        const NEW_VALUE: i64 = 2;
        let history = fixture_history(
            vec![
                invocation(
                    0,
                    "attempt-write",
                    "logical-write",
                    "client-a",
                    "register-a",
                    OperationInput::Write {
                        value: SemanticValue::Integer(OLD_VALUE),
                    },
                    INVOKE_WRITE_NS,
                ),
                completion(
                    1,
                    "attempt-write",
                    CompletionOutcome::Ok,
                    None,
                    COMPLETE_WRITE_NS,
                ),
                invocation(
                    2,
                    "attempt-cas",
                    "logical-cas",
                    "client-b",
                    "register-a",
                    OperationInput::CompareAndSwap {
                        expected: SemanticValue::Integer(OLD_VALUE),
                        replacement: SemanticValue::Integer(NEW_VALUE),
                    },
                    INVOKE_CAS_NS,
                ),
                completion(
                    3,
                    "attempt-cas",
                    CompletionOutcome::Ok,
                    Some(SemanticValue::Bool(true)),
                    COMPLETE_CAS_NS,
                ),
            ],
            "compare-and-swap-register",
        );
        assert_eq!(
            check_linearizability(&history).unwrap().verdict,
            LinearizabilityVerdict::Valid
        );
    }

    #[test]
    fn decomposition_requires_model_isolation() {
        let mut history = valid_concurrent_fixture();
        history.profile.model = "cross-key-register".to_string();
        history.profile.independent_key_decomposition = true;
        let report = check_linearizability(&history).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Unknown);
        assert_eq!(
            report.unknown.unwrap().class,
            UnknownClass::DecompositionNotAdmitted
        );
    }

    #[test]
    fn legacy_reports_are_readable_but_not_promotable() {
        let value = serde_json::json!({"schema_version": 1, "verdict": "passed"});
        let legacy = read_legacy_history_value(&value).unwrap();
        assert!(legacy.history_sha256.starts_with(SHA256_PREFIX));
        assert_eq!(
            reject_legacy_promotion(&legacy).unwrap_err().class,
            SemanticErrorClass::LegacyPromotion
        );
    }

    #[test]
    fn reference_disagreement_is_retained_and_blocks_promotion() {
        let history = valid_concurrent_fixture();
        let report = check_linearizability(&history).unwrap();
        let conformance = classify_reference_conformance(
            &report,
            &ReferenceVerdict {
                tool_id: REFERENCE_TOOL_ID.to_string(),
                history_id: report.history_id.clone(),
                verdict: LinearizabilityVerdict::Invalid,
            },
        )
        .unwrap();
        assert_eq!(conformance.status, ConformanceStatus::Disagreement);
        assert!(conformance.promotion_blocked);
    }

    #[test]
    fn jepsen_adapter_exports_imports_and_rechecks_operations() {
        let history = valid_concurrent_fixture();
        let reference = export_jepsen_compatible(&history).unwrap();
        let imported = import_jepsen_compatible(
            &reference,
            "imported-semantic-fixture",
            "blake3:imported-reference",
            fixture_profile("register"),
        )
        .unwrap();
        assert_eq!(
            check_linearizability(&imported).unwrap().verdict,
            LinearizabilityVerdict::Valid
        );
        assert_eq!(
            imported.completeness.invocations,
            history.completeness.invocations
        );
    }

    #[test]
    fn unsupported_model_and_operation_limit_are_unknown() {
        let mut unsupported = valid_concurrent_fixture();
        unsupported.profile.model = "unsupported-transaction-model".to_string();
        let report = check_linearizability(&unsupported).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Unknown);
        assert_eq!(
            report.unknown.unwrap().class,
            UnknownClass::UnsupportedModel
        );

        let mut bounded = valid_concurrent_fixture();
        bounded.profile.bounds.max_operations = 1;
        let report = check_linearizability(&bounded).unwrap();
        assert_eq!(report.verdict, LinearizabilityVerdict::Unknown);
        assert_eq!(report.unknown.unwrap().class, UnknownClass::MaxOperations);
    }

    #[test]
    fn malformed_value_and_completeness_mismatch_fail_closed() {
        let malformed = serde_json::json!({
            "schema_version": SEMANTIC_HISTORY_SCHEMA_VERSION,
            "workload_profile": "malformed",
            "source_artifact": "blake3:source",
            "profile": fixture_profile("register"),
            "completeness": {
                "invocations": 1,
                "completions": 0,
                "pending": 1,
                "applied_faults": 0,
                "observed_faults": 0
            },
            "events": [{
                "kind": "invoke",
                "event_index": 0,
                "logical_operation_id": "logical-a",
                "attempt_id": "attempt-a",
                "retry_of_attempt": null,
                "process": "client-a",
                "object_key": "register-a",
                "input": {"function": "write", "value": {"bad": true}},
                "controller_time_ns": 0,
                "source_artifact": "blake3:source"
            }]
        });
        assert!(serde_json::from_value::<SemanticHistory>(malformed).is_err());

        let mut mismatch = valid_concurrent_fixture();
        mismatch.completeness.completions = 0;
        assert_eq!(
            admit_history(&mismatch).unwrap_err().class,
            SemanticErrorClass::Completeness
        );
    }

    #[test]
    fn fault_effect_timeline_requires_an_admitted_effect_record() {
        const FAULT_NS: u64 = 70;
        let mut missing_record = valid_concurrent_fixture();
        missing_record.events.push(SemanticEvent::Fault(FaultEvent {
            event_index: missing_record.events.len(),
            fault_attempt_id: "fault-a".to_string(),
            phase: FaultEffectPhase::Applied,
            effect_record_ref: None,
            controller_time_ns: FAULT_NS,
        }));
        missing_record.completeness.applied_faults = 1;
        assert_eq!(
            admit_history(&missing_record).unwrap_err().class,
            SemanticErrorClass::Identity
        );

        let mut admitted = valid_concurrent_fixture();
        admitted.events.push(SemanticEvent::Fault(FaultEvent {
            event_index: admitted.events.len(),
            fault_attempt_id: "fault-a".to_string(),
            phase: FaultEffectPhase::Observed,
            effect_record_ref: Some("blake3:accepted-fault-effect".to_string()),
            controller_time_ns: FAULT_NS,
        }));
        admitted.completeness.observed_faults = 1;
        let report = check_linearizability(&admitted).unwrap();
        let timeline = semantic_timeline(&admitted, &report).unwrap();
        assert!(timeline
            .entries
            .iter()
            .any(|entry| entry.kind == TimelineKind::FaultObserved));
    }

    #[test]
    fn selected_fault_is_not_rendered_as_observed_effect() {
        const FAULT_NS: u64 = 70;
        let mut history = valid_concurrent_fixture();
        history.events.push(SemanticEvent::Fault(FaultEvent {
            event_index: history.events.len(),
            fault_attempt_id: "fault-a".to_string(),
            phase: FaultEffectPhase::Selected,
            effect_record_ref: None,
            controller_time_ns: FAULT_NS,
        }));
        let report = check_linearizability(&history).unwrap();
        let timeline = semantic_timeline(&history, &report).unwrap();
        assert!(timeline
            .entries
            .iter()
            .any(|entry| entry.kind == TimelineKind::FaultSelected));
        assert!(!timeline
            .entries
            .iter()
            .any(|entry| entry.kind == TimelineKind::FaultObserved));
    }

    #[test]
    fn overclaim_and_digest_drift_fail_closed() {
        let history = valid_concurrent_fixture();
        let mut report = check_linearizability(&history).unwrap();
        report.scope = "system correct for exhaustive production use".to_string();
        assert_eq!(
            validate_semantic_report(&report, &history)
                .unwrap_err()
                .class,
            SemanticErrorClass::ClaimBoundary
        );

        let mut report = check_linearizability(&history).unwrap();
        report.history_id = format!("{BLAKE3_PREFIX}stale");
        assert_eq!(
            validate_semantic_report(&report, &history)
                .unwrap_err()
                .class,
            SemanticErrorClass::Identity
        );
    }

    #[test]
    fn complete_selftest_covers_positive_and_negative_oracles() {
        assert!(semantic_history_selftest()
            .unwrap()
            .starts_with(BLAKE3_PREFIX));
    }
}
