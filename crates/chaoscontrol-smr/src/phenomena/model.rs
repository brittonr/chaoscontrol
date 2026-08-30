use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const HISTORY_SCHEMA_VERSION: u32 = 1;
pub const MAX_HISTORY_OPERATIONS: usize = 4_096;
pub const MAX_HISTORY_DEPENDENCIES: usize = 16_384;
pub const MAX_HISTORY_GAPS: usize = 1_024;
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 1_024;
const BLAKE3_HEX_BYTES: usize = 64;
const BLAKE3_PREFIX: &str = "blake3:";
const HISTORY_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.phenomena.history.v1\0";
const OPERATION_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.phenomena.operation.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhenomenaHistory {
    pub schema_version: u32,
    pub history_id: String,
    pub workload: String,
    pub source_blake3: String,
    pub operations: Vec<HistoryOperation>,
    pub gaps: Vec<ObservationGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryOperation {
    pub operation_id: String,
    pub process: String,
    pub sequence: u64,
    pub status: OperationStatus,
    pub kind: OperationKind,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Committed,
    Aborted,
    Intermediate,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationKind {
    Read {
        key: String,
        observation: ReadObservation,
    },
    Write {
        key: String,
        version: u64,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadObservation {
    Initial,
    Write {
        operation_id: String,
        version: u64,
        value: String,
    },
    Unattributed {
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub predecessor: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    WriteRead,
    WriteWrite,
    ReadWrite,
    Realtime,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationGap {
    pub left_operation: String,
    pub right_operation: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhenomenaError {
    pub class: &'static str,
    pub operation_id: Option<String>,
    pub detail: String,
}

impl PhenomenaError {
    pub(crate) fn new(class: &'static str, detail: impl Into<String>) -> Self {
        Self {
            class,
            operation_id: None,
            detail: detail.into(),
        }
    }

    fn operation(class: &'static str, operation_id: &str, detail: impl Into<String>) -> Self {
        Self {
            class,
            operation_id: Some(operation_id.to_string()),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for PhenomenaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.operation_id.as_deref() {
            Some(operation_id) => write!(
                formatter,
                "{} at operation {}: {}",
                self.class, operation_id, self.detail
            ),
            None => write!(formatter, "{}: {}", self.class, self.detail),
        }
    }
}

impl std::error::Error for PhenomenaError {}

pub fn bind_history(
    workload: impl Into<String>,
    source_blake3: impl Into<String>,
    mut operations: Vec<HistoryOperation>,
    mut gaps: Vec<ObservationGap>,
) -> Result<PhenomenaHistory, PhenomenaError> {
    canonicalize(&mut operations, &mut gaps);
    let mut history = PhenomenaHistory {
        schema_version: HISTORY_SCHEMA_VERSION,
        history_id: String::new(),
        workload: workload.into(),
        source_blake3: source_blake3.into(),
        operations,
        gaps,
    };
    validate_material(&history)?;
    history.history_id = history_identity_unchecked(&history)?;
    validate_history(&history)?;
    Ok(history)
}

pub fn validate_history(history: &PhenomenaHistory) -> Result<(), PhenomenaError> {
    validate_material(history)?;
    let expected = history_identity_unchecked(history)?;
    if history.history_id != expected {
        return Err(PhenomenaError::new(
            "history-identity",
            format!(
                "claimed history identity {:?} does not match {:?}",
                history.history_id, expected
            ),
        ));
    }
    Ok(())
}

pub fn history_identity(history: &PhenomenaHistory) -> Result<String, PhenomenaError> {
    validate_material(history)?;
    history_identity_unchecked(history)
}

pub fn operation_identity(operation: &HistoryOperation) -> Result<String, PhenomenaError> {
    validate_operation_shape(operation)?;
    let mut canonical = operation.clone();
    canonical.dependencies.sort();
    let bytes = serde_json::to_vec(&canonical).map_err(|error| {
        PhenomenaError::operation(
            "operation-serialization",
            &operation.operation_id,
            error.to_string(),
        )
    })?;
    Ok(domain_hash(OPERATION_IDENTITY_DOMAIN, &bytes))
}

fn history_identity_unchecked(history: &PhenomenaHistory) -> Result<String, PhenomenaError> {
    #[derive(Serialize)]
    struct Material<'a> {
        schema_version: u32,
        workload: &'a str,
        source_blake3: &'a str,
        operations: &'a [HistoryOperation],
        gaps: &'a [ObservationGap],
    }

    let mut operations = history.operations.clone();
    let mut gaps = history.gaps.clone();
    canonicalize(&mut operations, &mut gaps);
    let material = Material {
        schema_version: history.schema_version,
        workload: &history.workload,
        source_blake3: &history.source_blake3,
        operations: &operations,
        gaps: &gaps,
    };
    let bytes = serde_json::to_vec(&material)
        .map_err(|error| PhenomenaError::new("history-serialization", error.to_string()))?;
    Ok(domain_hash(HISTORY_IDENTITY_DOMAIN, &bytes))
}

fn canonicalize(operations: &mut [HistoryOperation], gaps: &mut [ObservationGap]) {
    for operation in operations.iter_mut() {
        operation.dependencies.sort();
    }
    operations.sort_by(|left, right| {
        (left.sequence, left.operation_id.as_str())
            .cmp(&(right.sequence, right.operation_id.as_str()))
    });
    gaps.sort();
}

fn validate_material(history: &PhenomenaHistory) -> Result<(), PhenomenaError> {
    if history.schema_version != HISTORY_SCHEMA_VERSION {
        return Err(PhenomenaError::new(
            "history-schema",
            format!("unsupported schema version {}", history.schema_version),
        ));
    }
    validate_identifier("workload", &history.workload)?;
    validate_digest("source_blake3", &history.source_blake3)?;
    if history.operations.is_empty() || history.operations.len() > MAX_HISTORY_OPERATIONS {
        return Err(PhenomenaError::new(
            "history-operation-bound",
            "operation count is empty or exceeds the supported bound",
        ));
    }
    if history.gaps.len() > MAX_HISTORY_GAPS {
        return Err(PhenomenaError::new(
            "history-gap-bound",
            "observation gap count exceeds the supported bound",
        ));
    }
    let mut canonical_operations = history.operations.clone();
    let mut canonical_gaps = history.gaps.clone();
    canonicalize(&mut canonical_operations, &mut canonical_gaps);
    if canonical_operations != history.operations || canonical_gaps != history.gaps {
        return Err(PhenomenaError::new(
            "history-order",
            "operations, dependencies, or gaps are not in canonical order",
        ));
    }

    let mut ids = BTreeMap::new();
    let mut sequences = BTreeSet::new();
    let mut dependency_count = 0_usize;
    for operation in &history.operations {
        validate_operation_shape(operation)?;
        if ids
            .insert(operation.operation_id.as_str(), operation)
            .is_some()
        {
            return Err(PhenomenaError::operation(
                "operation-identity",
                &operation.operation_id,
                "duplicate operation identity",
            ));
        }
        if !sequences.insert(operation.sequence) {
            return Err(PhenomenaError::operation(
                "operation-sequence",
                &operation.operation_id,
                "duplicate operation sequence",
            ));
        }
        dependency_count = dependency_count
            .checked_add(operation.dependencies.len())
            .ok_or_else(|| {
                PhenomenaError::new("history-dependency-bound", "dependency count overflow")
            })?;
    }
    if dependency_count > MAX_HISTORY_DEPENDENCIES {
        return Err(PhenomenaError::new(
            "history-dependency-bound",
            "dependency count exceeds the supported bound",
        ));
    }

    for operation in &history.operations {
        let mut dependencies = BTreeSet::new();
        for dependency in &operation.dependencies {
            if dependency.predecessor == operation.operation_id {
                return Err(PhenomenaError::operation(
                    "operation-dependency",
                    &operation.operation_id,
                    "self dependency is not admitted",
                ));
            }
            if !ids.contains_key(dependency.predecessor.as_str()) {
                return Err(PhenomenaError::operation(
                    "operation-dependency",
                    &operation.operation_id,
                    format!(
                        "dependency predecessor {:?} is unknown",
                        dependency.predecessor
                    ),
                ));
            }
            if !dependencies.insert(dependency) {
                return Err(PhenomenaError::operation(
                    "operation-dependency",
                    &operation.operation_id,
                    "duplicate dependency",
                ));
            }
        }
        validate_read_observation(operation, &ids)?;
    }

    for gap in &history.gaps {
        validate_text("gap.reason", &gap.reason, MAX_VALUE_BYTES)?;
        if gap.left_operation == gap.right_operation
            || !ids.contains_key(gap.left_operation.as_str())
            || !ids.contains_key(gap.right_operation.as_str())
        {
            return Err(PhenomenaError::new(
                "history-gap",
                "gap endpoints must name two distinct admitted operations",
            ));
        }
    }
    Ok(())
}

fn validate_operation_shape(operation: &HistoryOperation) -> Result<(), PhenomenaError> {
    validate_identifier("operation_id", &operation.operation_id).map_err(|error| {
        PhenomenaError::operation(error.class, &operation.operation_id, error.detail)
    })?;
    validate_identifier("process", &operation.process).map_err(|error| {
        PhenomenaError::operation(error.class, &operation.operation_id, error.detail)
    })?;
    match &operation.kind {
        OperationKind::Read { key, observation } => {
            validate_identifier("read.key", key).map_err(|error| {
                PhenomenaError::operation(error.class, &operation.operation_id, error.detail)
            })?;
            match observation {
                ReadObservation::Initial => {}
                ReadObservation::Write {
                    operation_id,
                    value,
                    ..
                } => {
                    validate_identifier("read.operation_id", operation_id).map_err(|error| {
                        PhenomenaError::operation(
                            error.class,
                            &operation.operation_id,
                            error.detail,
                        )
                    })?;
                    validate_text("read.value", value, MAX_VALUE_BYTES).map_err(|error| {
                        PhenomenaError::operation(
                            error.class,
                            &operation.operation_id,
                            error.detail,
                        )
                    })?;
                }
                ReadObservation::Unattributed { value } => {
                    validate_text("read.value", value, MAX_VALUE_BYTES).map_err(|error| {
                        PhenomenaError::operation(
                            error.class,
                            &operation.operation_id,
                            error.detail,
                        )
                    })?;
                }
            }
        }
        OperationKind::Write { key, value, .. } => {
            validate_identifier("write.key", key).map_err(|error| {
                PhenomenaError::operation(error.class, &operation.operation_id, error.detail)
            })?;
            validate_text("write.value", value, MAX_VALUE_BYTES).map_err(|error| {
                PhenomenaError::operation(error.class, &operation.operation_id, error.detail)
            })?;
        }
    }
    Ok(())
}

fn validate_read_observation(
    operation: &HistoryOperation,
    ids: &BTreeMap<&str, &HistoryOperation>,
) -> Result<(), PhenomenaError> {
    let OperationKind::Read { key, observation } = &operation.kind else {
        return Ok(());
    };
    let ReadObservation::Write {
        operation_id,
        version,
        value,
    } = observation
    else {
        return Ok(());
    };
    let observed = ids.get(operation_id.as_str()).ok_or_else(|| {
        PhenomenaError::operation(
            "read-observation",
            &operation.operation_id,
            format!("observed write {operation_id:?} is unknown"),
        )
    })?;
    let OperationKind::Write {
        key: write_key,
        version: write_version,
        value: write_value,
    } = &observed.kind
    else {
        return Err(PhenomenaError::operation(
            "read-observation",
            &operation.operation_id,
            "observed operation is not a write",
        ));
    };
    if write_key != key || write_version != version || write_value != value {
        return Err(PhenomenaError::operation(
            "read-observation",
            &operation.operation_id,
            "observed write key, version, or value does not match",
        ));
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), PhenomenaError> {
    validate_text(field, value, MAX_IDENTIFIER_BYTES)?;
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    }) {
        return Err(PhenomenaError::new(
            "identifier",
            format!("{field} contains a non-canonical byte"),
        ));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), PhenomenaError> {
    if value.is_empty() || value.len() > maximum {
        return Err(PhenomenaError::new(
            "text-bound",
            format!("{field} is empty or exceeds {maximum} bytes"),
        ));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(PhenomenaError::new(
            "text-character",
            format!("{field} contains a control byte"),
        ));
    }
    Ok(())
}

fn validate_digest(field: &'static str, value: &str) -> Result<(), PhenomenaError> {
    let Some(hex) = value.strip_prefix(BLAKE3_PREFIX) else {
        return Err(PhenomenaError::new(
            "digest",
            format!("{field} must use a BLAKE3 identity"),
        ));
    };
    if hex.len() != BLAKE3_HEX_BYTES
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(PhenomenaError::new(
            "digest",
            format!("{field} has malformed lowercase BLAKE3 hex"),
        ));
    }
    Ok(())
}

fn domain_hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
    format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex())
}
