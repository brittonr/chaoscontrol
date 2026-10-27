//! Bounded queries and static timelines over retained event envelopes.
//!
//! This module is pure: callers supply admitted events, queries, and facts.
//! It performs no file, clock, process, network, or environment access.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const EVENT_ENVELOPE_SCHEMA: &str = "chaoscontrol.event-envelope.v1";
pub const EVENT_QUERY_RECEIPT_SCHEMA: &str = "chaoscontrol.event-query-receipt.v1";
pub const EVENT_QUERY_RESULT_SCHEMA: &str = "chaoscontrol.event-query-result.v1";
pub const TIMELINE_SCHEMA: &str = "chaoscontrol.event-timeline.v1";

pub const MAX_QUERY_NODES: usize = 256;
pub const MAX_QUERY_DEPTH: usize = 64;
pub const MAX_SCANNED_EVENTS: usize = 1_000_000;
pub const MAX_RESULT_ROWS: usize = 100_000;
pub const MAX_EVENT_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_EVENT_DETAILS_BYTES: usize = 64 * 1024;
pub const MAX_TIMELINE_NODES: usize = 10_000;
pub const MAX_FIELD_BYTES: usize = 256;

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
pub fn envelopes_from_recording(recording: &crate::recording::Recording) -> Vec<EventEnvelope> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chaoscontrol.recording-source.v1\0");
    hasher.update(recording.session_id.as_bytes());
    hasher.update(&recording.fault_run_id.0);
    let source_id = *hasher.finalize().as_bytes();
    recording
        .events
        .iter()
        .enumerate()
        .map(|(sequence, event)| envelope_from_recorded_event(source_id, sequence as u64, event))
        .collect()
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
    if max_records == 0 {
        return Err(EventQueryError::ScanBound);
    }
    let source_id = *blake3::hash(bytes).as_bytes();
    let mut envelopes = Vec::new();
    for sequence in 0..bytes.len() / RECORD_SIZE {
        if envelopes.len() >= max_records {
            break;
        }
        let start = sequence * RECORD_SIZE;
        let mut record = [0_u8; RECORD_SIZE];
        record.copy_from_slice(&bytes[start..start + RECORD_SIZE]);
        let record = DlogRecord::from_bytes(record);
        envelopes.push(envelope_from_dlog(source_id, sequence as u64, &record));
    }
    Ok(envelopes)
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
    if value.len() <= MAX_EVENT_TEXT_BYTES {
        return value.to_string();
    }
    let mut end = MAX_EVENT_TEXT_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// A bounded predicate over one event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventPredicate {
    TickRange { start_tick: u64, end_tick: u64 },
    Vm(u32),
    SourceKind(EventSourceKind),
    Kind(String),
    Process(String),
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
    Map {
        input: Box<EventQuery>,
        field: String,
        pointer: String,
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
    pub max_scanned_events: usize,
    pub max_result_rows: usize,
}

impl Default for EventQueryLimits {
    fn default() -> Self {
        Self {
            max_nodes: MAX_QUERY_NODES,
            max_scanned_events: MAX_SCANNED_EVENTS,
            max_result_rows: MAX_RESULT_ROWS,
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
    pub query: EventQuery,
    #[serde(default = "no_u64", skip_serializing_if = "Option::is_none")]
    pub moment_tick: Option<u64>,
    pub limits: EventQueryLimits,
    pub scanned: u64,
    pub matched: u64,
    pub rows: u64,
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
    ScanBound,
    RowBound,
    InvalidEvent,
    InvalidText,
    InvalidField,
    InvalidPointer,
    TimelineBound,
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
            Self::ScanBound => "event query scan count exceeds the bound",
            Self::RowBound => "event query result rows exceed the bound",
            Self::InvalidEvent => "event envelope schema or identity is invalid",
            Self::InvalidText => "event text is empty, over-bound, or contains controls",
            Self::InvalidField => "event query field is not supported",
            Self::InvalidPointer => "event query JSON pointer is not confined",
            Self::TimelineBound => "timeline node count exceeds the bound",
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
            || self.max_scanned_events == 0
            || self.max_scanned_events > MAX_SCANNED_EVENTS
            || self.max_result_rows == 0
            || self.max_result_rows > MAX_RESULT_ROWS
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
    Ok(())
}

/// Evaluate one query over admitted events.
pub fn evaluate(
    query: &EventQuery,
    events: &[EventEnvelope],
    limits: &EventQueryLimits,
) -> Result<EventQueryResult, EventQueryError> {
    limits.validate()?;
    validate_query(query, limits)?;
    if events.len() > limits.max_scanned_events {
        return Err(EventQueryError::ScanBound);
    }
    for event in events {
        admit_event(event)?;
    }
    let mut rows = evaluate_rows(query, events)?;
    rows.sort_by_key(event_order);
    rows.dedup_by_key(|event| (event.source_id, event.source_sequence));
    let matched = u64::try_from(rows.len()).map_err(|_| EventQueryError::CounterOverflow)?;
    let truncated = rows.len() > limits.max_result_rows;
    if truncated {
        rows.truncate(limits.max_result_rows);
    }
    Ok(EventQueryResult {
        schema: EVENT_QUERY_RESULT_SCHEMA.to_string(),
        rows,
        scanned: u64::try_from(events.len()).map_err(|_| EventQueryError::CounterOverflow)?,
        matched,
        truncated,
    })
}

/// Build a receipt binding the query, source identities, limits, and rows.
pub fn receipt_for(
    query: &EventQuery,
    source_ids: &[[u8; 32]],
    result: &EventQueryResult,
    limits: &EventQueryLimits,
) -> Result<EventQueryReceipt, EventQueryError> {
    let rows = serde_json::to_vec(&result.rows).map_err(|_| EventQueryError::Digest)?;
    let rows_digest = *blake3::hash(&rows).as_bytes();
    let mut source_ids = source_ids.to_vec();
    source_ids.sort_unstable();
    source_ids.dedup();
    Ok(EventQueryReceipt {
        schema: EVENT_QUERY_RECEIPT_SCHEMA.to_string(),
        source_ids,
        query: query.clone(),
        moment_tick: moment_of(query),
        limits: *limits,
        scanned: result.scanned,
        matched: result.matched,
        rows: u64::try_from(result.rows.len()).map_err(|_| EventQueryError::CounterOverflow)?,
        rows_digest,
    })
}

/// Render a bounded static Markdown timeline.
pub fn render_timeline_markdown(facts: &[TimelineFact]) -> Result<String, EventQueryError> {
    let ordered = ordered_timeline(facts)?;
    let mut output =
        String::from("| tick | id | parent | kind | label |\n|---:|---:|---:|---|---|\n");
    for fact in ordered {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            fact.tick,
            fact.id,
            fact.parent
                .map_or_else(|| "-".to_string(), |p| p.to_string()),
            fact.kind,
            fact.label
        ));
    }
    Ok(output)
}

/// Render a bounded static DOT graph.
pub fn render_timeline_dot(facts: &[TimelineFact]) -> Result<String, EventQueryError> {
    let ordered = ordered_timeline(facts)?;
    let mut output = String::from("digraph chaoscontrol_timeline {\n");
    for fact in &ordered {
        output.push_str(&format!(
            "  n{} [label=\"{}: {}\"];\n",
            fact.id, fact.tick, fact.label
        ));
    }
    for fact in &ordered {
        if let Some(parent) = fact.parent {
            output.push_str(&format!("  n{parent} -> n{};\n", fact.id));
        }
    }
    output.push_str("}\n");
    Ok(output)
}

fn evaluate_rows(
    query: &EventQuery,
    events: &[EventEnvelope],
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    match query {
        EventQuery::All => Ok(events.to_vec()),
        EventQuery::Filter { input, predicate } => Ok(evaluate_rows(input, events)?
            .into_iter()
            .filter(|event| predicate.matches(event))
            .collect()),
        EventQuery::Map {
            input,
            field,
            pointer,
        } => {
            validate_field(field)?;
            validate_pointer(pointer)?;
            let mut rows = evaluate_rows(input, events)?;
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
        EventQuery::Contains { input, field, text } => {
            validate_field(field)?;
            validate_text(text)?;
            Ok(evaluate_rows(input, events)?
                .into_iter()
                .filter(|event| field_value(event, field).is_some_and(|value| value.contains(text)))
                .collect())
        }
        EventQuery::Matches { input, field, text } => {
            validate_field(field)?;
            validate_text(text)?;
            Ok(evaluate_rows(input, events)?
                .into_iter()
                .filter(|event| field_value(event, field) == Some(text.as_str()))
                .collect())
        }
        EventQuery::Union { inputs } => {
            let mut rows = Vec::new();
            for input in inputs {
                rows.extend(evaluate_rows(input, events)?);
            }
            Ok(rows)
        }
        EventQuery::Window {
            input,
            start_tick,
            end_tick,
        } => {
            if start_tick > end_tick {
                return Err(EventQueryError::InvalidText);
            }
            Ok(evaluate_rows(input, events)?
                .into_iter()
                .filter(|event| (*start_tick..=*end_tick).contains(&event.tick))
                .collect())
        }
        EventQuery::UpTo { input, moment_tick } => Ok(evaluate_rows(input, events)?
            .into_iter()
            .filter(|event| event.tick <= *moment_tick)
            .collect()),
    }
}

impl EventPredicate {
    fn matches(&self, event: &EventEnvelope) -> bool {
        match self {
            Self::TickRange {
                start_tick,
                end_tick,
            } => (*start_tick..=*end_tick).contains(&event.tick),
            Self::Vm(vm) => event.vm_index == Some(*vm),
            Self::SourceKind(kind) => event.source_kind == *kind,
            Self::Kind(kind) => event.kind == *kind,
            Self::Process(process) => event.process.as_deref() == Some(process),
        }
    }
}

fn validate_query(query: &EventQuery, limits: &EventQueryLimits) -> Result<(), EventQueryError> {
    let mut nodes = 0_usize;
    validate_query_inner(query, limits, 0, &mut nodes)
}

fn validate_query_inner(
    query: &EventQuery,
    limits: &EventQueryLimits,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), EventQueryError> {
    if depth > MAX_QUERY_DEPTH {
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
        EventQuery::Filter { input, .. }
        | EventQuery::Map { input, .. }
        | EventQuery::Contains { input, .. }
        | EventQuery::Matches { input, .. }
        | EventQuery::Window { input, .. }
        | EventQuery::UpTo { input, .. } => validate_query_inner(input, limits, depth + 1, nodes),
        EventQuery::Union { inputs } => {
            if inputs.is_empty() {
                return Err(EventQueryError::QueryBound);
            }
            for input in inputs {
                validate_query_inner(input, limits, depth + 1, nodes)?;
            }
            Ok(())
        }
    }
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

fn moment_of(query: &EventQuery) -> Option<u64> {
    match query {
        EventQuery::UpTo { moment_tick, .. } => Some(*moment_tick),
        EventQuery::Filter { input, .. }
        | EventQuery::Map { input, .. }
        | EventQuery::Contains { input, .. }
        | EventQuery::Matches { input, .. }
        | EventQuery::Window { input, .. } => moment_of(input),
        EventQuery::Union { inputs } => inputs.iter().find_map(moment_of),
        EventQuery::All => None,
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
    for fact in facts {
        if let Some(parent) = fact.parent {
            if !by_id.contains_key(&parent) {
                return Err(EventQueryError::MissingTimelineParent);
            }
        }
        let mut seen = BTreeSet::new();
        let mut current = Some(fact.id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(EventQueryError::TimelineCycle);
            }
            current = by_id.get(&id).and_then(|node| node.parent);
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
    fn dlog_bytes_become_canonical_envelopes() {
        use chaoscontrol_vmm::dlog::{DlogRecord, DlogTag, RECORD_SIZE};
        let record = DlogRecord::new(7, 100, 42, 0x200, DlogTag::IoOut, 0).with_port(0x10);
        let bytes = record.to_bytes();
        let envelopes = envelopes_from_dlog_bytes(&bytes, 10).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].source_kind, EventSourceKind::Dlog);
        assert_eq!(envelopes[0].tick, 100);
        assert_eq!(envelopes[0].kind, format!("dlog_{}", record.tag));
        assert_ne!(envelopes[0].payload_digest, [0; 32]);
        assert_eq!(
            envelopes_from_dlog_bytes(&bytes[..RECORD_SIZE - 1], 10).unwrap_err(),
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
        let receipt = receipt_for(&query, &[[2; 32], [1; 32], [1; 32]], &result, &limits).unwrap();
        assert_eq!(receipt.source_ids, vec![[1; 32], [2; 32]]);
        assert_eq!(receipt.rows, 2);
        let bytes = serde_json::to_vec(&result.rows).unwrap();
        assert_eq!(receipt.rows_digest, *blake3::hash(&bytes).as_bytes());
    }
}
