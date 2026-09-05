//! Versioned fallback assertion records and deterministic bounded sinks.
//!
//! This module is the pure protocol boundary for processes that do not link
//! the Rust SDK. Filesystem or shared-device adapters supply complete lines;
//! this module validates, orders, bounds, identifies, and admits them.

use crate::admission::{
    token_for_descriptors, validate_accepted_catalog, AcceptedCatalog, CatalogBuilder,
    CatalogConflict,
};
use crate::identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION, MAX_ASSERTION_KEY_BYTES, MAX_ASSERTION_MESSAGE_BYTES,
    MAX_ASSERTION_NAMESPACE_BYTES,
};
use serde::{Deserialize, Serialize};

pub const FALLBACK_RECORD_SCHEMA_VERSION: u8 = 1;
pub const MAX_FALLBACK_RECORDS: usize = crate::admission::MAX_ASSERTION_CATALOG_ENTRIES;
pub const MAX_FALLBACK_LINE_BYTES: usize = 8_192;
pub const MAX_FALLBACK_DETAILS_BYTES: usize = crate::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES;
pub const MAX_FALLBACK_PROCESS_COMPONENT_BYTES: usize = 48;
pub const FALLBACK_ASSERTION_CATEGORY: &str = "fallback-process";
const FALLBACK_SOURCE_LINE: u32 = 1;
const FALLBACK_SOURCE_COLUMN: u32 = 1;
const FALLBACK_SINK_DOMAIN: &[u8] = b"chaoscontrol.fallback-assertion-sink.v1\0";
const FALLBACK_RECORD_DOMAIN: &[u8] = b"chaoscontrol.fallback-assertion-record.v1\0";
const LOWER_HEX_BYTES: usize = crate::identity::ASSERTION_FINGERPRINT_HEX_BYTES;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackProcessIdentity {
    pub guest: String,
    pub process: String,
}

impl FallbackProcessIdentity {
    pub fn validate(&self) -> Result<(), FallbackErrorKind> {
        validate_identifier(
            "process.guest",
            &self.guest,
            MAX_FALLBACK_PROCESS_COMPONENT_BYTES,
        )?;
        validate_identifier(
            "process.process",
            &self.process,
            MAX_FALLBACK_PROCESS_COMPONENT_BYTES,
        )
    }

    pub fn descriptor_guest(&self) -> String {
        format!("{}/process/{}", self.guest, self.process)
    }

    fn source_file(&self) -> String {
        format!("fallback/{}/{}", self.guest, self.process)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackRecordType {
    Always,
    Sometimes,
    Reachable,
    Unreachable,
    Lifecycle,
}

impl FallbackRecordType {
    pub fn assertion_kind(self) -> Option<AssertionKind> {
        match self {
            Self::Always => Some(AssertionKind::Always),
            Self::Sometimes => Some(AssertionKind::Sometimes),
            Self::Reachable => Some(AssertionKind::Reachable),
            Self::Unreachable => Some(AssertionKind::Unreachable),
            Self::Lifecycle => None,
        }
    }

    fn condition_required(self) -> bool {
        matches!(self, Self::Always | Self::Sometimes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackRecord {
    pub schema_version: u8,
    pub sequence: u64,
    pub process: FallbackProcessIdentity,
    pub namespace: String,
    pub logical_key: String,
    pub record_type: FallbackRecordType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<bool>,
    pub message: String,
    #[serde(default = "empty_details")]
    pub details: serde_json::Value,
}

fn empty_details() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

impl FallbackRecord {
    pub fn validate_at(&self, expected_sequence: u64) -> Result<(), FallbackErrorKind> {
        if self.schema_version != FALLBACK_RECORD_SCHEMA_VERSION {
            return Err(FallbackErrorKind::InvalidVersion);
        }
        if self.sequence != expected_sequence {
            return Err(FallbackErrorKind::InvalidSequence {
                expected: expected_sequence,
                actual: self.sequence,
            });
        }
        self.process.validate()?;
        validate_text("namespace", &self.namespace, MAX_ASSERTION_NAMESPACE_BYTES)?;
        validate_text("logical_key", &self.logical_key, MAX_ASSERTION_KEY_BYTES)?;
        validate_text("message", &self.message, MAX_ASSERTION_MESSAGE_BYTES)?;
        let details =
            serde_json::to_vec(&self.details).map_err(|_| FallbackErrorKind::MalformedDetails)?;
        if details.len() > MAX_FALLBACK_DETAILS_BYTES {
            return Err(FallbackErrorKind::DetailsTooLong);
        }
        if self.record_type.condition_required() && self.condition.is_none() {
            return Err(FallbackErrorKind::MissingCondition);
        }
        if !self.record_type.condition_required() && self.condition.is_some() {
            return Err(FallbackErrorKind::UnexpectedCondition);
        }
        if let Some(descriptor) = self.assertion_descriptor()? {
            descriptor
                .validate()
                .map_err(FallbackErrorKind::Descriptor)?;
        }
        Ok(())
    }

    pub fn assertion_descriptor(&self) -> Result<Option<AssertionDescriptor>, FallbackErrorKind> {
        let Some(kind) = self.record_type.assertion_kind() else {
            return Ok(None);
        };
        let descriptor = AssertionDescriptor {
            identity_version: ASSERTION_IDENTITY_VERSION,
            namespace: self.namespace.clone(),
            logical_key: AssertionLogicalKey::Stable {
                key: self.logical_key.clone(),
            },
            compatibility_id: None,
            kind,
            message: self.message.clone(),
            source_file: self.process.source_file(),
            source_line: FALLBACK_SOURCE_LINE,
            source_column: FALLBACK_SOURCE_COLUMN,
            guest: self.process.descriptor_guest(),
            category: FALLBACK_ASSERTION_CATEGORY.to_string(),
        };
        descriptor
            .validate()
            .map_err(FallbackErrorKind::Descriptor)?;
        Ok(Some(descriptor))
    }

    pub fn record_blake3(&self) -> Result<String, FallbackErrorKind> {
        let bytes = canonical_record_bytes(self)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(FALLBACK_RECORD_DOMAIN);
        hasher.update(&bytes);
        Ok(hasher.finalize().to_hex().to_string())
    }

    pub fn assertion_scope(
        &self,
        sink_blake3: &str,
    ) -> Result<Option<FallbackAssertionScope>, FallbackErrorKind> {
        let Some(descriptor) = self.assertion_descriptor()? else {
            return Ok(None);
        };
        Ok(Some(FallbackAssertionScope {
            process: self.process.clone(),
            record_sequence: self.sequence,
            record_blake3: self.record_blake3()?,
            sink_blake3: sink_blake3.to_string(),
            assertion_fingerprint: descriptor
                .fingerprint()
                .map_err(FallbackErrorKind::Descriptor)?,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackOverflowEvent {
    pub limit: usize,
    pub rejected_sequence: u64,
    pub process: FallbackProcessIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackSinkEvidence {
    pub limit: usize,
    pub records: Vec<FallbackRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overflow: Option<FallbackOverflowEvent>,
    pub sink_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackAssertionScope {
    pub process: FallbackProcessIdentity,
    pub record_sequence: u64,
    pub record_blake3: String,
    pub sink_blake3: String,
    pub assertion_fingerprint: AssertionFingerprint,
}

impl FallbackAssertionScope {
    pub fn validate_against(
        &self,
        identity: &crate::admission::AssertionEvidenceIdentity,
    ) -> Result<(), FallbackErrorKind> {
        self.process.validate()?;
        validate_digest("record_blake3", &self.record_blake3)?;
        validate_digest("sink_blake3", &self.sink_blake3)?;
        if self.assertion_fingerprint != identity.fingerprint {
            return Err(FallbackErrorKind::AssertionIdentityMismatch);
        }
        if identity.descriptor.category != FALLBACK_ASSERTION_CATEGORY
            || identity.descriptor.guest != self.process.descriptor_guest()
            || identity.descriptor.source_file != self.process.source_file()
        {
            return Err(FallbackErrorKind::ProcessScopeMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackSinkEvent {
    Accepted {
        sequence: u64,
        record_blake3: String,
    },
    Overflow(FallbackOverflowEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackSink {
    limit: usize,
    records: Vec<FallbackRecord>,
    overflow: Option<FallbackOverflowEvent>,
}

impl FallbackSink {
    pub fn new(limit: usize) -> Result<Self, FallbackError> {
        if limit == 0 || limit > MAX_FALLBACK_RECORDS {
            return Err(FallbackError::new(0, None, FallbackErrorKind::InvalidLimit));
        }
        Ok(Self {
            limit,
            records: Vec::with_capacity(limit),
            overflow: None,
        })
    }

    pub fn admit_line(&mut self, line: &str) -> Result<FallbackSinkEvent, FallbackError> {
        let record_index = u64::try_from(self.records.len())
            .map_err(|_| FallbackError::new(0, None, FallbackErrorKind::RecordCountOverflow))?;
        if self.overflow.is_some() {
            return Err(FallbackError::new(
                record_index,
                None,
                FallbackErrorKind::SinkOverflowed,
            ));
        }
        if line.len() > MAX_FALLBACK_LINE_BYTES {
            return Err(FallbackError::new(
                record_index,
                None,
                FallbackErrorKind::LineTooLong,
            ));
        }
        let record = serde_json::from_str::<FallbackRecord>(line).map_err(|_| {
            FallbackError::new(record_index, None, FallbackErrorKind::MalformedJson)
        })?;
        record
            .validate_at(record_index)
            .map_err(|kind| FallbackError::new(record_index, Some(record.process.clone()), kind))?;
        if self.records.len() >= self.limit {
            let overflow = FallbackOverflowEvent {
                limit: self.limit,
                rejected_sequence: record.sequence,
                process: record.process,
            };
            if self.overflow.is_none() {
                self.overflow = Some(overflow.clone());
            }
            return Ok(FallbackSinkEvent::Overflow(overflow));
        }
        let record_blake3 = record
            .record_blake3()
            .map_err(|kind| FallbackError::new(record_index, Some(record.process.clone()), kind))?;
        self.records.push(record);
        Ok(FallbackSinkEvent::Accepted {
            sequence: record_index,
            record_blake3,
        })
    }

    pub fn evidence(self) -> Result<FallbackSinkEvidence, FallbackError> {
        let sink_blake3 = compute_sink_blake3(self.limit, &self.records, self.overflow.as_ref())
            .map_err(|kind| FallbackError::new(0, None, kind))?;
        Ok(FallbackSinkEvidence {
            limit: self.limit,
            records: self.records,
            overflow: self.overflow,
            sink_blake3,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackError {
    pub record_index: u64,
    pub process: Option<FallbackProcessIdentity>,
    pub kind: FallbackErrorKind,
}

impl FallbackError {
    fn new(
        record_index: u64,
        process: Option<FallbackProcessIdentity>,
        kind: FallbackErrorKind,
    ) -> Self {
        Self {
            record_index,
            process,
            kind,
        }
    }
}

impl core::fmt::Display for FallbackError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let process = self
            .process
            .as_ref()
            .map_or("<missing>".to_string(), |value| {
                format!("{}/{}", value.guest, value.process)
            });
        write!(
            formatter,
            "fallback record {} for process {}: {:?}",
            self.record_index, process, self.kind
        )
    }
}

impl std::error::Error for FallbackError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackErrorKind {
    AssertionIdentityMismatch,
    Descriptor(crate::identity::AssertionError),
    DetailsTooLong,
    DigestMismatch,
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    InvalidCharacter(&'static str),
    InvalidDigest(&'static str),
    InvalidLimit,
    InvalidOverflow,
    InvalidSequence { expected: u64, actual: u64 },
    InvalidVersion,
    LineTooLong,
    MalformedDetails,
    MalformedJson,
    MissingCondition,
    ProcessScopeMismatch,
    RecordCountOverflow,
    SinkOverflowed,
    UnexpectedCondition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackCatalogEvent {
    pub process: FallbackProcessIdentity,
    pub record_sequence: u64,
    pub candidate_fingerprint: AssertionFingerprint,
    pub existing_fingerprint: Option<AssertionFingerprint>,
    pub conflict: CatalogConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackAdmissionError {
    Catalog(Box<FallbackCatalogEvent>),
    InvalidCatalog(CatalogConflict),
    InvalidSink(FallbackError),
    RecordCountOverflow,
}

pub fn validate_fallback_sink_evidence(
    evidence: &FallbackSinkEvidence,
) -> Result<(), FallbackError> {
    if evidence.limit == 0 || evidence.limit > MAX_FALLBACK_RECORDS {
        return Err(FallbackError::new(0, None, FallbackErrorKind::InvalidLimit));
    }
    if evidence.records.len() > evidence.limit {
        return Err(FallbackError::new(
            0,
            None,
            FallbackErrorKind::InvalidOverflow,
        ));
    }
    for (index, record) in evidence.records.iter().enumerate() {
        let expected = u64::try_from(index)
            .map_err(|_| FallbackError::new(0, None, FallbackErrorKind::RecordCountOverflow))?;
        record
            .validate_at(expected)
            .map_err(|kind| FallbackError::new(expected, Some(record.process.clone()), kind))?;
    }
    if let Some(overflow) = &evidence.overflow {
        let expected = u64::try_from(evidence.records.len())
            .map_err(|_| FallbackError::new(0, None, FallbackErrorKind::RecordCountOverflow))?;
        if evidence.records.len() != evidence.limit
            || overflow.limit != evidence.limit
            || overflow.rejected_sequence != expected
            || overflow.process.validate().is_err()
        {
            return Err(FallbackError::new(
                expected,
                Some(overflow.process.clone()),
                FallbackErrorKind::InvalidOverflow,
            ));
        }
    }
    let expected = compute_sink_blake3(
        evidence.limit,
        &evidence.records,
        evidence.overflow.as_ref(),
    )
    .map_err(|kind| FallbackError::new(0, None, kind))?;
    if evidence.sink_blake3 != expected {
        return Err(FallbackError::new(
            0,
            None,
            FallbackErrorKind::DigestMismatch,
        ));
    }
    Ok(())
}

pub fn catalog_with_fallback(
    base: &AcceptedCatalog,
    evidence: &FallbackSinkEvidence,
) -> Result<AcceptedCatalog, FallbackAdmissionError> {
    validate_accepted_catalog(base).map_err(FallbackAdmissionError::InvalidCatalog)?;
    validate_fallback_sink_evidence(evidence).map_err(FallbackAdmissionError::InvalidSink)?;

    let fallback_count = evidence
        .records
        .iter()
        .filter(|record| record.record_type.assertion_kind().is_some())
        .count();
    let expected = base
        .assertions
        .len()
        .checked_add(fallback_count)
        .ok_or(FallbackAdmissionError::RecordCountOverflow)?;
    let mut builder =
        CatalogBuilder::begin(expected).map_err(FallbackAdmissionError::InvalidCatalog)?;
    let mut descriptors = Vec::with_capacity(expected);
    let mut seen = std::collections::BTreeMap::new();
    for admitted in base.assertions.values() {
        builder
            .insert(admitted.descriptor.clone())
            .map_err(FallbackAdmissionError::InvalidCatalog)?;
        seen.insert(
            (
                admitted.descriptor.namespace.clone(),
                admitted.descriptor.logical_key.clone(),
            ),
            admitted.fingerprint,
        );
        descriptors.push(admitted.descriptor.clone());
    }
    for record in &evidence.records {
        let Some(descriptor) = record.assertion_descriptor().map_err(|kind| {
            FallbackAdmissionError::InvalidSink(FallbackError::new(
                record.sequence,
                Some(record.process.clone()),
                kind,
            ))
        })?
        else {
            continue;
        };
        let candidate_fingerprint = descriptor
            .fingerprint()
            .map_err(CatalogConflict::Descriptor)
            .map_err(FallbackAdmissionError::InvalidCatalog)?;
        let key = (descriptor.namespace.clone(), descriptor.logical_key.clone());
        let existing_fingerprint = seen.get(&key).copied();
        if let Err(conflict) = builder.insert(descriptor.clone()) {
            return Err(FallbackAdmissionError::Catalog(Box::new(
                FallbackCatalogEvent {
                    process: record.process.clone(),
                    record_sequence: record.sequence,
                    candidate_fingerprint,
                    existing_fingerprint,
                    conflict,
                },
            )));
        }
        seen.insert(key, candidate_fingerprint);
        descriptors.push(descriptor);
    }
    let token =
        token_for_descriptors(&descriptors).map_err(FallbackAdmissionError::InvalidCatalog)?;
    builder
        .complete(token)
        .map_err(FallbackAdmissionError::InvalidCatalog)
}

fn compute_sink_blake3(
    limit: usize,
    records: &[FallbackRecord],
    overflow: Option<&FallbackOverflowEvent>,
) -> Result<String, FallbackErrorKind> {
    let limit = u64::try_from(limit).map_err(|_| FallbackErrorKind::InvalidLimit)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(FALLBACK_SINK_DOMAIN);
    hasher.update(&limit.to_le_bytes());
    for record in records {
        let bytes = canonical_record_bytes(record)?;
        let length =
            u64::try_from(bytes.len()).map_err(|_| FallbackErrorKind::RecordCountOverflow)?;
        hasher.update(&length.to_le_bytes());
        hasher.update(&bytes);
    }
    if let Some(overflow) = overflow {
        let bytes =
            serde_json::to_vec(overflow).map_err(|_| FallbackErrorKind::MalformedDetails)?;
        let length =
            u64::try_from(bytes.len()).map_err(|_| FallbackErrorKind::RecordCountOverflow)?;
        hasher.update(&length.to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn canonical_record_bytes(record: &FallbackRecord) -> Result<Vec<u8>, FallbackErrorKind> {
    let mut canonical = record.clone();
    canonical.details = canonical_json(&record.details);
    serde_json::to_vec(&canonical).map_err(|_| FallbackErrorKind::MalformedDetails)
}

fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(canonical_json).collect())
        }
        serde_json::Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let mut canonical = serde_json::Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json(&values[key]));
            }
            serde_json::Value::Object(canonical)
        }
        scalar => scalar.clone(),
    }
}

fn validate_digest(field: &'static str, value: &str) -> Result<(), FallbackErrorKind> {
    if value.len() != LOWER_HEX_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || value.bytes().all(|byte| byte == b'0')
    {
        return Err(FallbackErrorKind::InvalidDigest(field));
    }
    Ok(())
}

fn validate_identifier(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), FallbackErrorKind> {
    validate_text(field, value, maximum)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(FallbackErrorKind::InvalidCharacter(field));
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), FallbackErrorKind> {
    if value.is_empty() {
        return Err(FallbackErrorKind::EmptyField(field));
    }
    if value.len() > maximum {
        return Err(FallbackErrorKind::FieldTooLong(field));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(FallbackErrorKind::InvalidCharacter(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINK_LIMIT: usize = 2;
    const FIXTURE_TERM: u64 = 3;
    const FIXTURE_SECOND_VALUE: u64 = 2;

    fn process(name: &str) -> FallbackProcessIdentity {
        FallbackProcessIdentity {
            guest: "guest-a".to_string(),
            process: name.to_string(),
        }
    }

    fn record(sequence: u64, name: &str, condition: bool) -> FallbackRecord {
        FallbackRecord {
            schema_version: FALLBACK_RECORD_SCHEMA_VERSION,
            sequence,
            process: process(name),
            namespace: "org.example.store".to_string(),
            logical_key: "wal-reset-safe".to_string(),
            record_type: FallbackRecordType::Always,
            condition: Some(condition),
            message: "WAL reset preserves committed state".to_string(),
            details: serde_json::json!({"term": FIXTURE_TERM}),
        }
    }

    fn base_catalog(descriptor: AssertionDescriptor) -> AcceptedCatalog {
        let descriptors = vec![descriptor];
        let token = token_for_descriptors(&descriptors).expect("token");
        let mut builder = CatalogBuilder::begin(descriptors.len()).expect("builder");
        for descriptor in descriptors {
            builder.insert(descriptor).expect("descriptor");
        }
        builder.complete(token).expect("catalog")
    }

    #[test]
    fn accepts_ordered_records_and_recomputes_sink_identity() {
        let mut sink = FallbackSink::new(SINK_LIMIT).expect("sink");
        let first = serde_json::to_string(&record(0, "wal", false)).expect("record");
        assert!(matches!(
            sink.admit_line(&first).expect("accepted"),
            FallbackSinkEvent::Accepted { sequence: 0, .. }
        ));
        let evidence = sink.evidence().expect("evidence");
        validate_fallback_sink_evidence(&evidence).expect("valid evidence");
        let descriptor = evidence.records[0]
            .assertion_descriptor()
            .expect("descriptor result")
            .expect("assertion descriptor");
        assert_eq!(descriptor.guest, "guest-a/process/wal");
        assert_eq!(descriptor.category, FALLBACK_ASSERTION_CATEGORY);
        let scope = evidence.records[0]
            .assertion_scope(&evidence.sink_blake3)
            .expect("scope result")
            .expect("scope");
        assert_eq!(
            scope.assertion_fingerprint,
            descriptor.fingerprint().expect("descriptor fingerprint")
        );
    }

    #[test]
    fn canonical_record_identity_ignores_object_key_order() {
        let mut first = record(0, "wal", true);
        first.details = serde_json::json!({"b": FIXTURE_SECOND_VALUE, "a": 1});
        let mut second = first.clone();
        second.details = serde_json::json!({"a": 1, "b": FIXTURE_SECOND_VALUE});
        assert_eq!(
            first.record_blake3().expect("first identity"),
            second.record_blake3().expect("second identity")
        );
    }

    #[test]
    fn rejects_reordered_and_missing_process_records() {
        let mut sink = FallbackSink::new(SINK_LIMIT).expect("sink");
        let reordered = serde_json::to_string(&record(1, "wal", true)).expect("record");
        assert!(matches!(
            sink.admit_line(&reordered).expect_err("reordered"),
            FallbackError {
                kind: FallbackErrorKind::InvalidSequence { .. },
                ..
            }
        ));

        let missing_process = serde_json::json!({
            "schema_version": FALLBACK_RECORD_SCHEMA_VERSION,
            "sequence": 0,
            "namespace": "org.example.store",
            "logical_key": "wal-reset-safe",
            "record_type": "always",
            "condition": true,
            "message": "missing process",
            "details": {}
        });
        let error = sink
            .admit_line(&missing_process.to_string())
            .expect_err("missing process");
        assert_eq!(error.kind, FallbackErrorKind::MalformedJson);
        assert_eq!(error.process, None);
    }

    #[test]
    fn emits_overflow_without_corrupting_accepted_prefix() {
        let mut sink = FallbackSink::new(1).expect("sink");
        sink.admit_line(&serde_json::to_string(&record(0, "wal", true)).expect("record line"))
            .expect("first");
        let overflow_record = FallbackRecord {
            sequence: 1,
            logical_key: "second".to_string(),
            ..record(1, "worker", true)
        };
        assert!(matches!(
            sink.admit_line(
                &serde_json::to_string(&overflow_record).expect("overflow record line"),
            )
            .expect("overflow event"),
            FallbackSinkEvent::Overflow(FallbackOverflowEvent {
                rejected_sequence: 1,
                ..
            })
        ));
        let evidence = sink.evidence().expect("evidence");
        assert_eq!(evidence.records.len(), 1);
        assert!(evidence.overflow.is_some());
        validate_fallback_sink_evidence(&evidence).expect("overflow evidence remains valid");

        let mut closed = FallbackSink::new(1).expect("closed sink");
        closed
            .admit_line(&serde_json::to_string(&record(0, "wal", true)).expect("record line"))
            .expect("first record");
        closed
            .admit_line(&serde_json::to_string(&overflow_record).expect("overflow record line"))
            .expect("overflow event");
        assert_eq!(
            closed
                .admit_line(&serde_json::to_string(&overflow_record).expect("post-overflow line"))
                .expect_err("closed sink rejects later records")
                .kind,
            FallbackErrorKind::SinkOverflowed
        );
    }

    #[test]
    fn reports_catalog_conflict_with_existing_and_candidate_identities() {
        let fallback = record(0, "wal", false);
        let mut sdk_descriptor = fallback
            .assertion_descriptor()
            .expect("descriptor result")
            .expect("descriptor");
        sdk_descriptor.message = "SDK message differs".to_string();
        let base = base_catalog(sdk_descriptor.clone());
        let mut sink = FallbackSink::new(SINK_LIMIT).expect("sink");
        sink.admit_line(&serde_json::to_string(&fallback).expect("fallback record line"))
            .expect("line");
        let evidence = sink.evidence().expect("evidence");
        let error = catalog_with_fallback(&base, &evidence).expect_err("conflict");
        let FallbackAdmissionError::Catalog(event) = error else {
            panic!("unexpected error: {error:?}");
        };
        assert_eq!(event.process, process("wal"));
        assert_eq!(
            event.existing_fingerprint,
            Some(
                sdk_descriptor
                    .fingerprint()
                    .expect("SDK descriptor fingerprint")
            )
        );
        assert_eq!(event.conflict, CatalogConflict::MessageConflict);
    }

    #[test]
    fn rejects_sink_identity_drift() {
        let mut sink = FallbackSink::new(SINK_LIMIT).expect("sink");
        sink.admit_line(&serde_json::to_string(&record(0, "wal", true)).expect("record line"))
            .expect("line");
        let mut evidence = sink.evidence().expect("evidence");
        evidence.sink_blake3 = "a".repeat(LOWER_HEX_BYTES);
        assert_eq!(
            validate_fallback_sink_evidence(&evidence)
                .expect_err("identity drift")
                .kind,
            FallbackErrorKind::DigestMismatch
        );
    }
}
