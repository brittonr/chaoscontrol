//! Static workspace views and evidence over exact admitted query inputs.
use super::*;
use crate::moment::MomentContext;

pub const WORKSPACE_QUERY_RECEIPT_SCHEMA: &str = "chaoscontrol.workspace-query-receipt.v1";
pub const WORKSPACE_TIMELINE_RECEIPT_SCHEMA: &str = "chaoscontrol.workspace-timeline-receipt.v1";
pub const MAX_TIMELINE_EDGES: usize = MAX_TIMELINE_NODES;
pub const MAX_TIMELINE_ROWS: usize = MAX_RESULT_ROWS;
pub const MAX_MISSING_ARTIFACTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceQueryReceipt {
    pub schema: String,
    pub query: EventQueryReceipt,
    pub moment: MomentContext,
    pub output_digest: [u8; 32],
}

fn source_names(ids: &[[u8; 32]]) -> Vec<String> {
    ids.iter()
        .map(|id| format!("b3:{}", blake3::Hash::from(*id).to_hex()))
        .collect()
}

fn admit_context(moment: &MomentContext, events: &[EventEnvelope]) -> Result<(), EventQueryError> {
    moment
        .validate()
        .map_err(|_| EventQueryError::InvalidWindow)?;
    let ids: Vec<_> = events
        .iter()
        .map(|e| e.source_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    // The admission shell binds declared artifacts, including empty sources.
    // The pure core can check origins, not reopen those artifacts.
    if moment.source_ids.len() > MAX_SCANNED_EVENTS
        || source_names(&ids)
            .iter()
            .any(|id| moment.source_ids.binary_search(id).is_err())
    {
        return Err(EventQueryError::SourceDrift);
    }
    if events
        .iter()
        .any(|e| e.tick < moment.retained_start_tick || e.tick > moment.retained_end_tick)
    {
        return Err(EventQueryError::InvalidWindow);
    }
    Ok(())
}

fn at_moment(query: &EventQuery, moment: &MomentContext) -> EventQuery {
    EventQuery::Window {
        input: Box::new(query.clone()),
        start_tick: moment.start_tick,
        end_tick: moment.end_tick,
    }
}

/// Query exactly the admitted inclusive moment horizon; no future execution occurs here.
pub fn evaluate_at_moment(
    query: &EventQuery,
    events: &[EventEnvelope],
    limits: &EventQueryLimits,
    moment: &MomentContext,
) -> Result<EventQueryResult, EventQueryError> {
    admit_context(moment, events)?;
    if moment_of(query)?.is_some_and(|tick| tick != moment.tick) {
        return Err(EventQueryError::AmbiguousMoment);
    }
    evaluate(&at_moment(query, moment), events, limits)
}

pub fn workspace_query_receipt(
    query: &EventQuery,
    events: &[EventEnvelope],
    result: &EventQueryResult,
    limits: &EventQueryLimits,
    moment: &MomentContext,
) -> Result<WorkspaceQueryReceipt, EventQueryError> {
    admit_context(moment, events)?;
    if moment_of(query)?.is_some_and(|tick| tick != moment.tick) {
        return Err(EventQueryError::AmbiguousMoment);
    }
    let query = receipt_for(&at_moment(query, moment), events, result, limits)?;
    let bytes = serde_json::to_vec(result).map_err(|_| EventQueryError::Digest)?;
    Ok(WorkspaceQueryReceipt {
        schema: WORKSPACE_QUERY_RECEIPT_SCHEMA.to_string(),
        query,
        moment: moment.clone(),
        output_digest: *blake3::hash(&bytes).as_bytes(),
    })
}

pub fn validate_workspace_query_receipt(
    receipt: &WorkspaceQueryReceipt,
    query: &EventQuery,
    events: &[EventEnvelope],
    result: &EventQueryResult,
    limits: &EventQueryLimits,
    moment: &MomentContext,
) -> Result<(), EventQueryError> {
    if *receipt != workspace_query_receipt(query, events, result, limits, moment)? {
        return Err(EventQueryError::ReceiptDrift);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl Default for TimelineLimits {
    fn default() -> Self {
        Self {
            max_nodes: MAX_TIMELINE_NODES,
            max_edges: MAX_TIMELINE_EDGES,
            max_rows: MAX_TIMELINE_ROWS,
            max_bytes: MAX_TIMELINE_BYTES,
        }
    }
}

impl TimelineLimits {
    pub fn validate(&self) -> Result<(), EventQueryError> {
        if self.max_nodes == 0
            || self.max_nodes > MAX_TIMELINE_NODES
            || self.max_edges == 0
            || self.max_edges > MAX_TIMELINE_EDGES
            || self.max_rows == 0
            || self.max_rows > MAX_TIMELINE_ROWS
            || self.max_bytes == 0
            || self.max_bytes > MAX_TIMELINE_BYTES
        {
            return Err(EventQueryError::TimelineBound);
        }
        Ok(())
    }
}

/// No other claim is deserializable: in particular a complete-history claim fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineClaim {
    RetainedFactsOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTimelineInput {
    pub facts: Vec<TimelineFact>,
    pub events: Vec<EventEnvelope>,
    pub query_receipt: EventQueryReceipt,
    pub query_result: EventQueryResult,
    pub moment: MomentContext,
    pub missing_artifacts: Vec<String>,
    #[serde(default)]
    pub source_truncated: bool,
    pub limits: TimelineLimits,
    pub claim: TimelineClaim,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTimelineReceipt {
    pub schema: String,
    pub format: String,
    pub query_receipt: EventQueryReceipt,
    pub moment: MomentContext,
    pub source_ids: Vec<String>,
    pub limits: TimelineLimits,
    pub nodes: u64,
    pub edges: u64,
    pub rows: u64,
    pub scanned: u64,
    pub matched: u64,
    pub truncated: bool,
    pub source_truncated: bool,
    pub missing_artifacts: Vec<String>,
    pub claim: TimelineClaim,
    pub bytes: u64,
    pub input_digest: [u8; 32],
    pub output_digest: [u8; 32],
}

fn admitted_timeline(input: &WorkspaceTimelineInput) -> Result<Vec<TimelineFact>, EventQueryError> {
    input.limits.validate()?;
    admit_context(&input.moment, &input.events)?;
    validate_receipt(
        &input.query_receipt,
        &input.query_receipt.query,
        &input.events,
        &input.query_result,
        &input.query_receipt.limits,
    )?;
    // The retained query receipt must have been evaluated inside this exact horizon.
    match &input.query_receipt.query {
        EventQuery::Window {
            start_tick,
            end_tick,
            ..
        } if *start_tick == input.moment.start_tick && *end_tick == input.moment.end_tick => {}
        _ => return Err(EventQueryError::ReceiptDrift),
    }
    let nodes = input
        .facts
        .len()
        .checked_add(input.query_result.rows.len())
        .and_then(|count| count.checked_add(input.missing_artifacts.len()))
        .ok_or(EventQueryError::CounterOverflow)?;
    if nodes > input.limits.max_nodes
        || input.facts.iter().filter(|f| f.parent.is_some()).count() > input.limits.max_edges
    {
        return Err(EventQueryError::TimelineBound);
    }
    if nodes > input.limits.max_rows {
        return Err(EventQueryError::RowBound);
    }
    if input.missing_artifacts.len() > MAX_MISSING_ARTIFACTS {
        return Err(EventQueryError::TimelineBound);
    }
    let mut gaps = BTreeSet::new();
    for gap in &input.missing_artifacts {
        validate_text(gap)?;
        if !gaps.insert(gap) {
            return Err(EventQueryError::InvalidEvent);
        }
    }
    let ordered = ordered_timeline(&input.facts)?;
    let ticks: BTreeMap<_, _> = ordered.iter().map(|f| (f.id, f.tick)).collect();
    for fact in &ordered {
        if fact.tick < input.moment.retained_start_tick
            || fact.tick > input.moment.retained_end_tick
        {
            return Err(EventQueryError::InvalidWindow);
        }
        if fact.parent.is_some_and(|p| ticks[&p] > fact.tick) {
            return Err(EventQueryError::InvalidWindow);
        }
    }
    Ok(ordered)
}

fn append_bounded(output: &mut String, value: &str, limit: usize) -> Result<(), EventQueryError> {
    if output
        .len()
        .checked_add(value.len())
        .ok_or(EventQueryError::CounterOverflow)?
        > limit
    {
        return Err(EventQueryError::TimelineByteBound);
    }
    output.push_str(value);
    Ok(())
}

/// Render a self-contained static artifact with every declared gap and truncation fact.
pub fn render_workspace_timeline(
    input: &WorkspaceTimelineInput,
    format: &str,
) -> Result<String, EventQueryError> {
    let ordered = admitted_timeline(input)?;
    let mut output = String::new();
    let header = format!("Retained facts only; not complete capture or causal evidence. sources={} moment={} horizon={}..={} retained={}..={} truncated={} query_truncated={} source_truncated={} scanned={} matched={} rows={}",
        input.moment.source_ids.join(","), input.moment.tick,
        input.moment.start_tick, input.moment.end_tick, input.moment.retained_start_tick,
        input.moment.retained_end_tick, input.query_result.truncated || input.source_truncated,
        input.query_result.truncated, input.source_truncated, input.query_result.scanned,
        input.query_result.matched, input.query_result.rows.len());
    let limit = input.limits.max_bytes;
    match format {
        "markdown" => {
            append_bounded(
                &mut output,
                &format!("# Static retained timeline\n\n{header}\n\n"),
                limit,
            )?;
            for gap in &input.missing_artifacts {
                append_bounded(
                    &mut output,
                    &format!(
                        "Missing artifact: {}\n\n",
                        crate::text::escape_markdown_cell(gap)
                    ),
                    limit,
                )?;
            }
            append_bounded(
                &mut output,
                &render_timeline_markdown_ordered(&ordered)?,
                limit,
            )?;
            append_bounded(&mut output, "\n## Selected query rows\n\n| tick | source | sequence | owner | kind | text |\n|---:|---|---:|---|---|---|\n", limit)?;
            for row in &input.query_result.rows {
                let owner = row
                    .process
                    .clone()
                    .unwrap_or_else(|| format!("vm:{}", row.vm_index.unwrap_or_default()));
                append_bounded(
                    &mut output,
                    &format!(
                        "| {} | b3:{} | {} | {} | {} | {} |\n",
                        row.tick,
                        blake3::Hash::from(row.source_id).to_hex(),
                        row.source_sequence,
                        crate::text::escape_markdown_cell(&owner),
                        crate::text::escape_markdown_cell(&row.kind),
                        crate::text::escape_markdown_cell(&row.text)
                    ),
                    limit,
                )?;
            }
        }
        "dot" => {
            append_bounded(
                &mut output,
                &format!(
                    "digraph chaoscontrol_workspace {{\n  graph [label=\"{}\"];\n",
                    escape_dot_label(&header)
                ),
                limit,
            )?;
            for (index, gap) in input.missing_artifacts.iter().enumerate() {
                append_bounded(
                    &mut output,
                    &format!(
                        "  gap{index} [shape=note,label=\"Missing artifact: {}\"];\n",
                        escape_dot_label(gap)
                    ),
                    limit,
                )?;
            }
            for fact in &ordered {
                append_bounded(
                    &mut output,
                    &format!(
                        "  n{} [label=\"{} {}: {}\"];\n",
                        fact.id,
                        fact.tick,
                        escape_dot_label(&fact.kind),
                        escape_dot_label(&fact.label)
                    ),
                    limit,
                )?;
                if let Some(parent) = fact.parent {
                    append_bounded(
                        &mut output,
                        &format!("  n{parent} -> n{};\n", fact.id),
                        limit,
                    )?;
                }
            }
            for (index, row) in input.query_result.rows.iter().enumerate() {
                append_bounded(
                    &mut output,
                    &format!(
                        "  row{index} [shape=box,label=\"{} b3:{}#{} {}: {}\"];\n",
                        row.tick,
                        blake3::Hash::from(row.source_id).to_hex(),
                        row.source_sequence,
                        escape_dot_label(&row.kind),
                        escape_dot_label(&row.text)
                    ),
                    limit,
                )?;
            }
            append_bounded(&mut output, "}\n", limit)?;
        }
        _ => return Err(EventQueryError::InvalidFormat),
    }
    Ok(output)
}

pub fn workspace_timeline_receipt(
    input: &WorkspaceTimelineInput,
    format: &str,
    rendered: &str,
) -> Result<WorkspaceTimelineReceipt, EventQueryError> {
    if render_workspace_timeline(input, format)? != rendered {
        return Err(EventQueryError::TimelineReceiptDrift);
    }
    let bytes = serde_json::to_vec(input).map_err(|_| EventQueryError::Digest)?;
    Ok(WorkspaceTimelineReceipt {
        schema: WORKSPACE_TIMELINE_RECEIPT_SCHEMA.to_string(),
        format: format.to_string(),
        query_receipt: input.query_receipt.clone(),
        moment: input.moment.clone(),
        source_ids: input.moment.source_ids.clone(),
        limits: input.limits,
        nodes: (input.facts.len() + input.query_result.rows.len() + input.missing_artifacts.len())
            as u64,
        edges: input.facts.iter().filter(|f| f.parent.is_some()).count() as u64,
        rows: input.query_result.rows.len() as u64,
        scanned: input.query_result.scanned,
        matched: input.query_result.matched,
        truncated: input.query_result.truncated || input.source_truncated,
        source_truncated: input.source_truncated,
        missing_artifacts: input.missing_artifacts.clone(),
        claim: input.claim,
        bytes: rendered.len() as u64,
        input_digest: *blake3::hash(&bytes).as_bytes(),
        output_digest: *blake3::hash(rendered.as_bytes()).as_bytes(),
    })
}

pub fn validate_workspace_timeline_receipt(
    receipt: &WorkspaceTimelineReceipt,
    input: &WorkspaceTimelineInput,
    format: &str,
    rendered: &str,
) -> Result<(), EventQueryError> {
    if *receipt != workspace_timeline_receipt(input, format, rendered)? {
        return Err(EventQueryError::TimelineReceiptDrift);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
