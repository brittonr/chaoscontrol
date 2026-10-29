//! Bounded queries and static timelines over retained event envelopes.
//!
//! This module is pure: callers supply admitted events, queries, and facts.
//! It performs no file, clock, process, network, or environment access.

use crate::recording::FaultRoundTraceDelta;
use chaoscontrol_fault::outcomes::{
    FaultOutcomeLedger, FaultStageEvent, FaultStageKind, NANOSECONDS_PER_SIMULATION_TICK,
};
use chaoscontrol_protocol::branch_marker::MarkerEvidenceContext;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

pub const EVENT_ENVELOPE_SCHEMA: &str = "chaoscontrol.event-envelope.v1";
pub const EVENT_QUERY_RECEIPT_SCHEMA: &str = "chaoscontrol.event-query-receipt.v1";
pub const EVENT_QUERY_RESULT_SCHEMA: &str = "chaoscontrol.event-query-result.v1";
pub const TIMELINE_SCHEMA: &str = "chaoscontrol.event-timeline.v1";
pub const TIMELINE_RECEIPT_SCHEMA: &str = "chaoscontrol.event-timeline-receipt.v1";

pub const MAX_QUERY_NODES: usize = 256;
pub const MAX_QUERY_DEPTH: usize = 64;
pub const MAX_SCANNED_EVENTS: usize = 1_000_000;
pub const MAX_QUERY_OPERATIONS: usize = 10_000_000;
pub const MAX_RESULT_ROWS: usize = 100_000;
pub const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_EVENT_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_EVENT_DETAILS_BYTES: usize = 64 * 1024;
pub const MAX_DERIVED_FIELDS: usize = MAX_QUERY_NODES;
pub const MAX_DERIVED_BYTES: usize = MAX_EVENT_DETAILS_BYTES;
pub const MAX_TIMELINE_NODES: usize = 10_000;
pub const MAX_TIMELINE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FIELD_BYTES: usize = 256;

// Bound expanded programs and lazy DFA caches independently of source-pattern bytes.
const MAX_REGEX_PROGRAM_BYTES: usize = 256 * 1024;
const MAX_REGEX_DFA_BYTES: usize = 256 * 1024;
const MAX_REGEX_NEST_DEPTH: u32 = 64;

type RegexCache<'a> = BTreeMap<&'a str, regex::Regex>;

/// Retained source family of one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSourceKind {
    Recording,
    Dlog,
    SdkLifecycle,
    Assertion,
    FaultStage,
    Serial,
    BranchMarker,
}

impl EventSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Dlog => "dlog",
            Self::SdkLifecycle => "sdk_lifecycle",
            Self::Assertion => "assertion",
            Self::FaultStage => "fault_stage",
            Self::Serial => "serial",
            Self::BranchMarker => "branch_marker",
        }
    }
}

/// One canonical event envelope over retained source facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub schema: String,
    pub source_id: [u8; 32],
    pub source_kind: EventSourceKind,
    pub source_sequence: u64,
    pub tick: u64,
    #[serde(default = "no_u32", skip_serializing_if = "Option::is_none")]
    pub vm_index: Option<u32>,
    #[serde(default = "no_string", skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(default = "no_u32", skip_serializing_if = "Option::is_none")]
    pub thread: Option<u32>,
    pub kind: String,
    pub text: String,
    pub payload_digest: [u8; 32],
    #[serde(default = "no_json", skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(default = "empty_derived", skip_serializing_if = "BTreeMap::is_empty")]
    pub derived: BTreeMap<String, String>,
}

fn no_u32() -> Option<u32> {
    None
}

fn no_u64() -> Option<u64> {
    None
}

fn no_string() -> Option<String> {
    None
}

fn no_json() -> Option<serde_json::Value> {
    None
}

fn empty_derived() -> BTreeMap<String, String> {
    BTreeMap::new()
}

/// Build canonical envelopes from one retained recording.
pub fn envelopes_from_recording(
    recording: &crate::recording::Recording,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chaoscontrol.recording-source.v1\0");
    hasher.update(recording.session_id.as_bytes());
    hasher.update(&recording.fault_run_id.0);
    let source_id = *hasher.finalize().as_bytes();
    let mut envelopes: Vec<EventEnvelope> = recording
        .events
        .iter()
        .enumerate()
        .map(|(sequence, event)| envelope_from_recorded_event(source_id, sequence as u64, event))
        .collect();
    for envelope in &envelopes {
        admit_event(envelope)?;
    }
    let stages: &[FaultStageEvent] = if recording.fault_stage_events.is_empty() {
        &recording.fault_outcome_ledger.events
    } else {
        &recording.fault_stage_events
    };
    envelopes.extend(envelopes_from_fault_stages(
        fault_stage_source_id(recording),
        stages,
        &recording.fault_round_deltas,
        &recording.fault_outcome_ledger,
    )?);
    Ok(envelopes)
}

/// Stable identity for the canonical fault-stage trace of one recording.
fn fault_stage_source_id(recording: &crate::recording::Recording) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chaoscontrol.fault-stage-source.v1\0");
    hasher.update(recording.session_id.as_bytes());
    hasher.update(&recording.fault_run_id.0);
    *hasher.finalize().as_bytes()
}

/// Build canonical envelopes from a retained fault-stage trace.
///
/// Round deltas supply the exact round tick. Legacy traces without deltas use
/// the attempt selection tick as the only retained time fact.
pub fn envelopes_from_fault_stages(
    source_id: [u8; 32],
    events: &[FaultStageEvent],
    deltas: &[FaultRoundTraceDelta],
    ledger: &FaultOutcomeLedger,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    if source_id == [0; 32] {
        return Err(EventQueryError::InvalidEvent);
    }
    events
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let envelope = envelope_from_fault_stage(source_id, index, event, deltas, ledger)?;
            admit_event(&envelope)?;
            Ok(envelope)
        })
        .collect()
}

fn envelope_from_fault_stage(
    source_id: [u8; 32],
    index: usize,
    event: &FaultStageEvent,
    deltas: &[FaultRoundTraceDelta],
    ledger: &FaultOutcomeLedger,
) -> Result<EventEnvelope, EventQueryError> {
    let sequence = u64::try_from(index).map_err(|_| EventQueryError::CounterOverflow)?;
    if event.sequence != sequence {
        return Err(EventQueryError::InvalidEvent);
    }
    let attempt_state = ledger
        .attempts
        .get(&event.attempt_id)
        .ok_or(EventQueryError::InvalidEvent)?;
    let tick = if deltas.is_empty() {
        attempt_state.attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK
    } else {
        deltas
            .iter()
            .find(|delta| sequence >= delta.event_start && sequence < delta.event_end)
            .map(|delta| delta.tick)
            .ok_or(EventQueryError::InvalidEvent)?
    };
    let stage = fault_stage_name(&event.kind);
    let payload = serde_json::to_vec(event).map_err(|_| EventQueryError::InvalidEvent)?;
    let fault = &attempt_state.attempt.fault;
    Ok(EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id,
        source_kind: EventSourceKind::FaultStage,
        source_sequence: sequence,
        tick,
        vm_index: None,
        process: None,
        thread: None,
        kind: format!("fault_stage_{stage}"),
        text: bounded_text(&format!("{stage} attempt={}", event.attempt_id)),
        payload_digest: *blake3::hash(&payload).as_bytes(),
        details: Some(serde_json::json!({
            "attempt_id": event.attempt_id.to_string(),
            "stage": stage,
            "selected_at_ns": attempt_state.attempt.selected_at_ns,
            "fault": fault,
        })),
        derived: BTreeMap::new(),
    })
}

fn fault_stage_name(kind: &FaultStageKind) -> &'static str {
    match kind {
        FaultStageKind::Selected => "selected",
        FaultStageKind::Applicable { .. } => "applicable",
        FaultStageKind::Rejected { .. } => "rejected",
        FaultStageKind::Applied { .. } => "applied",
        FaultStageKind::ApplicationFailed { .. } => "application_failed",
        FaultStageKind::Observed { .. } => "observed",
    }
}

/// Build one canonical envelope from a recorded event.
pub fn envelope_from_recorded_event(
    source_id: [u8; 32],
    source_sequence: u64,
    event: &crate::recording::RecordedEvent,
) -> EventEnvelope {
    use crate::recording::RecordedEvent;
    let (source_kind, kind, tick, vm_index, text, details) = match event {
        RecordedEvent::FaultFired { tick, fault } => (
            EventSourceKind::FaultStage,
            "fault_fired",
            *tick,
            None,
            fault.clone(),
            Some(serde_json::json!({"fault": fault})),
        ),
        RecordedEvent::AssertionHit {
            tick,
            vm_index,
            assertion_id,
            location,
            passed,
        } => (
            EventSourceKind::Assertion,
            "assertion_hit",
            *tick,
            Some(*vm_index as u32),
            location.clone(),
            Some(serde_json::json!({"assertion_id": assertion_id, "passed": passed})),
        ),
        RecordedEvent::VmStatusChange {
            tick,
            vm_index,
            old_status,
            new_status,
        } => (
            EventSourceKind::SdkLifecycle,
            "vm_status_change",
            *tick,
            Some(*vm_index as u32),
            format!("{old_status}->{new_status}"),
            Some(serde_json::json!({"old": old_status, "new": new_status})),
        ),
        RecordedEvent::SerialOutput {
            tick,
            vm_index,
            data,
        } => (
            EventSourceKind::Serial,
            "serial_output",
            *tick,
            Some(*vm_index as u32),
            data.clone(),
            None,
        ),
        RecordedEvent::BugDetected {
            tick,
            bug_id,
            description,
            checkpoint_id,
        } => (
            EventSourceKind::Assertion,
            "bug_detected",
            *tick,
            None,
            description.clone(),
            Some(serde_json::json!({"bug_id": bug_id, "checkpoint_id": checkpoint_id})),
        ),
    };
    let payload = serde_json::to_vec(event).unwrap_or_default();
    let mut text = text;
    if text.is_empty() {
        text = kind.to_string();
    }
    text = bounded_text(&text);
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id,
        source_kind,
        source_sequence,
        tick,
        vm_index,
        process: None,
        thread: None,
        kind: kind.to_string(),
        text,
        payload_digest: *blake3::hash(&payload).as_bytes(),
        details,
        derived: BTreeMap::new(),
    }
}

/// Build canonical envelopes from raw dlog bytes.
///
/// The source identity is the BLAKE3 digest of the complete dlog byte stream.
pub fn envelopes_from_dlog_bytes(
    bytes: &[u8],
    max_records: usize,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    use chaoscontrol_vmm::dlog::{DlogRecord, RECORD_SIZE};
    if !bytes.len().is_multiple_of(RECORD_SIZE) {
        return Err(EventQueryError::InvalidEvent);
    }
    if max_records == 0 || bytes.len() / RECORD_SIZE > max_records {
        return Err(EventQueryError::ScanBound);
    }
    let source_id = *blake3::hash(bytes).as_bytes();
    let mut envelopes = Vec::new();
    for sequence in 0..bytes.len() / RECORD_SIZE {
        let start = sequence * RECORD_SIZE;
        let mut record = [0_u8; RECORD_SIZE];
        record.copy_from_slice(&bytes[start..start + RECORD_SIZE]);
        let record = DlogRecord::from_bytes(record);
        let envelope = envelope_from_dlog(source_id, sequence as u64, &record);
        admit_event(&envelope)?;
        envelopes.push(envelope);
    }
    Ok(envelopes)
}

/// Build canonical envelopes from retained branch-marker evidence.
///
/// The caller supplies the source identity. Each marker keeps its observed
/// tick and owner; marker order is the retained order.
pub fn envelopes_from_markers(
    source_id: [u8; 32],
    markers: &[MarkerEvidenceContext],
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    if source_id == [0; 32] {
        return Err(EventQueryError::InvalidEvent);
    }
    markers
        .iter()
        .enumerate()
        .map(|(sequence, marker)| {
            let envelope = envelope_from_marker(source_id, sequence as u64, marker)?;
            admit_event(&envelope)?;
            Ok(envelope)
        })
        .collect()
}

fn envelope_from_marker(
    source_id: [u8; 32],
    source_sequence: u64,
    marker: &MarkerEvidenceContext,
) -> Result<EventEnvelope, EventQueryError> {
    marker
        .validate()
        .map_err(|_| EventQueryError::InvalidEvent)?;
    let details = serde_json::to_value(marker).map_err(|_| EventQueryError::InvalidEvent)?;
    let payload = serde_json::to_vec(&details).map_err(|_| EventQueryError::InvalidEvent)?;
    Ok(EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id,
        source_kind: EventSourceKind::BranchMarker,
        source_sequence,
        tick: marker.observed_tick,
        vm_index: None,
        process: Some(marker.owner.clone()),
        thread: None,
        kind: "branch_marker".to_string(),
        text: marker.marker_identity.clone(),
        payload_digest: *blake3::hash(&payload).as_bytes(),
        details: Some(details),
        derived: BTreeMap::new(),
    })
}

fn envelope_from_dlog(
    source_id: [u8; 32],
    source_sequence: u64,
    record: &chaoscontrol_vmm::dlog::DlogRecord,
) -> EventEnvelope {
    let text = format!(
        "tag={} exit={} rip={:#x} vcpu={}",
        record.tag, record.exit_count, record.rip, record.vcpu
    );
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id,
        source_kind: EventSourceKind::Dlog,
        source_sequence,
        tick: record.virtual_tsc,
        vm_index: None,
        process: None,
        thread: Some(u32::from(record.vcpu)),
        kind: format!("dlog_{}", record.tag),
        text: bounded_text(&text),
        payload_digest: *blake3::hash(&record.to_bytes()).as_bytes(),
        details: Some(serde_json::json!({
            "seq": record.seq,
            "exit_count": record.exit_count,
            "rip": record.rip,
            "tag": record.tag,
            "vcpu": record.vcpu,
            "port_or_addr": u32::from(record.port_or_addr_lo) | (u32::from(record.port_or_addr_hi) << 16),
            "data": record.data,
            "extra": record.extra,
        })),
        derived: BTreeMap::new(),
    }
}

fn bounded_text(value: &str) -> String {
    crate::text::prefix_at_char_boundary(value, MAX_EVENT_TEXT_BYTES).to_string()
}

/// Comparison of finite IEEE-754 binary64 values (JavaScript Number semantics).
/// Integers outside binary64's exact range may round before comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberComparison {
    LessThan,
    LessThanOrEqual,
    Equal,
    NotEqual,
    GreaterThanOrEqual,
    GreaterThan,
}

impl NumberComparison {
    fn matches(self, actual: Option<&serde_json::Value>, expected: &serde_json::Number) -> bool {
        actual
            .and_then(serde_json::Value::as_f64)
            .filter(|actual| actual.is_finite())
            .zip(expected.as_f64())
            .is_some_and(|(actual, expected)| match self {
                Self::LessThan => actual < expected,
                Self::LessThanOrEqual => actual <= expected,
                Self::Equal => actual == expected,
                // r[impl chaoscontrol.numeric_event_query.inequality]
                Self::NotEqual => actual != expected,
                Self::GreaterThanOrEqual => actual >= expected,
                Self::GreaterThan => actual > expected,
            })
    }
}

/// A bounded predicate over one event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventPredicate {
    /// Left-to-right conjunction; empty input matches every event.
    All(Vec<EventPredicate>),
    /// Left-to-right disjunction; empty input matches no events.
    Any(Vec<EventPredicate>),
    /// Negate the complete child result, including a missing-field non-match.
    Not(Box<EventPredicate>),
    TickRange {
        start_tick: u64,
        end_tick: u64,
    },
    Vm(u32),
    SourceKind(EventSourceKind),
    Kind(String),
    Process(String),
    /// Literal case-sensitive substring selection over canonical or derived fields.
    Contains {
        field: String,
        text: String,
    },
    /// Exact case-sensitive string selection (not a regular expression).
    Matches {
        field: String,
        text: String,
    },
    /// Search a canonical or derived string with a bounded Rust regular expression.
    Regex {
        field: String,
        pattern: String,
    },
    /// Compare finite JSON-number text in a canonical or derived string field.
    FieldNumber {
        field: String,
        comparison: NumberComparison,
        value: serde_json::Number,
    },
    /// Compare a finite binary64 quotient of two selected numeric string fields.
    FieldRatio {
        numerator: String,
        denominator: String,
        comparison: NumberComparison,
        value: serde_json::Number,
    },
    /// Compare a JSON number in `details`; missing/non-numeric values never match.
    JsonNumber {
        pointer: String,
        comparison: NumberComparison,
        value: serde_json::Number,
    },
    /// Compare a JSON number in canonical text; visited invalid JSON is an error.
    JsonTextNumber {
        pointer: String,
        comparison: NumberComparison,
        value: serde_json::Number,
    },
    /// Test pointer presence in `details`, including explicit null and containers.
    JsonPresent {
        pointer: String,
    },
    /// Test pointer presence in JSON text; visited invalid JSON is an error.
    JsonTextPresent {
        pointer: String,
    },
}

/// Strict tick direction and witness existence for retained temporal search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalRelation {
    PrecededBy,
    NotPrecededBy,
    FollowedBy,
    NotFollowedBy,
}

/// A typed, bounded query expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventQuery {
    All,
    Filter {
        input: Box<EventQuery>,
        predicate: EventPredicate,
    },
    /// Annotate unique canonical prefixes with inclusive predicate match counts.
    RunningCount {
        input: Box<EventQuery>,
        predicate: EventPredicate,
        field: String,
    },
    /// Annotate unique canonical prefixes with inclusive finite numeric totals.
    RunningSum {
        input: Box<EventQuery>,
        predicate: EventPredicate,
        field: String,
        value_field: String,
    },
    /// Annotate unique canonical prefixes with inclusive finite numeric minima.
    RunningMin {
        input: Box<EventQuery>,
        predicate: EventPredicate,
        field: String,
        value_field: String,
    },
    /// Annotate unique canonical prefixes with inclusive finite numeric maxima.
    RunningMax {
        input: Box<EventQuery>,
        predicate: EventPredicate,
        field: String,
        value_field: String,
    },
    Map {
        input: Box<EventQuery>,
        field: String,
        pointer: String,
    },
    /// Project a JSON scalar from canonical text, preserving original source fields.
    MapText {
        input: Box<EventQuery>,
        field: String,
        pointer: String,
    },
    /// Annotate finite field quotients, removing stale output for undefined ratios.
    MapRatio {
        input: Box<EventQuery>,
        field: String,
        numerator: String,
        denominator: String,
    },
    /// Annotate finite field differences, removing stale output when undefined.
    MapDifference {
        input: Box<EventQuery>,
        field: String,
        minuend: String,
        subtrahend: String,
    },
    /// Retain selected derived fields without discarding canonical source evidence.
    Narrow {
        input: Box<EventQuery>,
        fields: Vec<String>,
    },
    Contains {
        input: Box<EventQuery>,
        field: String,
        text: String,
    },
    Matches {
        input: Box<EventQuery>,
        field: String,
        text: String,
    },
    Union {
        inputs: Vec<EventQuery>,
    },
    /// Keep the first encountered row per tick, preferring earlier inputs.
    DistinctByTick {
        inputs: Vec<EventQuery>,
    },
    /// Keep left rows present in `other`, using source-position identity.
    Intersect {
        input: Box<EventQuery>,
        other: Box<EventQuery>,
    },
    /// Keep left rows absent from `other`, using source-position identity.
    Difference {
        input: Box<EventQuery>,
        other: Box<EventQuery>,
    },
    /// Select candidates by nearby witnesses in the caller-supplied tick domain.
    Temporal {
        input: Box<EventQuery>,
        witness: Box<EventQuery>,
        relation: TemporalRelation,
        within_ticks: u64,
    },
    /// Copy a scalar from the nearest strictly earlier witness in a tick window.
    WithLastValue {
        input: Box<EventQuery>,
        witness: Box<EventQuery>,
        field: String,
        witness_field: String,
        within_ticks: u64,
    },
    Window {
        input: Box<EventQuery>,
        start_tick: u64,
        end_tick: u64,
    },
    UpTo {
        input: Box<EventQuery>,
        moment_tick: u64,
    },
}

/// Named finite query limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventQueryLimits {
    pub max_nodes: usize,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    pub max_scanned_events: usize,
    pub max_result_rows: usize,
    #[serde(default = "default_max_result_bytes")]
    pub max_result_bytes: usize,
    #[serde(default = "default_max_operations")]
    pub max_operations: usize,
}

fn default_max_result_bytes() -> usize {
    MAX_RESULT_BYTES
}

fn default_max_operations() -> usize {
    MAX_QUERY_OPERATIONS
}

fn default_max_depth() -> usize {
    MAX_QUERY_DEPTH
}

impl Default for EventQueryLimits {
    fn default() -> Self {
        Self {
            max_nodes: MAX_QUERY_NODES,
            max_depth: MAX_QUERY_DEPTH,
            max_scanned_events: MAX_SCANNED_EVENTS,
            max_result_rows: MAX_RESULT_ROWS,
            max_result_bytes: MAX_RESULT_BYTES,
            max_operations: MAX_QUERY_OPERATIONS,
        }
    }
}

/// One bounded query result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventQueryResult {
    pub schema: String,
    pub rows: Vec<EventEnvelope>,
    pub scanned: u64,
    pub matched: u64,
    pub truncated: bool,
}

/// Evidence receipt for one query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventQueryReceipt {
    pub schema: String,
    pub source_ids: Vec<[u8; 32]>,
    #[serde(default)]
    pub events_digest: [u8; 32],
    pub query: EventQuery,
    #[serde(default = "no_u64", skip_serializing_if = "Option::is_none")]
    pub moment_tick: Option<u64>,
    pub limits: EventQueryLimits,
    pub scanned: u64,
    pub matched: u64,
    pub rows: u64,
    #[serde(default)]
    pub truncated: bool,
    pub rows_digest: [u8; 32],
}

/// One static timeline fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineFact {
    pub id: u64,
    #[serde(default = "no_u64", skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    pub tick: u64,
    pub kind: String,
    pub label: String,
}

/// Query, timeline, and receipt failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventQueryError {
    QueryBound,
    QueryDepth,
    AmbiguousMoment,
    ScanBound,
    RowBound,
    ResultBound,
    OperationBound,
    InvalidEvent,
    InvalidResult,
    DuplicateSourcePosition,
    InvalidPredicate,
    InvalidText,
    InvalidJsonText,
    InvalidRegex,
    InvalidNumber,
    InvalidWindow,
    InvalidFormat,
    InvalidField,
    InvalidPointer,
    ReceiptDrift,
    ResultDrift,
    SourceDrift,
    TimelineReceiptDrift,
    TimelineBound,
    TimelineByteBound,
    DuplicateTimelineId,
    MissingTimelineParent,
    TimelineCycle,
    CounterOverflow,
    Digest,
}

impl core::fmt::Display for EventQueryError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::QueryBound => "event query node count exceeds the bound",
            Self::QueryDepth => "event query depth exceeds the bound",
            Self::AmbiguousMoment => "event query combines more than one moment",
            Self::ScanBound => "event query scan count exceeds the bound",
            Self::RowBound => "event query result rows exceed the bound",
            Self::ResultBound => "event query result bytes exceed the bound",
            Self::OperationBound => "event query operations exceed the bound",
            Self::InvalidEvent => "event envelope schema or identity is invalid",
            Self::InvalidResult => "event query result is malformed or inconsistent",
            Self::DuplicateSourcePosition => {
                "two events claim one source position with different content"
            }
            Self::InvalidPredicate => "event predicate is malformed or unsupported",
            Self::InvalidText => "event text is empty, over-bound, or contains controls",
            Self::InvalidJsonText => "event text is not valid JSON within parser bounds",
            Self::InvalidRegex => "event regex is invalid or exceeds compilation bounds",
            Self::InvalidNumber => "event query requires finite JSON-number operands and totals",
            Self::InvalidWindow => "event query window is reversed",
            Self::InvalidFormat => "event timeline format is not markdown or dot",
            Self::InvalidField => "event query field is not supported",
            Self::InvalidPointer => "event query JSON pointer is not confined",
            Self::ReceiptDrift => "event query receipt does not match the admitted query facts",
            Self::ResultDrift => "event query result does not match a fresh evaluation",
            Self::SourceDrift => {
                "event query receipt source identity or content does not match the inputs"
            }
            Self::TimelineReceiptDrift => {
                "event timeline receipt does not match the admitted facts and output"
            }
            Self::TimelineBound => "timeline node count exceeds the bound",
            Self::TimelineByteBound => "timeline output bytes exceed the bound",
            Self::DuplicateTimelineId => "timeline node identity is duplicated",
            Self::MissingTimelineParent => "timeline node parent is missing",
            Self::TimelineCycle => "timeline node parentage contains a cycle",
            Self::CounterOverflow => "event query counter overflow",
            Self::Digest => "event query result digest failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for EventQueryError {}

impl EventQueryLimits {
    pub fn validate(&self) -> Result<(), EventQueryError> {
        if self.max_nodes == 0
            || self.max_nodes > MAX_QUERY_NODES
            || self.max_depth == 0
            || self.max_depth > MAX_QUERY_DEPTH
            || self.max_scanned_events == 0
            || self.max_scanned_events > MAX_SCANNED_EVENTS
            || self.max_result_rows == 0
            || self.max_result_rows > MAX_RESULT_ROWS
            || self.max_result_bytes == 0
            || self.max_result_bytes > MAX_RESULT_BYTES
            || self.max_operations == 0
            || self.max_operations > MAX_QUERY_OPERATIONS
        {
            return Err(EventQueryError::QueryBound);
        }
        Ok(())
    }
}

/// Admit one event envelope.
pub fn admit_event(event: &EventEnvelope) -> Result<(), EventQueryError> {
    if event.schema != EVENT_ENVELOPE_SCHEMA || event.source_id == [0; 32] {
        return Err(EventQueryError::InvalidEvent);
    }
    validate_text(&event.kind)?;
    validate_text(&event.text)?;
    if let Some(process) = &event.process {
        validate_text(process)?;
    }
    if let Some(details) = &event.details {
        let bytes = serde_json::to_vec(details).map_err(|_| EventQueryError::InvalidEvent)?;
        if bytes.len() > MAX_EVENT_DETAILS_BYTES {
            return Err(EventQueryError::InvalidEvent);
        }
    }
    if event.derived.len() > MAX_DERIVED_FIELDS {
        return Err(EventQueryError::InvalidEvent);
    }
    let mut derived_bytes = 0_usize;
    for (field, value) in &event.derived {
        derived_bytes = derived_bytes
            .checked_add(field.len())
            .and_then(|total| total.checked_add(value.len()))
            .ok_or(EventQueryError::CounterOverflow)?;
        if derived_bytes > MAX_DERIVED_BYTES {
            return Err(EventQueryError::InvalidEvent);
        }
    }
    Ok(())
}

/// Reject two distinct records that claim one source position.
///
/// Byte-identical repeats are allowed so a union can collapse them; any
/// content difference at the same `(source_id, source_sequence)` is ambiguous.
fn validate_source_positions(events: &[EventEnvelope]) -> Result<(), EventQueryError> {
    let mut positions: BTreeMap<([u8; 32], u64), &EventEnvelope> = BTreeMap::new();
    for event in events {
        let position = (event.source_id, event.source_sequence);
        match positions.get(&position) {
            Some(existing) if !same_source_content(existing, event) => {
                return Err(EventQueryError::DuplicateSourcePosition);
            }
            Some(_) => {}
            None => {
                positions.insert(position, event);
            }
        }
    }
    Ok(())
}

/// Compare source-visible content; `derived` is query-local and excluded.
fn same_source_content(left: &EventEnvelope, right: &EventEnvelope) -> bool {
    left.schema == right.schema
        && left.source_id == right.source_id
        && left.source_kind == right.source_kind
        && left.source_sequence == right.source_sequence
        && left.tick == right.tick
        && left.vm_index == right.vm_index
        && left.process == right.process
        && left.thread == right.thread
        && left.kind == right.kind
        && left.text == right.text
        && left.payload_digest == right.payload_digest
        && left.details == right.details
}

/// Evaluate one query over admitted events.
pub fn evaluate(
    query: &EventQuery,
    events: &[EventEnvelope],
    limits: &EventQueryLimits,
) -> Result<EventQueryResult, EventQueryError> {
    limits.validate()?;
    let regexes = &validate_query(query, limits)?;
    if events.len() > limits.max_scanned_events {
        return Err(EventQueryError::ScanBound);
    }
    for event in events {
        admit_event(event)?;
    }
    validate_source_positions(events)?;
    let mut operations = 0_usize;
    let mut rows = evaluate_rows(
        query,
        events,
        &mut operations,
        limits.max_operations,
        regexes,
    )?;
    for event in &rows {
        admit_event(event)?;
    }
    if !rows.is_sorted_by(|left, right| event_order(left) <= event_order(right)) {
        rows.sort_by_key(event_order);
    }
    rows.dedup_by_key(|event| (event.source_id, event.source_sequence));
    let matched = u64::try_from(rows.len()).map_err(|_| EventQueryError::CounterOverflow)?;
    let truncated = rows.len() > limits.max_result_rows;
    if truncated {
        rows.truncate(limits.max_result_rows);
    }
    serialized_rows_bytes(&rows, limits.max_result_bytes)?;
    Ok(EventQueryResult {
        schema: EVENT_QUERY_RESULT_SCHEMA.to_string(),
        rows,
        scanned: u64::try_from(events.len()).map_err(|_| EventQueryError::CounterOverflow)?,
        matched,
        truncated,
    })
}

fn serialized_rows_bytes(rows: &[EventEnvelope], limit: usize) -> Result<usize, EventQueryError> {
    let mut total = 2_usize;
    if total > limit {
        return Err(EventQueryError::ResultBound);
    }
    for (index, row) in rows.iter().enumerate() {
        let bytes = serde_json::to_vec(row).map_err(|_| EventQueryError::Digest)?;
        total = total
            .checked_add(bytes.len())
            .and_then(|total| {
                if index > 0 {
                    total.checked_add(1)
                } else {
                    Some(total)
                }
            })
            .ok_or(EventQueryError::CounterOverflow)?;
        if total > limit {
            return Err(EventQueryError::ResultBound);
        }
    }
    Ok(total)
}

/// Build a receipt binding the query, source identities, limits, and rows.
pub fn receipt_for(
    query: &EventQuery,
    events: &[EventEnvelope],
    result: &EventQueryResult,
    limits: &EventQueryLimits,
) -> Result<EventQueryReceipt, EventQueryError> {
    limits.validate()?;
    validate_query(query, limits)?;
    if events.len() > limits.max_scanned_events {
        return Err(EventQueryError::ScanBound);
    }
    for event in events {
        admit_event(event)?;
    }
    validate_source_positions(events)?;
    if result.schema != EVENT_QUERY_RESULT_SCHEMA
        || result.scanned
            != u64::try_from(events.len()).map_err(|_| EventQueryError::CounterOverflow)?
    {
        return Err(EventQueryError::InvalidResult);
    }
    if result.rows.len() > limits.max_result_rows {
        return Err(EventQueryError::RowBound);
    }
    let rows_len =
        u64::try_from(result.rows.len()).map_err(|_| EventQueryError::CounterOverflow)?;
    if result.matched > result.scanned || result.matched < rows_len {
        return Err(EventQueryError::InvalidResult);
    }
    if result.truncated {
        if result.rows.len() != limits.max_result_rows || result.matched <= rows_len {
            return Err(EventQueryError::InvalidResult);
        }
    } else if result.matched != rows_len {
        return Err(EventQueryError::InvalidResult);
    }
    validate_source_positions(&result.rows)?;
    if !result
        .rows
        .windows(2)
        .all(|pair| event_order(&pair[0]) < event_order(&pair[1]))
    {
        return Err(EventQueryError::InvalidResult);
    }
    for row in &result.rows {
        admit_event(row)?;
    }
    serialized_rows_bytes(&result.rows, limits.max_result_bytes)?;
    let rows = serde_json::to_vec(&result.rows).map_err(|_| EventQueryError::Digest)?;
    let rows_digest = *blake3::hash(&rows).as_bytes();
    let mut source_ids: Vec<[u8; 32]> = events.iter().map(|event| event.source_id).collect();
    source_ids.sort_unstable();
    source_ids.dedup();
    Ok(EventQueryReceipt {
        schema: EVENT_QUERY_RECEIPT_SCHEMA.to_string(),
        source_ids,
        events_digest: events_digest(events)?,
        query: query.clone(),
        moment_tick: moment_of(query)?,
        limits: *limits,
        scanned: result.scanned,
        matched: result.matched,
        rows: u64::try_from(result.rows.len()).map_err(|_| EventQueryError::CounterOverflow)?,
        truncated: result.truncated,
        rows_digest,
    })
}

#[derive(Serialize)]
struct EventSourceFingerprint<'a> {
    schema: &'a str,
    source_id: [u8; 32],
    source_kind: EventSourceKind,
    source_sequence: u64,
    tick: u64,
    vm_index: Option<u32>,
    process: Option<&'a str>,
    thread: Option<u32>,
    kind: &'a str,
    text: &'a str,
    payload_digest: [u8; 32],
    details: Option<&'a serde_json::Value>,
}

/// Digest every source-visible field in canonical source order.
///
/// `derived` is query-local and excluded so a prior projection cannot change
/// the receipt for otherwise identical inputs.
fn events_digest(events: &[EventEnvelope]) -> Result<[u8; 32], EventQueryError> {
    let mut ordered: Vec<&EventEnvelope> = events.iter().collect();
    ordered.sort_by_key(|event| (event.source_id, event.source_sequence));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chaoscontrol.event-query-source.v1\0");
    for event in ordered {
        let fingerprint = EventSourceFingerprint {
            schema: &event.schema,
            source_id: event.source_id,
            source_kind: event.source_kind,
            source_sequence: event.source_sequence,
            tick: event.tick,
            vm_index: event.vm_index,
            process: event.process.as_deref(),
            thread: event.thread,
            kind: &event.kind,
            text: &event.text,
            payload_digest: event.payload_digest,
            details: event.details.as_ref(),
        };
        let bytes = serde_json::to_vec(&fingerprint).map_err(|_| EventQueryError::Digest)?;
        let length = u64::try_from(bytes.len()).map_err(|_| EventQueryError::CounterOverflow)?;
        hasher.update(&length.to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Validate a receipt against the admitted query, sources, result, and limits.
///
/// Re-evaluates the query so a changed source, query, limit, row set, counter,
/// truncation fact, or digest is rejected rather than silently accepted.
pub fn validate_receipt(
    receipt: &EventQueryReceipt,
    query: &EventQuery,
    events: &[EventEnvelope],
    result: &EventQueryResult,
    limits: &EventQueryLimits,
) -> Result<(), EventQueryError> {
    if receipt.schema != EVENT_QUERY_RECEIPT_SCHEMA
        || receipt.query != *query
        || receipt.limits != *limits
    {
        return Err(EventQueryError::ReceiptDrift);
    }
    let fresh = evaluate(query, events, limits)?;
    if fresh != *result {
        return Err(EventQueryError::ResultDrift);
    }
    let source_ids: Vec<[u8; 32]> = events
        .iter()
        .map(|event| event.source_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if receipt.source_ids != source_ids || receipt.events_digest != events_digest(events)? {
        return Err(EventQueryError::SourceDrift);
    }
    if receipt_for(query, events, &fresh, limits)? != *receipt {
        return Err(EventQueryError::ReceiptDrift);
    }
    Ok(())
}

/// Evidence receipt for one rendered timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineReceipt {
    pub schema: String,
    pub format: String,
    pub nodes: u64,
    pub bytes: u64,
    pub facts_digest: [u8; 32],
    pub rendered_digest: [u8; 32],
}

/// Build a receipt by re-rendering the admitted facts in the declared format.
///
/// `format` is `markdown` or `dot`. The supplied output must equal the
/// canonical rendering, so the receipt binds the exact artifact content.
pub fn timeline_receipt(
    facts: &[TimelineFact],
    format: &str,
    rendered: &str,
) -> Result<TimelineReceipt, EventQueryError> {
    let ordered = ordered_timeline(facts)?;
    let expected = match format {
        "markdown" => render_timeline_markdown_ordered(&ordered)?,
        "dot" => render_timeline_dot_ordered(&ordered)?,
        _ => return Err(EventQueryError::InvalidFormat),
    };
    if expected != rendered {
        return Err(EventQueryError::TimelineReceiptDrift);
    }
    let facts_bytes = serde_json::to_vec(&ordered).map_err(|_| EventQueryError::InvalidEvent)?;
    Ok(TimelineReceipt {
        schema: TIMELINE_RECEIPT_SCHEMA.to_string(),
        format: format.to_string(),
        nodes: u64::try_from(ordered.len()).map_err(|_| EventQueryError::CounterOverflow)?,
        bytes: u64::try_from(rendered.len()).map_err(|_| EventQueryError::CounterOverflow)?,
        facts_digest: *blake3::hash(&facts_bytes).as_bytes(),
        rendered_digest: *blake3::hash(rendered.as_bytes()).as_bytes(),
    })
}

/// Validate a timeline receipt against its facts, format, and rendered output.
pub fn validate_timeline_receipt(
    receipt: &TimelineReceipt,
    facts: &[TimelineFact],
    format: &str,
    rendered: &str,
) -> Result<(), EventQueryError> {
    if *receipt != timeline_receipt(facts, format, rendered)? {
        return Err(EventQueryError::TimelineReceiptDrift);
    }
    Ok(())
}

/// Render a bounded static Markdown timeline.
pub fn render_timeline_markdown(facts: &[TimelineFact]) -> Result<String, EventQueryError> {
    render_timeline_markdown_ordered(&ordered_timeline(facts)?)
}

fn render_timeline_markdown_ordered(ordered: &[TimelineFact]) -> Result<String, EventQueryError> {
    let mut output =
        String::from("| tick | id | parent | kind | label |\n|---:|---:|---:|---|---|\n");
    for fact in ordered {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            fact.tick,
            fact.id,
            fact.parent
                .map_or_else(|| "-".to_string(), |p| p.to_string()),
            crate::text::escape_markdown_cell(&fact.kind),
            crate::text::escape_markdown_cell(&fact.label)
        ));
        if output.len() > MAX_TIMELINE_BYTES {
            return Err(EventQueryError::TimelineByteBound);
        }
    }
    Ok(output)
}

/// Escape DOT label delimiters so one fact cannot rewrite the graph shape.
fn escape_dot_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Render a bounded static DOT graph.
pub fn render_timeline_dot(facts: &[TimelineFact]) -> Result<String, EventQueryError> {
    render_timeline_dot_ordered(&ordered_timeline(facts)?)
}

fn render_timeline_dot_ordered(ordered: &[TimelineFact]) -> Result<String, EventQueryError> {
    let mut output = String::from("digraph chaoscontrol_timeline {\n");
    for fact in ordered {
        output.push_str(&format!(
            "  n{} [label=\"{}: {}\"];\n",
            fact.id,
            fact.tick,
            escape_dot_label(&fact.label)
        ));
        if output.len() > MAX_TIMELINE_BYTES {
            return Err(EventQueryError::TimelineByteBound);
        }
    }
    for fact in ordered {
        if let Some(parent) = fact.parent {
            output.push_str(&format!("  n{parent} -> n{};\n", fact.id));
        }
        if output.len() > MAX_TIMELINE_BYTES {
            return Err(EventQueryError::TimelineByteBound);
        }
    }
    output.push_str("}\n");
    Ok(output)
}

fn evaluate_rows(
    query: &EventQuery,
    events: &[EventEnvelope],
    operations: &mut usize,
    max_operations: usize,
    regexes: &RegexCache<'_>,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    match query {
        EventQuery::All => {
            charge_operations(operations, events.len(), max_operations)?;
            Ok(events.to_vec())
        }
        EventQuery::Filter { input, predicate } => {
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            rows.into_iter()
                .filter_map(|event| {
                    match predicate.matches(&event, operations, max_operations, regexes) {
                        Ok(true) => Some(Ok(event)),
                        Ok(false) => None,
                        Err(error) => Some(Err(error)),
                    }
                })
                .collect()
        }
        // r[impl chaoscontrol.running_count.prefixes]
        EventQuery::RunningCount {
            input,
            predicate,
            field,
        } => {
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for row in &rows {
                admit_event(row)?;
            }
            if !rows.is_sorted_by(|left, right| event_order(left) <= event_order(right)) {
                rows.sort_by_key(event_order);
            }
            rows.dedup_by_key(|row| (row.source_id, row.source_sequence));
            let mut count = 0_u64;
            for row in &mut rows {
                if predicate.matches(row, operations, max_operations, regexes)? {
                    count = count
                        .checked_add(1)
                        .ok_or(EventQueryError::CounterOverflow)?;
                }
                if let Some(value) = row.derived.get_mut(field) {
                    value.clear();
                    write!(value, "{count}").expect("writing to a String cannot fail");
                } else {
                    row.derived.insert(field.clone(), count.to_string());
                }
                admit_event(row)?;
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.running_sum.prefixes]
        EventQuery::RunningSum {
            input,
            predicate,
            field,
            value_field,
        } => {
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for row in &rows {
                admit_event(row)?;
            }
            if !rows.is_sorted_by(|left, right| event_order(left) <= event_order(right)) {
                rows.sort_by_key(event_order);
            }
            rows.dedup_by_key(|row| (row.source_id, row.source_sequence));
            let mut total = 0.0;
            for row in &mut rows {
                if predicate.matches(row, operations, max_operations, regexes)? {
                    total += finite_field_number(row, value_field)
                        .ok_or(EventQueryError::InvalidNumber)?;
                }
                let number =
                    serde_json::Number::from_f64(total).ok_or(EventQueryError::InvalidNumber)?;
                if let Some(value) = row.derived.get_mut(field) {
                    value.clear();
                    write!(value, "{number}").expect("writing to a String cannot fail");
                } else {
                    row.derived.insert(field.clone(), number.to_string());
                }
                admit_event(row)?;
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.running_extrema.prefixes]
        EventQuery::RunningMin {
            input,
            predicate,
            field,
            value_field,
        }
        | EventQuery::RunningMax {
            input,
            predicate,
            field,
            value_field,
        } => {
            let minimum = matches!(query, EventQuery::RunningMin { .. });
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for row in &rows {
                admit_event(row)?;
            }
            if !rows.is_sorted_by(|left, right| event_order(left) <= event_order(right)) {
                rows.sort_by_key(event_order);
            }
            rows.dedup_by_key(|row| (row.source_id, row.source_sequence));
            let mut extremum: Option<f64> = None;
            for row in &mut rows {
                if predicate.matches(row, operations, max_operations, regexes)? {
                    let operand = finite_field_number(row, value_field)
                        .ok_or(EventQueryError::InvalidNumber)?;
                    if extremum.is_none_or(|current| {
                        if minimum {
                            operand < current
                        } else {
                            operand > current
                        }
                    }) {
                        extremum = Some(operand);
                    }
                }
                if let Some(extremum) = extremum {
                    let number = serde_json::Number::from_f64(extremum)
                        .expect("selected numeric extrema are finite");
                    if let Some(value) = row.derived.get_mut(field) {
                        value.clear();
                        write!(value, "{number}").expect("writing to a String cannot fail");
                    } else {
                        row.derived.insert(field.clone(), number.to_string());
                    }
                } else {
                    row.derived.remove(field);
                }
                admit_event(row)?;
            }
            Ok(rows)
        }
        EventQuery::Map {
            input,
            field,
            pointer,
        } => {
            validate_field(field)?;
            validate_pointer(pointer)?;
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for event in &mut rows {
                let value = event
                    .details
                    .as_ref()
                    .and_then(|details| details.pointer(pointer))
                    .and_then(json_scalar);
                if let Some(value) = value {
                    event.derived.insert(field.clone(), value);
                }
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.json_text_projection.mapping]
        EventQuery::MapText {
            input,
            field,
            pointer,
        } => {
            validate_field(field)?;
            validate_pointer(pointer)?;
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for event in &mut rows {
                let mut parsed: serde_json::Value = serde_json::from_str(&event.text)
                    .map_err(|_| EventQueryError::InvalidJsonText)?;
                let value = parsed.pointer_mut(pointer).and_then(|value| match value {
                    serde_json::Value::String(value) => Some(std::mem::take(value)),
                    value => json_scalar(value),
                });
                if let Some(value) = value {
                    event.derived.insert(field.clone(), value);
                }
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.map_ratio.projection]
        EventQuery::MapRatio {
            input,
            field,
            numerator,
            denominator,
        } => {
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for event in &mut rows {
                admit_event(event)?;
                if let Some(number) = finite_field_ratio(event, numerator, denominator) {
                    if let Some(value) = event.derived.get_mut(field) {
                        value.clear();
                        write!(value, "{number}").expect("writing to a String cannot fail");
                    } else {
                        event.derived.insert(field.clone(), number.to_string());
                    }
                } else {
                    event.derived.remove(field);
                }
                admit_event(event)?;
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.map_difference.projection]
        EventQuery::MapDifference {
            input,
            field,
            minuend,
            subtrahend,
        } => {
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            for event in &mut rows {
                admit_event(event)?;
                let number = finite_field_number(event, minuend).and_then(|left| {
                    serde_json::Number::from_f64(left - finite_field_number(event, subtrahend)?)
                });
                if let Some(number) = number {
                    if let Some(value) = event.derived.get_mut(field) {
                        value.clear();
                        write!(value, "{number}").expect("writing to a String cannot fail");
                    } else {
                        event.derived.insert(field.clone(), number.to_string());
                    }
                } else {
                    event.derived.remove(field);
                }
                admit_event(event)?;
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.narrow_query.projections]
        EventQuery::Narrow { input, fields } => {
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            let fields: HashSet<_> = fields.iter().map(String::as_str).collect();
            for event in &mut rows {
                admit_event(event)?;
                event
                    .derived
                    .retain(|field, _| fields.contains(field.as_str()));
            }
            Ok(rows)
        }
        EventQuery::Contains { input, field, text } => {
            validate_field(field)?;
            validate_text(text)?;
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            Ok(rows
                .into_iter()
                .filter(|event| field_value(event, field).is_some_and(|value| value.contains(text)))
                .collect())
        }
        EventQuery::Matches { input, field, text } => {
            validate_field(field)?;
            validate_text(text)?;
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            Ok(rows
                .into_iter()
                .filter(|event| field_value(event, field) == Some(text.as_str()))
                .collect())
        }
        EventQuery::Union { inputs } => {
            let mut rows = Vec::new();
            let mut seen = HashSet::new();
            for input in inputs {
                let branch = evaluate_rows(input, events, operations, max_operations, regexes)?;
                charge_operations(operations, branch.len(), max_operations)?;
                for row in branch {
                    if seen.insert((row.source_id, row.source_sequence)) {
                        rows.push(row);
                    }
                }
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.distinct_tick_query.precedence]
        EventQuery::DistinctByTick { inputs } => {
            let mut rows = Vec::new();
            let mut seen = HashSet::new();
            for input in inputs {
                let branch = evaluate_rows(input, events, operations, max_operations, regexes)?;
                charge_operations(operations, branch.len(), max_operations)?;
                for row in branch {
                    admit_event(&row)?;
                    if seen.insert(row.tick) {
                        rows.push(row);
                    }
                }
            }
            Ok(rows)
        }
        // r[impl chaoscontrol.event_set_algebra.combination]
        EventQuery::Intersect { input, other } | EventQuery::Difference { input, other } => {
            let mut rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            let mut other = evaluate_rows(other, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            charge_operations(operations, other.len(), max_operations)?;
            for row in &other {
                admit_event(row)?;
            }
            other.sort_unstable_by(|left, right| {
                (&left.source_id, left.source_sequence)
                    .cmp(&(&right.source_id, right.source_sequence))
            });
            let intersect = matches!(query, EventQuery::Intersect { .. });
            rows.retain(|row| {
                other
                    .binary_search_by(|candidate| {
                        (&candidate.source_id, candidate.source_sequence)
                            .cmp(&(&row.source_id, row.source_sequence))
                    })
                    .is_ok()
                    == intersect
            });
            Ok(rows)
        }
        EventQuery::Temporal {
            input,
            witness,
            relation,
            within_ticks,
        } => {
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            let witnesses = evaluate_rows(witness, events, operations, max_operations, regexes)?;
            temporal_rows(
                rows,
                witnesses,
                *relation,
                *within_ticks,
                operations,
                max_operations,
            )
        }
        EventQuery::WithLastValue {
            input,
            witness,
            field,
            witness_field,
            within_ticks,
        } => {
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            let witnesses = evaluate_rows(witness, events, operations, max_operations, regexes)?;
            with_last_value_rows(
                rows,
                witnesses,
                field,
                witness_field,
                *within_ticks,
                operations,
                max_operations,
            )
        }
        EventQuery::Window {
            input,
            start_tick,
            end_tick,
        } => {
            if start_tick > end_tick {
                return Err(EventQueryError::InvalidWindow);
            }
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            Ok(rows
                .into_iter()
                .filter(|event| (*start_tick..=*end_tick).contains(&event.tick))
                .collect())
        }
        EventQuery::UpTo { input, moment_tick } => {
            let rows = evaluate_rows(input, events, operations, max_operations, regexes)?;
            charge_operations(operations, rows.len(), max_operations)?;
            Ok(rows
                .into_iter()
                .filter(|event| event.tick <= *moment_tick)
                .collect())
        }
    }
}

// r[impl chaoscontrol.last_witness_value.annotation]
fn with_last_value_rows(
    mut rows: Vec<EventEnvelope>,
    mut witnesses: Vec<EventEnvelope>,
    field: &str,
    witness_field: &str,
    within_ticks: u64,
    operations: &mut usize,
    max_operations: usize,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    charge_operations(operations, rows.len(), max_operations)?;
    charge_operations(operations, witnesses.len(), max_operations)?;
    for witness in &witnesses {
        admit_event(witness)?;
    }
    if !witnesses.is_sorted_by(|left, right| event_order(left) <= event_order(right)) {
        witnesses.sort_by_key(event_order);
    }
    witnesses.dedup_by_key(|witness| (witness.source_id, witness.source_sequence));
    for row in &mut rows {
        admit_event(row)?;
        let cursor = witnesses.partition_point(|witness| witness.tick < row.tick);
        let value = cursor
            .checked_sub(1)
            .map(|index| &witnesses[index])
            .filter(|witness| row.tick - witness.tick <= within_ticks)
            .and_then(|witness| field_value(witness, witness_field));
        if let Some(value) = value {
            if let Some(output) = row.derived.get_mut(field) {
                output.clear();
                output.push_str(value);
            } else {
                row.derived.insert(field.to_string(), value.to_string());
            }
        } else {
            row.derived.remove(field);
        }
        admit_event(row)?;
    }
    Ok(rows)
}

// r[impl chaoscontrol.temporal_event_query.correlation]
fn temporal_rows(
    mut rows: Vec<EventEnvelope>,
    mut witnesses: Vec<EventEnvelope>,
    relation: TemporalRelation,
    within_ticks: u64,
    operations: &mut usize,
    max_operations: usize,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    charge_operations(operations, rows.len(), max_operations)?;
    charge_operations(operations, witnesses.len(), max_operations)?;
    for witness in &witnesses {
        admit_event(witness)?;
    }
    witnesses.sort_unstable_by_key(|event| event.tick);
    let preceding = matches!(
        relation,
        TemporalRelation::PrecededBy | TemporalRelation::NotPrecededBy
    );
    let negative = matches!(
        relation,
        TemporalRelation::NotPrecededBy | TemporalRelation::NotFollowedBy
    );
    rows.retain(|event| {
        let cursor = witnesses.partition_point(|witness| {
            if preceding {
                witness.tick < event.tick
            } else {
                witness.tick <= event.tick
            }
        });
        let matched = if preceding {
            cursor.checked_sub(1).is_some_and(|index| {
                witnesses[index].tick >= event.tick.saturating_sub(within_ticks)
            })
        } else {
            witnesses
                .get(cursor)
                .is_some_and(|witness| witness.tick <= event.tick.saturating_add(within_ticks))
        };
        matched != negative
    });
    Ok(rows)
}

fn charge_operations(
    operations: &mut usize,
    amount: usize,
    max_operations: usize,
) -> Result<(), EventQueryError> {
    *operations = (*operations)
        .checked_add(amount)
        .ok_or(EventQueryError::CounterOverflow)?;
    if *operations > max_operations {
        return Err(EventQueryError::OperationBound);
    }
    Ok(())
}

impl EventPredicate {
    // r[impl chaoscontrol.compound_event_query.composition]
    fn matches(
        &self,
        event: &EventEnvelope,
        operations: &mut usize,
        max_operations: usize,
        regexes: &RegexCache<'_>,
    ) -> Result<bool, EventQueryError> {
        Ok(match self {
            Self::All(children) => {
                for child in children {
                    charge_operations(operations, 1, max_operations)?;
                    if !child.matches(event, operations, max_operations, regexes)? {
                        return Ok(false);
                    }
                }
                true
            }
            Self::Any(children) => {
                for child in children {
                    charge_operations(operations, 1, max_operations)?;
                    if child.matches(event, operations, max_operations, regexes)? {
                        return Ok(true);
                    }
                }
                false
            }
            Self::Not(child) => {
                charge_operations(operations, 1, max_operations)?;
                !child.matches(event, operations, max_operations, regexes)?
            }
            Self::TickRange {
                start_tick,
                end_tick,
            } => (*start_tick..=*end_tick).contains(&event.tick),
            Self::Vm(vm) => event.vm_index == Some(*vm),
            Self::SourceKind(kind) => event.source_kind == *kind,
            Self::Kind(kind) => event.kind == *kind,
            Self::Process(process) => event.process.as_deref() == Some(process),
            // r[impl chaoscontrol.text_event_predicate.selection]
            Self::Contains { field, text } => {
                field_value(event, field).is_some_and(|value| value.contains(text))
            }
            Self::Matches { field, text } => field_value(event, field) == Some(text.as_str()),
            // r[impl chaoscontrol.event_regex.selection]
            Self::Regex { field, pattern } => {
                let regex = regexes
                    .get(pattern.as_str())
                    .ok_or(EventQueryError::InvalidRegex)?;
                field_value(event, field).is_some_and(|value| regex.is_match(value))
            }
            // r[impl chaoscontrol.field_number.comparison]
            Self::FieldNumber {
                field,
                comparison,
                value,
            } => {
                let actual = field_value(event, field)
                    .and_then(|text| serde_json::from_str::<serde_json::Number>(text).ok())
                    .map(serde_json::Value::Number);
                comparison.matches(actual.as_ref(), value)
            }
            // r[impl chaoscontrol.field_ratio.comparison]
            Self::FieldRatio {
                numerator,
                denominator,
                comparison,
                value,
            } => {
                let actual = finite_field_ratio(event, numerator, denominator)
                    .map(serde_json::Value::Number);
                comparison.matches(actual.as_ref(), value)
            }
            // r[impl chaoscontrol.numeric_event_query.comparison]
            Self::JsonNumber {
                pointer,
                comparison,
                value,
            } => comparison.matches(
                event
                    .details
                    .as_ref()
                    .and_then(|details| details.pointer(pointer)),
                value,
            ),
            // r[impl chaoscontrol.json_text_number.comparison]
            Self::JsonTextNumber {
                pointer,
                comparison,
                value,
            } => {
                let parsed: serde_json::Value = serde_json::from_str(&event.text)
                    .map_err(|_| EventQueryError::InvalidJsonText)?;
                comparison.matches(parsed.pointer(pointer), value)
            }
            // r[impl chaoscontrol.json_field_presence.selection]
            Self::JsonPresent { pointer } => event
                .details
                .as_ref()
                .and_then(|details| details.pointer(pointer))
                .is_some(),
            Self::JsonTextPresent { pointer } => {
                let parsed: serde_json::Value = serde_json::from_str(&event.text)
                    .map_err(|_| EventQueryError::InvalidJsonText)?;
                parsed.pointer(pointer).is_some()
            }
        })
    }
}

fn validate_query<'a>(
    query: &'a EventQuery,
    limits: &EventQueryLimits,
) -> Result<RegexCache<'a>, EventQueryError> {
    let mut nodes = 0_usize;
    let mut compiled = RegexCache::new();
    let regexes = &mut compiled;
    validate_query_inner(query, limits, 0, &mut nodes, regexes)?;
    moment_of(query)?;
    Ok(compiled)
}

fn validate_query_inner<'a>(
    query: &'a EventQuery,
    limits: &EventQueryLimits,
    depth: usize,
    nodes: &mut usize,
    regexes: &mut RegexCache<'a>,
) -> Result<(), EventQueryError> {
    if depth > limits.max_depth {
        return Err(EventQueryError::QueryDepth);
    }
    *nodes = nodes
        .checked_add(1)
        .ok_or(EventQueryError::CounterOverflow)?;
    if *nodes > limits.max_nodes {
        return Err(EventQueryError::QueryBound);
    }
    match query {
        EventQuery::All => Ok(()),
        EventQuery::Filter { input, predicate } => {
            validate_predicate(predicate, limits, depth, nodes, regexes)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::RunningCount {
            input,
            predicate,
            field,
        } => {
            validate_field(field)?;
            validate_predicate(predicate, limits, depth, nodes, regexes)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::RunningSum {
            input,
            predicate,
            field,
            value_field,
        }
        | EventQuery::RunningMin {
            input,
            predicate,
            field,
            value_field,
        }
        | EventQuery::RunningMax {
            input,
            predicate,
            field,
            value_field,
        } => {
            validate_field(field)?;
            validate_field(value_field)?;
            validate_predicate(predicate, limits, depth, nodes, regexes)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::Map {
            input,
            field,
            pointer,
        }
        | EventQuery::MapText {
            input,
            field,
            pointer,
        } => {
            validate_field(field)?;
            validate_pointer(pointer)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::MapRatio {
            input,
            field,
            numerator,
            denominator,
        } => {
            validate_field(field)?;
            validate_field(numerator)?;
            validate_field(denominator)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::MapDifference {
            input,
            field,
            minuend,
            subtrahend,
        } => {
            validate_field(field)?;
            validate_field(minuend)?;
            validate_field(subtrahend)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::Narrow { input, fields } => {
            if fields.len() > MAX_DERIVED_FIELDS {
                return Err(EventQueryError::QueryBound);
            }
            for field in fields {
                validate_field(field)?;
            }
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::Contains { input, field, text }
        | EventQuery::Matches { input, field, text } => {
            validate_field(field)?;
            validate_text(text)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::Window {
            input,
            start_tick,
            end_tick,
        } => {
            if start_tick > end_tick {
                return Err(EventQueryError::InvalidWindow);
            }
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::UpTo { input, .. } => {
            validate_query_inner(input, limits, depth + 1, nodes, regexes)
        }
        EventQuery::WithLastValue {
            input,
            witness,
            field,
            witness_field,
            ..
        } => {
            validate_field(field)?;
            validate_field(witness_field)?;
            validate_query_inner(input, limits, depth + 1, nodes, regexes)?;
            validate_query_inner(witness, limits, depth + 1, nodes, regexes)
        }
        EventQuery::Temporal {
            input,
            witness: other,
            ..
        }
        | EventQuery::Intersect { input, other }
        | EventQuery::Difference { input, other } => {
            validate_query_inner(input, limits, depth + 1, nodes, regexes)?;
            validate_query_inner(other, limits, depth + 1, nodes, regexes)
        }
        EventQuery::Union { inputs } | EventQuery::DistinctByTick { inputs } => {
            if inputs.is_empty() {
                return Err(EventQueryError::QueryBound);
            }
            for input in inputs {
                validate_query_inner(input, limits, depth + 1, nodes, regexes)?;
            }
            Ok(())
        }
    }
}

fn validate_predicate<'a>(
    predicate: &'a EventPredicate,
    limits: &EventQueryLimits,
    depth: usize,
    nodes: &mut usize,
    regexes: &mut RegexCache<'a>,
) -> Result<(), EventQueryError> {
    // The enclosing query already accounts for the predicate root node.
    if depth > limits.max_depth {
        return Err(EventQueryError::QueryDepth);
    }
    match predicate {
        EventPredicate::All(children) | EventPredicate::Any(children) => {
            for child in children {
                validate_predicate_child(child, limits, depth + 1, nodes, regexes)?;
            }
            Ok(())
        }
        EventPredicate::Not(child) => {
            validate_predicate_child(child, limits, depth + 1, nodes, regexes)
        }
        EventPredicate::TickRange {
            start_tick,
            end_tick,
        } if start_tick > end_tick => Err(EventQueryError::InvalidPredicate),
        EventPredicate::Kind(kind) => {
            validate_text(kind).map_err(|_| EventQueryError::InvalidPredicate)
        }
        EventPredicate::Process(process) => {
            validate_text(process).map_err(|_| EventQueryError::InvalidPredicate)
        }
        EventPredicate::Contains { field, text } | EventPredicate::Matches { field, text } => {
            validate_field(field)?;
            validate_text(text)
        }
        EventPredicate::Regex { field, pattern } => {
            validate_field(field)?;
            validate_text(pattern)?;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                regexes.entry(pattern.as_str())
            {
                let regex = regex::RegexBuilder::new(pattern)
                    .size_limit(MAX_REGEX_PROGRAM_BYTES)
                    .dfa_size_limit(MAX_REGEX_DFA_BYTES)
                    .nest_limit(MAX_REGEX_NEST_DEPTH)
                    .build()
                    .map_err(|_| EventQueryError::InvalidRegex)?;
                entry.insert(regex);
            }
            Ok(())
        }
        EventPredicate::FieldNumber { field, value, .. } => {
            validate_field(field)?;
            if value.as_f64().is_none_or(|value| !value.is_finite()) {
                return Err(EventQueryError::InvalidPredicate);
            }
            Ok(())
        }
        EventPredicate::FieldRatio {
            numerator,
            denominator,
            value,
            ..
        } => {
            validate_field(numerator)?;
            validate_field(denominator)?;
            if value.as_f64().is_none_or(|value| !value.is_finite()) {
                return Err(EventQueryError::InvalidPredicate);
            }
            Ok(())
        }
        EventPredicate::JsonNumber { pointer, value, .. }
        | EventPredicate::JsonTextNumber { pointer, value, .. } => {
            validate_pointer(pointer)?;
            if value.as_f64().is_none_or(|value| !value.is_finite()) {
                return Err(EventQueryError::InvalidPredicate);
            }
            Ok(())
        }
        EventPredicate::JsonPresent { pointer } | EventPredicate::JsonTextPresent { pointer } => {
            validate_pointer(pointer)
        }
        EventPredicate::TickRange { .. }
        | EventPredicate::Vm(_)
        | EventPredicate::SourceKind(_) => Ok(()),
    }
}

fn validate_predicate_child<'a>(
    predicate: &'a EventPredicate,
    limits: &EventQueryLimits,
    depth: usize,
    nodes: &mut usize,
    regexes: &mut RegexCache<'a>,
) -> Result<(), EventQueryError> {
    *nodes = nodes
        .checked_add(1)
        .ok_or(EventQueryError::CounterOverflow)?;
    if *nodes > limits.max_nodes {
        return Err(EventQueryError::QueryBound);
    }
    validate_predicate(predicate, limits, depth, nodes, regexes)
}

fn validate_field(field: &str) -> Result<(), EventQueryError> {
    if field.is_empty() || field.len() > MAX_FIELD_BYTES || field.chars().any(char::is_control) {
        return Err(EventQueryError::InvalidField);
    }
    Ok(())
}

fn validate_pointer(pointer: &str) -> Result<(), EventQueryError> {
    if pointer.is_empty()
        || pointer.len() > MAX_FIELD_BYTES
        || !pointer.starts_with('/')
        || pointer.chars().any(char::is_control)
    {
        return Err(EventQueryError::InvalidPointer);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), EventQueryError> {
    if value.is_empty() || value.len() > MAX_EVENT_TEXT_BYTES || value.chars().any(char::is_control)
    {
        return Err(EventQueryError::InvalidText);
    }
    Ok(())
}

fn finite_field_number(event: &EventEnvelope, field: &str) -> Option<f64> {
    field_value(event, field)
        .and_then(|text| serde_json::from_str::<serde_json::Number>(text).ok())
        .and_then(|number| number.as_f64())
        .filter(|number| number.is_finite())
}

fn finite_field_ratio(
    event: &EventEnvelope,
    numerator: &str,
    denominator: &str,
) -> Option<serde_json::Number> {
    let denominator =
        finite_field_number(event, denominator).filter(|denominator| *denominator != 0.0)?;
    serde_json::Number::from_f64(finite_field_number(event, numerator)? / denominator)
}

fn field_value<'a>(event: &'a EventEnvelope, field: &str) -> Option<&'a str> {
    match field {
        "kind" => Some(event.kind.as_str()),
        "text" => Some(event.text.as_str()),
        "process" => event.process.as_deref(),
        "source_kind" => Some(event.source_kind.as_str()),
        _ => event.derived.get(field).map(String::as_str),
    }
}

fn json_scalar(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn event_order(event: &EventEnvelope) -> (u64, [u8; 32], u64) {
    (event.tick, event.source_id, event.source_sequence)
}

fn moment_of(query: &EventQuery) -> Result<Option<u64>, EventQueryError> {
    match query {
        EventQuery::UpTo { moment_tick, .. } => Ok(Some(*moment_tick)),
        EventQuery::Filter { input, .. }
        | EventQuery::RunningCount { input, .. }
        | EventQuery::RunningSum { input, .. }
        | EventQuery::RunningMin { input, .. }
        | EventQuery::RunningMax { input, .. }
        | EventQuery::Map { input, .. }
        | EventQuery::MapText { input, .. }
        | EventQuery::MapRatio { input, .. }
        | EventQuery::MapDifference { input, .. }
        | EventQuery::Narrow { input, .. }
        | EventQuery::Contains { input, .. }
        | EventQuery::Matches { input, .. }
        | EventQuery::Window { input, .. } => moment_of(input),
        EventQuery::Union { inputs } | EventQuery::DistinctByTick { inputs } => {
            let mut moment = None;
            for input in inputs {
                let Some(candidate) = moment_of(input)? else {
                    continue;
                };
                if moment.is_some_and(|existing| existing != candidate) {
                    return Err(EventQueryError::AmbiguousMoment);
                }
                moment = Some(candidate);
            }
            Ok(moment)
        }
        EventQuery::Temporal {
            input,
            witness: other,
            ..
        }
        | EventQuery::WithLastValue {
            input,
            witness: other,
            ..
        }
        | EventQuery::Intersect { input, other }
        | EventQuery::Difference { input, other } => {
            let input = moment_of(input)?;
            let other = moment_of(other)?;
            if matches!((input, other), (Some(left), Some(right)) if left != right) {
                return Err(EventQueryError::AmbiguousMoment);
            }
            Ok(input.or(other))
        }
        EventQuery::All => Ok(None),
    }
}

fn ordered_timeline(facts: &[TimelineFact]) -> Result<Vec<TimelineFact>, EventQueryError> {
    if facts.len() > MAX_TIMELINE_NODES {
        return Err(EventQueryError::TimelineBound);
    }
    let mut by_id = BTreeMap::new();
    for fact in facts {
        validate_text(&fact.kind)?;
        validate_text(&fact.label)?;
        if by_id.insert(fact.id, fact.clone()).is_some() {
            return Err(EventQueryError::DuplicateTimelineId);
        }
    }
    let mut complete = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for fact in facts {
        if let Some(parent) = fact.parent {
            if !by_id.contains_key(&parent) {
                return Err(EventQueryError::MissingTimelineParent);
            }
        }
        if complete.contains(&fact.id) {
            continue;
        }
        let mut path = Vec::new();
        let mut current = Some(fact.id);
        while let Some(id) = current {
            if complete.contains(&id) {
                break;
            }
            if !visiting.insert(id) {
                return Err(EventQueryError::TimelineCycle);
            }
            path.push(id);
            current = by_id.get(&id).and_then(|node| node.parent);
        }
        for id in path {
            visiting.remove(&id);
            complete.insert(id);
        }
    }
    let mut ordered: Vec<_> = by_id.into_values().collect();
    ordered.sort_by_key(|fact| (fact.tick, fact.id));
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        source: u8,
        sequence: u64,
        tick: u64,
        kind: &str,
        text: &str,
        details: Option<serde_json::Value>,
    ) -> EventEnvelope {
        EventEnvelope {
            schema: EVENT_ENVELOPE_SCHEMA.to_string(),
            source_id: [source; 32],
            source_kind: EventSourceKind::Recording,
            source_sequence: sequence,
            tick,
            vm_index: Some(u32::from(source)),
            process: Some("guest".to_string()),
            thread: None,
            kind: kind.to_string(),
            text: text.to_string(),
            payload_digest: [source; 32],
            details,
            derived: BTreeMap::new(),
        }
    }

    fn events() -> Vec<EventEnvelope> {
        vec![
            event(
                1,
                0,
                10,
                "assert",
                "leader elected",
                Some(serde_json::json!({"term": 1})),
            ),
            event(1, 1, 20, "fault", "network partition", None),
            event(
                2,
                0,
                15,
                "assert",
                "value committed",
                Some(serde_json::json!({"value": 7})),
            ),
            event(2, 1, 30, "serial", "shutdown", None),
        ]
    }

    #[test]
    fn filters_are_deterministic_by_tick_and_source() {
        let query = EventQuery::Filter {
            input: Box::new(EventQuery::All),
            predicate: EventPredicate::Kind("assert".to_string()),
        };
        let result = evaluate(&query, &events(), &EventQueryLimits::default()).unwrap();
        assert_eq!(result.matched, 2);
        assert_eq!(result.rows[0].text, "leader elected");
        assert_eq!(result.rows[1].text, "value committed");
    }

    #[test]
    fn contains_and_matches_use_declared_fields() {
        let contains = EventQuery::Contains {
            input: Box::new(EventQuery::All),
            field: "text".to_string(),
            text: "partition".to_string(),
        };
        assert_eq!(
            evaluate(&contains, &events(), &EventQueryLimits::default())
                .unwrap()
                .matched,
            1
        );
        let matches = EventQuery::Matches {
            input: Box::new(EventQuery::All),
            field: "kind".to_string(),
            text: "serial".to_string(),
        };
        assert_eq!(
            evaluate(&matches, &events(), &EventQueryLimits::default())
                .unwrap()
                .matched,
            1
        );
    }

    #[test]
    fn union_deduplicates_and_orders() {
        let query = EventQuery::Union {
            inputs: vec![EventQuery::All, EventQuery::All],
        };
        let result = evaluate(&query, &events(), &EventQueryLimits::default()).unwrap();
        assert_eq!(result.matched, 4);
        assert_eq!(result.rows[0].tick, 10);
        assert_eq!(result.rows[3].tick, 30);
    }

    #[test]
    fn conflicting_source_positions_are_rejected() {
        let admitted = events();
        let mut conflicting = admitted.clone();
        conflicting.push(event(1, 0, 10, "assert", "different text", None));
        assert_eq!(
            evaluate(&EventQuery::All, &conflicting, &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::DuplicateSourcePosition
        );

        let mut shifted_tick = admitted.clone();
        shifted_tick.push(event(1, 0, 99, "assert", "leader elected", None));
        assert_eq!(
            evaluate(
                &EventQuery::All,
                &shifted_tick,
                &EventQueryLimits::default()
            )
            .unwrap_err(),
            EventQueryError::DuplicateSourcePosition
        );

        let mut identical = admitted.clone();
        let mut repeat = identical[0].clone();
        repeat
            .derived
            .insert("projection".to_string(), "x".to_string());
        identical.push(repeat);
        let result = evaluate(&EventQuery::All, &identical, &EventQueryLimits::default()).unwrap();
        assert_eq!(result.matched, 4);
    }

    #[test]
    fn windows_and_up_to_bound_the_horizon() {
        let query = EventQuery::Window {
            input: Box::new(EventQuery::All),
            start_tick: 12,
            end_tick: 20,
        };
        assert_eq!(
            evaluate(&query, &events(), &EventQueryLimits::default())
                .unwrap()
                .matched,
            2
        );
        let query = EventQuery::UpTo {
            input: Box::new(EventQuery::All),
            moment_tick: 15,
        };
        assert_eq!(
            evaluate(&query, &events(), &EventQueryLimits::default())
                .unwrap()
                .matched,
            2
        );
    }

    #[test]
    fn derived_fields_are_bounded() {
        let mut event = event(
            1,
            0,
            10,
            "assert",
            "leader elected",
            Some(serde_json::json!({"value": "x".repeat(40_000)})),
        );
        let mapped = EventQuery::Map {
            input: Box::new(EventQuery::Map {
                input: Box::new(EventQuery::All),
                field: "first".to_string(),
                pointer: "/value".to_string(),
            }),
            field: "second".to_string(),
            pointer: "/value".to_string(),
        };
        assert_eq!(
            evaluate(
                &mapped,
                std::slice::from_ref(&event),
                &EventQueryLimits::default()
            )
            .unwrap_err(),
            EventQueryError::InvalidEvent
        );

        event.derived.clear();
        for index in 0..MAX_DERIVED_FIELDS {
            event
                .derived
                .insert(format!("field-{index}"), String::new());
        }
        assert!(evaluate(
            &EventQuery::All,
            &[event.clone()],
            &EventQueryLimits::default()
        )
        .is_ok());
        event
            .derived
            .insert(format!("field-{MAX_DERIVED_FIELDS}"), String::new());
        assert_eq!(
            evaluate(&EventQuery::All, &[event], &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidEvent
        );
    }

    #[test]
    fn union_moments_must_agree() {
        let limits = EventQueryLimits::default();
        let same = EventQuery::Union {
            inputs: vec![
                EventQuery::UpTo {
                    input: Box::new(EventQuery::All),
                    moment_tick: 5,
                },
                EventQuery::UpTo {
                    input: Box::new(EventQuery::Filter {
                        input: Box::new(EventQuery::All),
                        predicate: EventPredicate::Kind("assert".to_string()),
                    }),
                    moment_tick: 5,
                },
            ],
        };
        let result = evaluate(&same, &events(), &limits).unwrap();
        assert_eq!(result.matched, 0);
        let receipt = receipt_for(&same, &events(), &result, &limits).unwrap();
        assert_eq!(receipt.moment_tick, Some(5));

        let mixed = EventQuery::Union {
            inputs: vec![
                EventQuery::UpTo {
                    input: Box::new(EventQuery::All),
                    moment_tick: 5,
                },
                EventQuery::UpTo {
                    input: Box::new(EventQuery::All),
                    moment_tick: 9,
                },
            ],
        };
        assert_eq!(
            evaluate(&mixed, &events(), &limits).unwrap_err(),
            EventQueryError::AmbiguousMoment
        );
    }

    #[test]
    fn map_derives_scalars_from_details() {
        let query = EventQuery::Map {
            input: Box::new(EventQuery::All),
            field: "term".to_string(),
            pointer: "/term".to_string(),
        };
        let result = evaluate(&query, &events(), &EventQueryLimits::default()).unwrap();
        assert_eq!(
            result.rows[0].derived.get("term").map(String::as_str),
            Some("1")
        );
        assert!(!result.rows[1].derived.contains_key("term"));
    }

    #[test]
    fn bounds_and_invalid_fields_fail_closed() {
        let query = EventQuery::All;
        let limits = EventQueryLimits {
            max_scanned_events: 2,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&query, &events(), &limits).unwrap_err(),
            EventQueryError::ScanBound
        );
        let query = EventQuery::Contains {
            input: Box::new(EventQuery::All),
            field: "".to_string(),
            text: "x".to_string(),
        };
        assert_eq!(
            evaluate(&query, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidField
        );
        let unknown_field = EventQuery::Contains {
            input: Box::new(EventQuery::All),
            field: "not_derived".to_string(),
            text: "x".to_string(),
        };
        assert_eq!(
            evaluate(&unknown_field, &events(), &EventQueryLimits::default())
                .unwrap()
                .matched,
            0
        );

        let reversed_range = EventQuery::Filter {
            input: Box::new(EventQuery::All),
            predicate: EventPredicate::TickRange {
                start_tick: 20,
                end_tick: 10,
            },
        };
        assert_eq!(
            evaluate(&reversed_range, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidPredicate
        );
        let empty_kind = EventQuery::Filter {
            input: Box::new(EventQuery::All),
            predicate: EventPredicate::Kind(String::new()),
        };
        assert_eq!(
            evaluate(&empty_kind, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidPredicate
        );
        let control_process = EventQuery::Filter {
            input: Box::new(EventQuery::All),
            predicate: EventPredicate::Process("guest\n".to_string()),
        };
        assert_eq!(
            evaluate(&control_process, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidPredicate
        );

        let nested = EventQuery::Filter {
            input: Box::new(EventQuery::Filter {
                input: Box::new(EventQuery::All),
                predicate: EventPredicate::Kind("assert".to_string()),
            }),
            predicate: EventPredicate::Kind("assert".to_string()),
        };
        let shallow = EventQueryLimits {
            max_depth: 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&nested, &events(), &shallow).unwrap_err(),
            EventQueryError::QueryDepth
        );
        let over_bound = EventQueryLimits {
            max_depth: MAX_QUERY_DEPTH + 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events(), &over_bound).unwrap_err(),
            EventQueryError::QueryBound
        );
        let decoded: EventQueryLimits = serde_json::from_str(
            r#"{"max_nodes":256,"max_scanned_events":100,"max_result_rows":10}"#,
        )
        .unwrap();
        assert_eq!(decoded.max_depth, MAX_QUERY_DEPTH);
        assert_eq!(decoded.max_result_bytes, MAX_RESULT_BYTES);
        assert_eq!(decoded.max_operations, MAX_QUERY_OPERATIONS);

        let narrow_operations = EventQueryLimits {
            max_operations: 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events(), &narrow_operations).unwrap_err(),
            EventQueryError::OperationBound
        );
        let over_bound_operations = EventQueryLimits {
            max_operations: MAX_QUERY_OPERATIONS + 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events(), &over_bound_operations).unwrap_err(),
            EventQueryError::QueryBound
        );
        let narrow_result = EventQueryLimits {
            max_result_bytes: 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events(), &narrow_result).unwrap_err(),
            EventQueryError::ResultBound
        );
        let empty_window = EventQuery::Window {
            input: Box::new(EventQuery::All),
            start_tick: u64::MAX,
            end_tick: u64::MAX,
        };
        assert_eq!(
            evaluate(&empty_window, &events(), &narrow_result).unwrap_err(),
            EventQueryError::ResultBound
        );
        let exact_empty = EventQueryLimits {
            max_result_bytes: 2,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&empty_window, &events(), &exact_empty)
                .unwrap()
                .matched,
            0
        );
        let over_bound_result = EventQueryLimits {
            max_result_bytes: MAX_RESULT_BYTES + 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events(), &over_bound_result).unwrap_err(),
            EventQueryError::QueryBound
        );
    }

    #[test]
    fn depth_bound_admits_the_exact_maximum() {
        let wrap = |input| EventQuery::Window {
            input: Box::new(input),
            start_tick: 0,
            end_tick: u64::MAX,
        };
        let mut exact = EventQuery::All;
        for _ in 0..MAX_QUERY_DEPTH {
            exact = wrap(exact);
        }
        evaluate(&exact, &events(), &EventQueryLimits::default()).unwrap();

        assert_eq!(
            evaluate(&wrap(exact), &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::QueryDepth
        );
    }

    #[test]
    fn node_bound_admits_the_exact_maximum() {
        let exact = EventQuery::Union {
            inputs: vec![EventQuery::All; MAX_QUERY_NODES - 1],
        };
        let limits = EventQueryLimits {
            max_nodes: MAX_QUERY_NODES,
            ..EventQueryLimits::default()
        };
        let result = evaluate(&exact, &events(), &limits).unwrap();
        assert_eq!(result.matched, events().len() as u64);
        assert_eq!(result.rows.len(), events().len());

        let over = EventQuery::Union {
            inputs: vec![EventQuery::All; MAX_QUERY_NODES],
        };
        assert_eq!(
            evaluate(&over, &events(), &limits).unwrap_err(),
            EventQueryError::QueryBound
        );
    }

    #[test]
    fn timeline_renders_and_rejects_cycles() {
        let facts = vec![
            TimelineFact {
                id: 1,
                parent: None,
                tick: 0,
                kind: "root".to_string(),
                label: "root".to_string(),
            },
            TimelineFact {
                id: 2,
                parent: Some(1),
                tick: 5,
                kind: "branch".to_string(),
                label: "child".to_string(),
            },
        ];
        let markdown = render_timeline_markdown(&facts).unwrap();
        assert!(markdown.contains("| 5 | 2 | 1 | branch | child |"));
        let dot = render_timeline_dot(&facts).unwrap();
        assert!(dot.contains("n1 -> n2"));
        let cyclic = vec![
            TimelineFact {
                id: 1,
                parent: Some(2),
                tick: 0,
                kind: "a".to_string(),
                label: "a".to_string(),
            },
            TimelineFact {
                id: 2,
                parent: Some(1),
                tick: 1,
                kind: "b".to_string(),
                label: "b".to_string(),
            },
        ];
        assert_eq!(
            render_timeline_markdown(&cyclic).unwrap_err(),
            EventQueryError::TimelineCycle
        );

        let duplicate = vec![
            TimelineFact {
                id: 1,
                parent: None,
                tick: 0,
                kind: "a".to_string(),
                label: "a".to_string(),
            },
            TimelineFact {
                id: 1,
                parent: None,
                tick: 1,
                kind: "b".to_string(),
                label: "b".to_string(),
            },
        ];
        assert_eq!(
            render_timeline_markdown(&duplicate).unwrap_err(),
            EventQueryError::DuplicateTimelineId
        );

        let missing_parent = vec![TimelineFact {
            id: 1,
            parent: Some(2),
            tick: 0,
            kind: "a".to_string(),
            label: "a".to_string(),
        }];
        assert_eq!(
            render_timeline_dot(&missing_parent).unwrap_err(),
            EventQueryError::MissingTimelineParent
        );
    }

    #[test]
    fn query_limits_enforce_each_named_bound() {
        EventQueryLimits::default().validate().unwrap();
        let over = [
            EventQueryLimits {
                max_nodes: MAX_QUERY_NODES + 1,
                ..EventQueryLimits::default()
            },
            EventQueryLimits {
                max_depth: MAX_QUERY_DEPTH + 1,
                ..EventQueryLimits::default()
            },
            EventQueryLimits {
                max_scanned_events: MAX_SCANNED_EVENTS + 1,
                ..EventQueryLimits::default()
            },
            EventQueryLimits {
                max_result_rows: MAX_RESULT_ROWS + 1,
                ..EventQueryLimits::default()
            },
            EventQueryLimits {
                max_result_bytes: MAX_RESULT_BYTES + 1,
                ..EventQueryLimits::default()
            },
            EventQueryLimits {
                max_operations: MAX_QUERY_OPERATIONS + 1,
                ..EventQueryLimits::default()
            },
        ];
        for limits in over {
            assert_eq!(limits.validate().unwrap_err(), EventQueryError::QueryBound);
        }
    }

    #[test]
    fn event_details_enforce_the_serialized_byte_boundary() {
        let mut event = events()[0].clone();
        event.details = Some(serde_json::Value::String(
            "x".repeat(MAX_EVENT_DETAILS_BYTES - 2),
        ));
        admit_event(&event).unwrap();

        event.details = Some(serde_json::Value::String(
            "x".repeat(MAX_EVENT_DETAILS_BYTES - 1),
        ));
        assert_eq!(
            admit_event(&event).unwrap_err(),
            EventQueryError::InvalidEvent
        );
    }

    #[test]
    fn operation_budget_admits_the_exact_scan() {
        let events = events();
        let exact = EventQueryLimits {
            max_operations: events.len(),
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events, &exact).unwrap().matched,
            events.len() as u64
        );

        let tight = EventQueryLimits {
            max_operations: events.len() - 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events, &tight).unwrap_err(),
            EventQueryError::OperationBound
        );
    }

    #[test]
    fn derived_bytes_enforce_the_byte_boundary() {
        let mut event = events()[0].clone();
        event.derived.clear();
        event
            .derived
            .insert("a".repeat(MAX_DERIVED_BYTES), String::new());
        admit_event(&event).unwrap();

        event.derived.clear();
        event
            .derived
            .insert("a".repeat(MAX_DERIVED_BYTES + 1), String::new());
        assert_eq!(
            admit_event(&event).unwrap_err(),
            EventQueryError::InvalidEvent
        );
    }

    #[test]
    fn query_field_and_pointer_enforce_the_byte_boundary() {
        let field = "a".repeat(MAX_FIELD_BYTES);
        let exact_field = EventQuery::Map {
            input: Box::new(EventQuery::All),
            field,
            pointer: "/x".to_string(),
        };
        evaluate(&exact_field, &events(), &EventQueryLimits::default()).unwrap();

        let over_field = EventQuery::Map {
            input: Box::new(EventQuery::All),
            field: "a".repeat(MAX_FIELD_BYTES + 1),
            pointer: "/x".to_string(),
        };
        assert_eq!(
            evaluate(&over_field, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidField
        );

        let pointer = format!("/{}", "a".repeat(MAX_FIELD_BYTES - 1));
        let exact_pointer = EventQuery::Map {
            input: Box::new(EventQuery::All),
            field: "x".to_string(),
            pointer,
        };
        evaluate(&exact_pointer, &events(), &EventQueryLimits::default()).unwrap();

        let over_pointer = EventQuery::Map {
            input: Box::new(EventQuery::All),
            field: "x".to_string(),
            pointer: format!("/{}", "a".repeat(MAX_FIELD_BYTES)),
        };
        assert_eq!(
            evaluate(&over_pointer, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::InvalidPointer
        );
    }

    #[test]
    fn event_text_enforces_the_byte_boundary() {
        let mut event = events()[0].clone();
        event.text = "a".repeat(MAX_EVENT_TEXT_BYTES);
        admit_event(&event).unwrap();

        event.text.push('a');
        assert_eq!(
            admit_event(&event).unwrap_err(),
            EventQueryError::InvalidText
        );
    }

    #[test]
    fn empty_union_is_rejected() {
        let query = EventQuery::Union { inputs: Vec::new() };
        assert_eq!(
            evaluate(&query, &events(), &EventQueryLimits::default()).unwrap_err(),
            EventQueryError::QueryBound
        );
    }

    #[test]
    fn union_keeps_the_first_projected_duplicate() {
        let event = event(
            1,
            0,
            10,
            "assert",
            "leader elected",
            Some(serde_json::json!({"value": 7})),
        );
        let map = |field: &str| EventQuery::Map {
            input: Box::new(EventQuery::All),
            field: field.to_string(),
            pointer: "/value".to_string(),
        };
        let query = EventQuery::Union {
            inputs: vec![map("first"), map("second")],
        };
        let result = evaluate(
            &query,
            std::slice::from_ref(&event),
            &EventQueryLimits::default(),
        )
        .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0].derived.get("first").map(String::as_str),
            Some("7")
        );
        assert!(!result.rows[0].derived.contains_key("second"));
    }

    #[test]
    fn union_charges_only_new_rows() {
        let query = EventQuery::Union {
            inputs: vec![EventQuery::All; 2],
        };
        let exact = EventQueryLimits {
            max_operations: 16,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&query, &events(), &exact).unwrap().matched,
            events().len() as u64
        );

        let tight = EventQueryLimits {
            max_operations: 15,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&query, &events(), &tight).unwrap_err(),
            EventQueryError::OperationBound
        );
    }

    #[test]
    fn scan_and_row_bounds_admit_the_exact_maximum() {
        let events = events();
        let count = events.len();
        let exact_scan = EventQueryLimits {
            max_scanned_events: count,
            max_result_rows: count,
            ..EventQueryLimits::default()
        };
        let result = evaluate(&EventQuery::All, &events, &exact_scan).unwrap();
        assert_eq!(result.rows.len(), count);
        assert!(!result.truncated);

        let narrow_scan = EventQueryLimits {
            max_scanned_events: count - 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&EventQuery::All, &events, &narrow_scan).unwrap_err(),
            EventQueryError::ScanBound
        );

        let narrow_rows = EventQueryLimits {
            max_result_rows: count - 1,
            ..EventQueryLimits::default()
        };
        let result = evaluate(&EventQuery::All, &events, &narrow_rows).unwrap();
        assert!(result.truncated);
        assert_eq!(result.rows.len(), count - 1);
    }

    #[test]
    fn timeline_cycles_are_detected_after_completed_paths() {
        let facts = vec![
            TimelineFact {
                id: 1,
                parent: None,
                tick: 0,
                kind: "k".to_string(),
                label: "v".to_string(),
            },
            TimelineFact {
                id: 2,
                parent: Some(1),
                tick: 1,
                kind: "k".to_string(),
                label: "v".to_string(),
            },
            TimelineFact {
                id: 3,
                parent: Some(4),
                tick: 2,
                kind: "k".to_string(),
                label: "v".to_string(),
            },
            TimelineFact {
                id: 4,
                parent: Some(3),
                tick: 3,
                kind: "k".to_string(),
                label: "v".to_string(),
            },
        ];
        assert_eq!(
            render_timeline_markdown(&facts).unwrap_err(),
            EventQueryError::TimelineCycle
        );
    }

    #[test]
    fn timeline_node_bound_admits_the_exact_maximum() {
        let mut facts: Vec<_> = (0..MAX_TIMELINE_NODES)
            .map(|index| TimelineFact {
                id: index as u64 + 1,
                parent: (index > 0).then_some(index as u64),
                tick: index as u64,
                kind: "k".to_string(),
                label: "v".to_string(),
            })
            .collect();
        assert!(render_timeline_markdown(&facts).is_ok());

        facts.push(TimelineFact {
            id: MAX_TIMELINE_NODES as u64 + 1,
            parent: Some(MAX_TIMELINE_NODES as u64),
            tick: MAX_TIMELINE_NODES as u64,
            kind: "k".to_string(),
            label: "v".to_string(),
        });
        assert_eq!(
            render_timeline_markdown(&facts).unwrap_err(),
            EventQueryError::TimelineBound
        );
    }

    #[test]
    fn timeline_receipt_binds_canonical_facts_and_output() {
        let facts = vec![
            TimelineFact {
                id: 1,
                parent: None,
                tick: 0,
                kind: "root".to_string(),
                label: "root".to_string(),
            },
            TimelineFact {
                id: 2,
                parent: Some(1),
                tick: 5,
                kind: "branch".to_string(),
                label: "child".to_string(),
            },
        ];
        let markdown = render_timeline_markdown(&facts).unwrap();
        let receipt = timeline_receipt(&facts, "markdown", &markdown).unwrap();
        assert_eq!(receipt.schema, TIMELINE_RECEIPT_SCHEMA);
        assert_eq!(receipt.format, "markdown");
        assert_eq!(receipt.nodes, 2);
        assert_eq!(receipt.bytes, u64::try_from(markdown.len()).unwrap());
        validate_timeline_receipt(&receipt, &facts, "markdown", &markdown).unwrap();

        let mut reversed = facts.clone();
        reversed.reverse();
        assert_eq!(
            timeline_receipt(&reversed, "markdown", &markdown).unwrap(),
            receipt
        );

        let tampered = markdown.replace("root", "other");
        assert_eq!(
            validate_timeline_receipt(&receipt, &facts, "markdown", &tampered).unwrap_err(),
            EventQueryError::TimelineReceiptDrift
        );
        assert_eq!(
            timeline_receipt(&facts, "dot", &markdown).unwrap_err(),
            EventQueryError::TimelineReceiptDrift
        );
        assert_eq!(
            timeline_receipt(&facts, "yaml", &markdown).unwrap_err(),
            EventQueryError::InvalidFormat
        );
    }

    #[test]
    fn timeline_rejects_output_over_the_byte_bound() {
        let label = "x".repeat(MAX_EVENT_TEXT_BYTES);
        let count = MAX_TIMELINE_BYTES / MAX_EVENT_TEXT_BYTES + 1;
        assert!(count <= MAX_TIMELINE_NODES);
        let facts = (0..count)
            .map(|id| TimelineFact {
                id: u64::try_from(id).unwrap(),
                parent: None,
                tick: 0,
                kind: "kind".to_string(),
                label: label.clone(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            render_timeline_markdown(&facts).unwrap_err(),
            EventQueryError::TimelineByteBound
        );
        assert_eq!(
            render_timeline_dot(&facts).unwrap_err(),
            EventQueryError::TimelineByteBound
        );
    }

    #[test]
    fn receipt_construction_rejects_invalid_events() {
        let events = events();
        let limits = EventQueryLimits::default();
        let result = evaluate(&EventQuery::All, &events, &limits).unwrap();

        let mut conflicting = events.clone();
        conflicting.push(event(1, 0, 10, "other", "duplicate", None));
        assert_eq!(
            receipt_for(&EventQuery::All, &conflicting, &result, &limits).unwrap_err(),
            EventQueryError::DuplicateSourcePosition
        );

        let mut invalid = events.clone();
        invalid[0].schema = String::from("not-the-schema");
        assert_eq!(
            receipt_for(&EventQuery::All, &invalid, &result, &limits).unwrap_err(),
            EventQueryError::InvalidEvent
        );
    }

    #[test]
    fn receipt_construction_rejects_inconsistent_results() {
        let events = events();
        let limits = EventQueryLimits::default();
        let result = evaluate(&EventQuery::All, &events, &limits).unwrap();

        let mut bad_schema = result.clone();
        bad_schema.schema = String::from("other");
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &bad_schema, &limits).unwrap_err(),
            EventQueryError::InvalidResult
        );

        let mut bad_scan = result.clone();
        bad_scan.scanned += 1;
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &bad_scan, &limits).unwrap_err(),
            EventQueryError::InvalidResult
        );

        let narrow_rows = EventQueryLimits {
            max_result_rows: 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &result, &narrow_rows).unwrap_err(),
            EventQueryError::RowBound
        );

        let mut inconsistent = result.clone();
        inconsistent.matched += 1;
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &inconsistent, &limits).unwrap_err(),
            EventQueryError::InvalidResult
        );

        let narrow_scan = EventQueryLimits {
            max_scanned_events: 1,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &result, &narrow_scan).unwrap_err(),
            EventQueryError::ScanBound
        );

        let reversed = EventQuery::Window {
            input: Box::new(EventQuery::All),
            start_tick: 10,
            end_tick: 5,
        };
        assert_eq!(
            evaluate(&reversed, &events, &limits).unwrap_err(),
            EventQueryError::InvalidWindow
        );
        assert_eq!(
            receipt_for(&reversed, &events, &result, &limits).unwrap_err(),
            EventQueryError::InvalidWindow
        );
        let invalid_field = EventQuery::Contains {
            input: Box::new(EventQuery::All),
            field: String::new(),
            text: String::from("x"),
        };
        assert_eq!(
            receipt_for(&invalid_field, &events, &result, &limits).unwrap_err(),
            EventQueryError::InvalidField
        );

        let narrow_rows = EventQueryLimits {
            max_result_rows: 1,
            ..EventQueryLimits::default()
        };
        let truncated = evaluate(&EventQuery::All, &events, &narrow_rows).unwrap();
        assert!(truncated.truncated);
        let mut impossible = truncated.clone();
        impossible.matched = impossible.scanned + 1;
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &impossible, &narrow_rows).unwrap_err(),
            EventQueryError::InvalidResult
        );

        let mut unsorted = result.clone();
        unsorted.rows.swap(0, 1);
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &unsorted, &limits).unwrap_err(),
            EventQueryError::InvalidResult
        );

        let mut duplicated = result.clone();
        duplicated.rows = vec![result.rows[0].clone(), result.rows[0].clone()];
        duplicated.matched = 2;
        assert_eq!(
            receipt_for(&EventQuery::All, &events, &duplicated, &limits).unwrap_err(),
            EventQueryError::InvalidResult
        );
    }

    #[test]
    fn receipt_validation_rejects_drift() {
        let events = events();
        let query = EventQuery::Filter {
            input: Box::new(EventQuery::All),
            predicate: EventPredicate::Kind("assert".to_string()),
        };
        let limits = EventQueryLimits::default();
        let result = evaluate(&query, &events, &limits).unwrap();
        let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
        validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();

        let mut tampered = receipt.clone();
        tampered.rows += 1;
        assert_eq!(
            validate_receipt(&tampered, &query, &events, &result, &limits).unwrap_err(),
            EventQueryError::ReceiptDrift
        );

        let mut drifted = receipt.clone();
        drifted.source_ids.push([9; 32]);
        assert_eq!(
            validate_receipt(&drifted, &query, &events, &result, &limits).unwrap_err(),
            EventQueryError::SourceDrift
        );

        let mut changed_source = events.clone();
        changed_source[1].text = String::from("changed after the query");
        assert_eq!(
            validate_receipt(&receipt, &query, &changed_source, &result, &limits).unwrap_err(),
            EventQueryError::SourceDrift
        );

        let all = evaluate(&EventQuery::All, &events, &limits).unwrap();
        assert_eq!(
            validate_receipt(&receipt, &query, &events, &all, &limits).unwrap_err(),
            EventQueryError::ResultDrift
        );

        assert_eq!(
            validate_receipt(&receipt, &EventQuery::All, &events, &result, &limits).unwrap_err(),
            EventQueryError::ReceiptDrift
        );
    }

    #[test]
    fn timeline_escapes_table_and_label_delimiters() {
        let facts = vec![TimelineFact {
            id: 1,
            parent: None,
            tick: 0,
            kind: "k|ind".to_string(),
            label: "a|b\"c\\".to_string(),
        }];
        let markdown = render_timeline_markdown(&facts).unwrap();
        assert!(markdown.contains(r"k\|ind"));
        assert!(markdown.contains(r"a\|b"));
        let dot = render_timeline_dot(&facts).unwrap();
        assert!(dot.contains(r#"  n1 [label="0: a|b\"c\\"];"#));
    }

    #[test]
    fn recorded_events_become_canonical_envelopes() {
        use crate::recording::RecordedEvent;
        let event = RecordedEvent::AssertionHit {
            tick: 42,
            vm_index: 1,
            assertion_id: 7,
            location: "leader elected".to_string(),
            passed: true,
        };
        let envelope = envelope_from_recorded_event([5; 32], 3, &event);
        assert_eq!(envelope.source_kind, EventSourceKind::Assertion);
        assert_eq!(envelope.kind, "assertion_hit");
        assert_eq!(envelope.tick, 42);
        assert_eq!(envelope.vm_index, Some(1));
        assert_eq!(envelope.text, "leader elected");
        assert_ne!(envelope.payload_digest, [0; 32]);

        let long = RecordedEvent::SerialOutput {
            tick: 1,
            vm_index: 0,
            data: "x".repeat(MAX_EVENT_TEXT_BYTES + 100),
        };
        let envelope = envelope_from_recorded_event([5; 32], 4, &long);
        assert_eq!(envelope.text.len(), MAX_EVENT_TEXT_BYTES);
    }

    #[test]
    fn serialized_rows_bytes_matches_the_json_array() {
        let rows = events();
        let expected = serde_json::to_vec(&rows).unwrap().len();
        assert_eq!(serialized_rows_bytes(&rows, usize::MAX).unwrap(), expected);
        assert_eq!(serialized_rows_bytes(&[], usize::MAX).unwrap(), 2);
        assert_eq!(
            serialized_rows_bytes(&rows, expected - 1).unwrap_err(),
            EventQueryError::ResultBound
        );
    }

    #[test]
    fn bounded_text_stays_on_utf8_boundaries() {
        let value = format!("{}\u{e9}", "a".repeat(MAX_EVENT_TEXT_BYTES - 1));
        let bounded = bounded_text(&value);
        assert!(bounded.len() <= MAX_EVENT_TEXT_BYTES);
        assert!(value.is_char_boundary(bounded.len()));
    }

    #[test]
    fn dlog_bytes_become_canonical_envelopes() {
        use chaoscontrol_vmm::dlog::{DlogRecord, DlogTag, RECORD_SIZE};
        let record = DlogRecord::new(7, 100, 42, 0x200, DlogTag::IoOut, 0).with_port(0x10);
        let bytes = record.to_bytes();
        let envelopes = envelopes_from_dlog_bytes(&bytes, 10).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].source_kind, EventSourceKind::Dlog);
        assert_eq!(envelopes[0].tick, 100);
        admit_event(&envelopes[0]).unwrap();
        assert_eq!(envelopes[0].kind, format!("dlog_{}", record.tag));
        assert_ne!(envelopes[0].payload_digest, [0; 32]);
        assert_eq!(
            envelopes_from_dlog_bytes(&bytes[..RECORD_SIZE - 1], 10).unwrap_err(),
            EventQueryError::InvalidEvent
        );

        let mut two_records = bytes.to_vec();
        two_records.extend_from_slice(&bytes);
        assert_eq!(
            envelopes_from_dlog_bytes(&two_records, 1).unwrap_err(),
            EventQueryError::ScanBound
        );
        assert_eq!(envelopes_from_dlog_bytes(&two_records, 2).unwrap().len(), 2);
    }

    fn fault_stage_fixture() -> (
        chaoscontrol_fault::outcomes::FaultOutcomeLedger,
        Vec<crate::recording::FaultRoundTraceDelta>,
        chaoscontrol_fault::outcomes::FaultAttemptId,
    ) {
        use chaoscontrol_fault::faults::Fault;
        use chaoscontrol_fault::outcomes::{
            fault_run_id, transition_fault_outcome, FaultAttempt, FaultAttemptSource,
            FaultOutcomeLedger, FaultStageKind, NANOSECONDS_PER_SIMULATION_TICK,
        };
        use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};

        const SEED: u64 = 42;
        const RUN_SEQUENCE: u64 = 1;
        const TICK: u64 = 7;
        let selected_at_ns = TICK * NANOSECONDS_PER_SIMULATION_TICK;
        let fault = Fault::ProcessKill { target: 0 };
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(selected_at_ns, fault.clone()));
        let schedule_id = schedule.identity();
        let attempt = FaultAttempt::new_with_source(
            fault_run_id(SEED, RUN_SEQUENCE, schedule_id),
            RUN_SEQUENCE,
            schedule_id,
            0,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: selected_at_ns,
            },
            fault,
        );
        let ledger = transition_fault_outcome(
            &FaultOutcomeLedger::default(),
            Some(&attempt),
            attempt.id,
            FaultStageKind::Selected,
        )
        .unwrap();
        let deltas = vec![crate::recording::FaultRoundTraceDelta {
            tick: TICK,
            event_start: 0,
            event_end: 1,
        }];
        (ledger, deltas, attempt.id)
    }

    #[test]
    fn fault_stage_trace_becomes_canonical_envelopes() {
        use chaoscontrol_fault::outcomes::FaultOutcomeLedger;

        let (ledger, deltas, attempt_id) = fault_stage_fixture();
        let envelopes =
            envelopes_from_fault_stages([9; 32], &ledger.events, &deltas, &ledger).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].source_kind, EventSourceKind::FaultStage);
        assert_eq!(envelopes[0].source_sequence, 0);
        assert_eq!(envelopes[0].tick, 7);
        assert_eq!(envelopes[0].kind, "fault_stage_selected");
        assert!(envelopes[0].text.contains(&attempt_id.to_string()));
        assert_eq!(
            envelopes[0].details.as_ref().unwrap()["stage"],
            serde_json::json!("selected")
        );

        let legacy = envelopes_from_fault_stages([9; 32], &ledger.events, &[], &ledger).unwrap();
        assert_eq!(legacy[0].tick, 7);

        let uncovered = vec![crate::recording::FaultRoundTraceDelta {
            tick: 8,
            event_start: 1,
            event_end: 2,
        }];
        assert_eq!(
            envelopes_from_fault_stages([9; 32], &ledger.events, &uncovered, &ledger).unwrap_err(),
            EventQueryError::InvalidEvent
        );

        let mut tampered = ledger.events.clone();
        tampered[0].sequence = 9;
        assert_eq!(
            envelopes_from_fault_stages([9; 32], &tampered, &deltas, &ledger).unwrap_err(),
            EventQueryError::InvalidEvent
        );

        let mut missing_attempt = ledger.clone();
        missing_attempt.attempts.clear();
        assert_eq!(
            envelopes_from_fault_stages(
                [9; 32],
                &missing_attempt.events,
                &deltas,
                &missing_attempt
            )
            .unwrap_err(),
            EventQueryError::InvalidEvent
        );
        assert_eq!(
            envelopes_from_fault_stages(
                [0; 32],
                &FaultOutcomeLedger::default().events,
                &[],
                &FaultOutcomeLedger::default()
            )
            .unwrap_err(),
            EventQueryError::InvalidEvent
        );
    }

    #[test]
    fn recording_adapter_includes_fault_stage_envelopes() {
        use crate::recording::{Recorder, RecordingConfig};
        use chaoscontrol_fault::faults::Fault;
        use chaoscontrol_fault::outcomes::{
            fault_run_id, transition_fault_outcome, FaultAttempt, FaultAttemptSource,
            FaultOutcomeLedger, FaultStageKind, NANOSECONDS_PER_SIMULATION_TICK,
        };
        use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
        use chaoscontrol_vmm::controller::RoundResult;
        use std::collections::BTreeSet;

        const SEED: u64 = 42;
        const RUN_SEQUENCE: u64 = 1;
        const TICK: u64 = 7;
        let selected_at_ns = TICK * NANOSECONDS_PER_SIMULATION_TICK;
        let fault = Fault::ProcessKill { target: 0 };
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(selected_at_ns, fault.clone()));
        let schedule_id = schedule.identity();
        let attempt = FaultAttempt::new_with_source(
            fault_run_id(SEED, RUN_SEQUENCE, schedule_id),
            RUN_SEQUENCE,
            schedule_id,
            0,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: selected_at_ns,
            },
            fault.clone(),
        );
        let ledger = transition_fault_outcome(
            &FaultOutcomeLedger::default(),
            Some(&attempt),
            attempt.id,
            FaultStageKind::Selected,
        )
        .unwrap();
        let round = RoundResult {
            tick: TICK,
            vms_running: 1,
            vms_halted: 0,
            faults_fired: vec![fault],
            fault_outcomes: ledger.events.clone(),
            messages_delivered: 0,
            schedule_traces: Vec::new(),
        };
        let mut recorder = Recorder::new(
            RecordingConfig {
                num_vms: 2,
                vm_memory_size: 256 * 1024 * 1024,
                tsc_khz: 3_000_000,
                kernel_path: "/test/vmlinux".to_string(),
                initrd_path: None,
                quantum: 100,
                checkpoint_interval: 1_000,
                disk_image_path: None,
                random_faults: false,
            },
            schedule,
            SEED,
        );
        recorder.record_round(&round, &ledger).unwrap();
        let envelopes = envelopes_from_recording(recorder.recording()).unwrap();
        assert!(envelopes.iter().any(|event| event.kind == "fault_fired"));
        assert!(envelopes
            .iter()
            .any(|event| event.kind == "fault_stage_selected" && event.tick == TICK));
        let source_ids = envelopes
            .iter()
            .map(|event| event.source_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(source_ids.len(), 2);
    }

    #[test]
    fn branch_markers_become_canonical_envelopes() {
        let identity =
            chaoscontrol_protocol::branch_marker::marker_identity("raft", "leader-elected");
        let marker = MarkerEvidenceContext {
            marker_identity: identity.clone(),
            owner: "guest-0".to_string(),
            state_ref: None,
            logical_position_ref: None,
            run_id: 1,
            observed_tick: 42,
        };
        let envelopes = envelopes_from_markers([7; 32], std::slice::from_ref(&marker)).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].source_kind, EventSourceKind::BranchMarker);
        assert_eq!(envelopes[0].source_sequence, 0);
        assert_eq!(envelopes[0].tick, 42);
        assert_eq!(envelopes[0].process.as_deref(), Some("guest-0"));
        assert_eq!(envelopes[0].kind, "branch_marker");
        assert_eq!(envelopes[0].text, identity);
        assert!(envelopes[0].details.is_some());
        admit_event(&envelopes[0]).unwrap();

        let mut invalid = marker;
        invalid.marker_identity = "b3:00".to_string();
        assert_eq!(
            envelopes_from_markers([7; 32], &[invalid]).unwrap_err(),
            EventQueryError::InvalidEvent
        );
        assert_eq!(
            envelopes_from_markers([0; 32], &[]).unwrap_err(),
            EventQueryError::InvalidEvent
        );
    }

    #[test]
    fn receipt_binds_rows_and_sources() {
        let events = events();
        let query = EventQuery::Filter {
            input: Box::new(EventQuery::All),
            predicate: EventPredicate::Kind("assert".to_string()),
        };
        let limits = EventQueryLimits::default();
        let result = evaluate(&query, &events, &limits).unwrap();
        let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
        assert_eq!(receipt.source_ids, vec![[1; 32], [2; 32]]);
        assert_eq!(receipt.rows, 2);
        let bytes = serde_json::to_vec(&result.rows).unwrap();
        assert_eq!(receipt.rows_digest, *blake3::hash(&bytes).as_bytes());
        assert!(!receipt.truncated);

        let limited = EventQueryLimits {
            max_result_rows: 1,
            ..limits
        };
        let truncated_result = evaluate(&query, &events, &limited).unwrap();
        assert!(truncated_result.truncated);
        assert_eq!(truncated_result.rows.len(), 1);
        assert_eq!(truncated_result.matched, 2);
        let truncated_receipt = receipt_for(&query, &events, &truncated_result, &limited).unwrap();
        assert!(truncated_receipt.truncated);
        assert_eq!(truncated_receipt.rows, 1);
        assert_eq!(truncated_receipt.matched, 2);
    }
}
