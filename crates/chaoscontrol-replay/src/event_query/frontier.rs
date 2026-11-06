//! Typed frontier adapters. Product round numbers never become simulation ticks.
use super::*;
use chaoscontrol_explore::explorer::{inspect_frontier_trace, FrontierTraceViewError};
use chaoscontrol_explore::frontier::FrontierEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontierTimelineProjection {
    pub source_id: [u8; 32],
    pub facts: Vec<TimelineFact>,
    pub missing_artifacts: Vec<String>,
    pub truncated: bool,
    pub records: u64,
}

fn bounded_projection(
    projection: FrontierTimelineProjection,
    limits: &TimelineLimits,
) -> Result<FrontierTimelineProjection, EventQueryError> {
    if serde_json::to_vec(&projection)
        .map_err(|_| EventQueryError::Digest)?
        .len()
        > limits.max_bytes
    {
        return Err(EventQueryError::TimelineByteBound);
    }
    Ok(projection)
}

/// Inspect the existing retained frontier trace with its producer's decoder.
/// Trace v1 contains decisions but neither simulation ticks nor fork parentage;
/// therefore it contributes explicit gap/summary notes, not fabricated timed nodes.
pub fn timeline_from_frontier_trace(
    bytes: &[u8],
    limits: &TimelineLimits,
) -> Result<FrontierTimelineProjection, EventQueryError> {
    limits.validate()?;
    let view = inspect_frontier_trace(bytes, limits.max_rows.min(MAX_TIMELINE_NODES)).map_err(
        |error| match error {
            FrontierTraceViewError::InputBound => EventQueryError::TimelineBound,
            FrontierTraceViewError::InvalidRecord => EventQueryError::UnsupportedRecord,
            FrontierTraceViewError::InvalidShape => EventQueryError::InvalidEvent,
        },
    )?;
    let source_id = *blake3::hash(bytes).as_bytes();
    let identity = format!("b3:{}", blake3::Hash::from(source_id).to_hex());
    let mut missing_artifacts = vec![
        format!("frontier {identity}: simulation ticks absent in frontier-trace.v1; {} decision records / {} rounds are untimed", view.records, view.rounds),
        format!("frontier {identity}: snapshot checkpoint identities and branch parentage are not retained in frontier-trace.v1; decision selection IDs are not fork edges"),
    ];
    if !view.root_present {
        missing_artifacts.push(format!(
            "frontier {identity}: root record missing from retained trace prefix"
        ));
    }
    if view.truncated {
        missing_artifacts.push(format!(
            "frontier {identity}: truncated=true; stop record missing from retained trace prefix"
        ));
    }
    if missing_artifacts.len() > limits.max_nodes || missing_artifacts.len() > limits.max_rows {
        return Err(EventQueryError::TimelineBound);
    }
    bounded_projection(
        FrontierTimelineProjection {
            source_id,
            facts: vec![],
            missing_artifacts,
            truncated: view.truncated,
            records: view.records,
        },
        limits,
    )
}

/// Adapt real retained in-memory entries using exact snapshot ticks and parent IDs.
/// Callers must include referenced ancestors; a pruned/missing parent is a typed
/// failure rather than a fabricated root or synthetic checkpoint.
pub fn timeline_from_frontier_entries(
    source_id: [u8; 32],
    entries: &[FrontierEntry],
    limits: &TimelineLimits,
) -> Result<FrontierTimelineProjection, EventQueryError> {
    limits.validate()?;
    if source_id == [0; 32] {
        return Err(EventQueryError::InvalidEvent);
    }
    if entries.len() > limits.max_nodes
        || entries.len() > limits.max_rows
        || entries
            .iter()
            .filter(|entry| entry.parent.is_some())
            .count()
            > limits.max_edges
    {
        return Err(EventQueryError::TimelineBound);
    }
    let mut facts = Vec::with_capacity(entries.len());
    for entry in entries {
        if !entry.score.is_finite() {
            return Err(EventQueryError::InvalidNumber);
        }
        facts.push(TimelineFact {
            id: entry.id,
            parent: entry.parent,
            tick: entry.snapshot.tick,
            kind: "frontier_checkpoint".into(),
            label: format!(
                "frontier entry={} depth={} selected={} score_bits={} snapshot_tick={}",
                entry.id,
                entry.depth,
                entry.times_selected,
                entry.score.to_bits(),
                entry.snapshot.tick
            ),
        });
    }
    let facts = ordered_timeline(&facts)?;
    let by_id: BTreeMap<_, _> = entries.iter().map(|entry| (entry.id, entry)).collect();
    for entry in entries {
        if let Some(parent) = entry.parent {
            let parent = by_id
                .get(&parent)
                .ok_or(EventQueryError::MissingTimelineParent)?;
            if parent.snapshot.tick > entry.snapshot.tick
                || parent.depth.checked_add(1) != Some(entry.depth)
            {
                return Err(EventQueryError::InvalidEvent);
            }
        } else if entry.depth != 0 {
            return Err(EventQueryError::MissingTimelineParent);
        }
    }
    bounded_projection(
        FrontierTimelineProjection {
            source_id,
            facts,
            missing_artifacts: vec![],
            truncated: false,
            records: entries.len() as u64,
        },
        limits,
    )
}

#[cfg(test)]
mod tests;
