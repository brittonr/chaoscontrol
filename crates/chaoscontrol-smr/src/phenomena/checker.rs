use super::model::{
    operation_identity, validate_history, DependencyKind, HistoryOperation, ObservationGap,
    OperationKind, OperationStatus, PhenomenaError, PhenomenaHistory, ReadObservation,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const REPORT_SCHEMA_VERSION: u32 = 1;
pub const CHECKER_ID: &str = "chaoscontrol.history-phenomena.v1";
const REQUIRED_NON_CLAIM_COUNT: usize = 5;
const DEPENDENCY_PAIR_SIZE: usize = 2;
pub const REQUIRED_NON_CLAIMS: [&str; REQUIRED_NON_CLAIM_COUNT] = [
    "not a concurrent-history solver",
    "not proof of the code defect",
    "not complete when observations are missing",
    "not deterministic replay proof",
    "not release eligibility",
];
const REPORT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.phenomena.report.v1\0";
const MAX_VIOLATIONS: usize = 1_024;
const BLAKE3_PREFIX: &str = "blake3:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phenomenon {
    AbortedRead,
    IntermediateRead,
    GarbageRead,
    StaleRead,
    LostWrite,
    WriteCycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Complete,
    InsufficientData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationBinding {
    pub operation_id: String,
    pub operation_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Violation {
    pub phenomenon: Phenomenon,
    pub operations: Vec<OperationBinding>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhenomenaReport {
    pub schema_version: u32,
    pub checker: String,
    pub history_id: String,
    pub source_blake3: String,
    pub outcome: CheckOutcome,
    pub violations: Vec<Violation>,
    pub insufficient_pairs: Vec<ObservationGap>,
    pub checked_operations: usize,
    pub report_blake3: String,
    pub non_claims: Vec<String>,
}

pub fn check_history(history: &PhenomenaHistory) -> Result<PhenomenaReport, PhenomenaError> {
    validate_history(history)?;
    if !history.gaps.is_empty() {
        return finish_report(PhenomenaReport {
            schema_version: REPORT_SCHEMA_VERSION,
            checker: CHECKER_ID.to_string(),
            history_id: history.history_id.clone(),
            source_blake3: history.source_blake3.clone(),
            outcome: CheckOutcome::InsufficientData,
            violations: Vec::new(),
            insufficient_pairs: history.gaps.clone(),
            checked_operations: history.operations.len(),
            report_blake3: String::new(),
            non_claims: required_non_claims(),
        });
    }

    let operations = history
        .operations
        .iter()
        .map(|operation| (operation.operation_id.as_str(), operation))
        .collect::<BTreeMap<_, _>>();
    let mut violations = Vec::new();
    let mut dedup = BTreeSet::new();

    check_read_phenomena(history, &operations, &mut violations, &mut dedup)?;
    check_lost_writes(history, &operations, &mut violations, &mut dedup)?;
    if let Some(cycle) = find_write_cycle(history) {
        push_violation(
            Phenomenon::WriteCycle,
            cycle,
            "write-write dependencies contain a directed cycle",
            &operations,
            &mut violations,
            &mut dedup,
        )?;
    }

    finish_report(PhenomenaReport {
        schema_version: REPORT_SCHEMA_VERSION,
        checker: CHECKER_ID.to_string(),
        history_id: history.history_id.clone(),
        source_blake3: history.source_blake3.clone(),
        outcome: CheckOutcome::Complete,
        violations,
        insufficient_pairs: Vec::new(),
        checked_operations: history.operations.len(),
        report_blake3: String::new(),
        non_claims: required_non_claims(),
    })
}

pub fn validate_report_for_history(
    report: &PhenomenaReport,
    history: &PhenomenaHistory,
) -> Result<(), PhenomenaError> {
    let expected = check_history(history)?;
    if report != &expected {
        return Err(PhenomenaError::new(
            "phenomena-report-identity",
            "report fields, operation bindings, or BLAKE3 identity drifted",
        ));
    }
    Ok(())
}

fn check_read_phenomena(
    history: &PhenomenaHistory,
    operations: &BTreeMap<&str, &HistoryOperation>,
    violations: &mut Vec<Violation>,
    dedup: &mut BTreeSet<(Phenomenon, Vec<String>)>,
) -> Result<(), PhenomenaError> {
    for read in &history.operations {
        let OperationKind::Read { key, observation } = &read.kind else {
            continue;
        };
        match observation {
            ReadObservation::Initial => {}
            ReadObservation::Unattributed { .. } => {
                push_violation(
                    Phenomenon::GarbageRead,
                    vec![read.operation_id.clone()],
                    "read returned a value with no attributed write",
                    operations,
                    violations,
                    dedup,
                )?;
            }
            ReadObservation::Write {
                operation_id,
                version,
                ..
            } => {
                let write = operations.get(operation_id.as_str()).ok_or_else(|| {
                    PhenomenaError::new(
                        "read-observation",
                        format!("read references unknown write {operation_id:?}"),
                    )
                })?;
                match write.status {
                    OperationStatus::Aborted => push_violation(
                        Phenomenon::AbortedRead,
                        vec![write.operation_id.clone(), read.operation_id.clone()],
                        "committed observation reads from an aborted write",
                        operations,
                        violations,
                        dedup,
                    )?,
                    OperationStatus::Intermediate => push_violation(
                        Phenomenon::IntermediateRead,
                        vec![write.operation_id.clone(), read.operation_id.clone()],
                        "read observes an intermediate write state",
                        operations,
                        violations,
                        dedup,
                    )?,
                    OperationStatus::Committed => {}
                }
                if let Some(latest) = latest_visible_write(history, key, read.sequence, *version) {
                    push_violation(
                        Phenomenon::StaleRead,
                        vec![
                            write.operation_id.clone(),
                            latest.operation_id.clone(),
                            read.operation_id.clone(),
                        ],
                        "read observes an older version after a newer committed write",
                        operations,
                        violations,
                        dedup,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn latest_visible_write<'a>(
    history: &'a PhenomenaHistory,
    key: &str,
    read_sequence: u64,
    observed_version: u64,
) -> Option<&'a HistoryOperation> {
    history
        .operations
        .iter()
        .filter(|operation| {
            operation.sequence < read_sequence
                && operation.status == OperationStatus::Committed
                && matches!(
                    &operation.kind,
                    OperationKind::Write {
                        key: write_key,
                        version,
                        ..
                    } if write_key == key && *version > observed_version
                )
        })
        .max_by_key(|operation| match operation.kind {
            OperationKind::Write { version, .. } => version,
            OperationKind::Read { .. } => 0,
        })
}

fn check_lost_writes(
    history: &PhenomenaHistory,
    operations: &BTreeMap<&str, &HistoryOperation>,
    violations: &mut Vec<Violation>,
    dedup: &mut BTreeSet<(Phenomenon, Vec<String>)>,
) -> Result<(), PhenomenaError> {
    let mut by_key: BTreeMap<&str, Vec<&HistoryOperation>> = BTreeMap::new();
    for operation in &history.operations {
        if operation.status != OperationStatus::Committed {
            continue;
        }
        if let OperationKind::Write { key, .. } = &operation.kind {
            by_key.entry(key).or_default().push(operation);
        }
    }
    for writes in by_key.values_mut() {
        writes.sort_by_key(|operation| (operation.sequence, operation.operation_id.as_str()));
        for pair in writes.windows(DEPENDENCY_PAIR_SIZE) {
            let earlier = pair[0];
            let later = pair[1];
            if !has_dependency_path(&earlier.operation_id, &later.operation_id, operations) {
                push_violation(
                    Phenomenon::LostWrite,
                    vec![earlier.operation_id.clone(), later.operation_id.clone()],
                    "later committed write has no dependency path from the prior committed write",
                    operations,
                    violations,
                    dedup,
                )?;
            }
        }
    }
    Ok(())
}

fn has_dependency_path(
    predecessor: &str,
    target: &str,
    operations: &BTreeMap<&str, &HistoryOperation>,
) -> bool {
    let mut frontier = vec![target];
    let mut visited = BTreeSet::new();
    while let Some(current) = frontier.pop() {
        if !visited.insert(current) {
            continue;
        }
        let Some(operation) = operations.get(current) else {
            return false;
        };
        for dependency in &operation.dependencies {
            if dependency.predecessor == predecessor {
                return true;
            }
            frontier.push(dependency.predecessor.as_str());
        }
    }
    false
}

fn find_write_cycle(history: &PhenomenaHistory) -> Option<Vec<String>> {
    let writes = history
        .operations
        .iter()
        .filter(|operation| matches!(operation.kind, OperationKind::Write { .. }))
        .map(|operation| operation.operation_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut graph: BTreeMap<&str, Vec<&str>> = writes
        .iter()
        .copied()
        .map(|operation_id| (operation_id, Vec::new()))
        .collect();
    for operation in &history.operations {
        if !writes.contains(operation.operation_id.as_str()) {
            continue;
        }
        for dependency in &operation.dependencies {
            if dependency.kind == DependencyKind::WriteWrite
                && writes.contains(dependency.predecessor.as_str())
            {
                graph
                    .entry(dependency.predecessor.as_str())
                    .or_default()
                    .push(operation.operation_id.as_str());
            }
        }
    }
    for neighbours in graph.values_mut() {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    find_directed_cycle(&graph)
}

fn find_directed_cycle(graph: &BTreeMap<&str, Vec<&str>>) -> Option<Vec<String>> {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Visit {
        Visiting,
        Done,
    }

    let mut state = BTreeMap::new();
    for start in graph.keys().copied() {
        if state.get(start) == Some(&Visit::Done) {
            continue;
        }
        let mut stack = vec![(start, 0_usize)];
        let mut path = vec![start];
        let mut positions = BTreeMap::from([(start, 0_usize)]);
        state.insert(start, Visit::Visiting);
        while let Some((node, next_index)) = stack.last_mut() {
            let neighbours = graph.get(node).map_or(&[][..], Vec::as_slice);
            if *next_index >= neighbours.len() {
                let finished = *node;
                stack.pop();
                path.pop();
                positions.remove(finished);
                state.insert(finished, Visit::Done);
                continue;
            }
            let neighbour = neighbours[*next_index];
            *next_index = next_index.saturating_add(1);
            match state.get(neighbour) {
                Some(Visit::Visiting) => {
                    let position = *positions.get(neighbour)?;
                    return Some(
                        path[position..]
                            .iter()
                            .map(|item| (*item).to_string())
                            .collect(),
                    );
                }
                Some(Visit::Done) => {}
                None => {
                    state.insert(neighbour, Visit::Visiting);
                    positions.insert(neighbour, path.len());
                    path.push(neighbour);
                    stack.push((neighbour, 0));
                }
            }
        }
    }
    None
}

fn push_violation(
    phenomenon: Phenomenon,
    mut operation_ids: Vec<String>,
    detail: &str,
    operations: &BTreeMap<&str, &HistoryOperation>,
    violations: &mut Vec<Violation>,
    dedup: &mut BTreeSet<(Phenomenon, Vec<String>)>,
) -> Result<(), PhenomenaError> {
    operation_ids.sort_by(|left, right| {
        let left_sequence = operations
            .get(left.as_str())
            .map_or(u64::MAX, |operation| operation.sequence);
        let right_sequence = operations
            .get(right.as_str())
            .map_or(u64::MAX, |operation| operation.sequence);
        (left_sequence, left.as_str()).cmp(&(right_sequence, right.as_str()))
    });
    operation_ids.dedup();
    if !dedup.insert((phenomenon, operation_ids.clone())) {
        return Ok(());
    }
    if violations.len() >= MAX_VIOLATIONS {
        return Err(PhenomenaError::new(
            "phenomena-violation-bound",
            "violation count exceeds the supported bound",
        ));
    }
    let bindings = operation_ids
        .into_iter()
        .map(|operation_id| {
            let operation = operations.get(operation_id.as_str()).ok_or_else(|| {
                PhenomenaError::new(
                    "phenomena-operation-binding",
                    format!("operation {operation_id:?} is missing"),
                )
            })?;
            Ok(OperationBinding {
                operation_id,
                operation_blake3: operation_identity(operation)?,
            })
        })
        .collect::<Result<Vec<_>, PhenomenaError>>()?;
    violations.push(Violation {
        phenomenon,
        operations: bindings,
        detail: detail.to_string(),
    });
    Ok(())
}

fn finish_report(mut report: PhenomenaReport) -> Result<PhenomenaReport, PhenomenaError> {
    report.report_blake3 = report_identity(&report)?;
    Ok(report)
}

fn report_identity(report: &PhenomenaReport) -> Result<String, PhenomenaError> {
    #[derive(Serialize)]
    struct Material<'a> {
        schema_version: u32,
        checker: &'a str,
        history_id: &'a str,
        source_blake3: &'a str,
        outcome: CheckOutcome,
        violations: &'a [Violation],
        insufficient_pairs: &'a [ObservationGap],
        checked_operations: usize,
        non_claims: &'a [String],
    }
    let material = Material {
        schema_version: report.schema_version,
        checker: &report.checker,
        history_id: &report.history_id,
        source_blake3: &report.source_blake3,
        outcome: report.outcome,
        violations: &report.violations,
        insufficient_pairs: &report.insufficient_pairs,
        checked_operations: report.checked_operations,
        non_claims: &report.non_claims,
    };
    let bytes = serde_json::to_vec(&material)
        .map_err(|error| PhenomenaError::new("report-serialization", error.to_string()))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(REPORT_IDENTITY_DOMAIN);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(&bytes);
    Ok(format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex()))
}

fn required_non_claims() -> Vec<String> {
    REQUIRED_NON_CLAIMS
        .iter()
        .map(|item| (*item).to_string())
        .collect()
}
