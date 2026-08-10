// r[impl chaoscontrol.property_coverage.fault_assertion]
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::{OracleSnapshot, PropertyOracle};
use chaoscontrol_fault::report_merge::merge_oracle_reports;
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use chaoscontrol_protocol::admission::{
    token_for_descriptors, BoundAssertionEvent, CatalogBuilder, CatalogValidationStatus,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use serde::{Deserialize, Serialize};

use crate::framework::{run_generated, DeterministicRng, Failure, PropertyProfile, SuiteReport};

const SUITE: &str = "fault-assertion";
const INITIAL_FAULTS: usize = 5;
const FAULT_TIME_QUANTUM_NS: u64 = 10;
const FAULT_TIME_RANGE_NS: u64 = 80;
const VALID_SUBSET_SECOND_INDEX: usize = 2;
const COMMAND_VARIANTS: usize = 12;
const COMMAND_DRAIN: usize = 0;
const COMMAND_RESET: usize = 1;
const COMMAND_VALID_SUBSET: usize = 2;
const COMMAND_DUPLICATE_SUBSET: usize = 3;
const COMMAND_OUT_OF_BOUNDS_SUBSET: usize = 4;
const COMMAND_ASSERT_TRUE: usize = 5;
const COMMAND_ASSERT_FALSE: usize = 6;
const COMMAND_ASSERT_WRONG_TOKEN: usize = 7;
const COMMAND_END_BEGIN_RUN: usize = 8;
const COMMAND_SAVE_ORACLE: usize = 9;
const COMMAND_RESTORE_ORACLE: usize = 10;
const COMMAND_MERGE_REPORT: usize = 11;
const FAULT_KIND_COUNT: usize = 3;
const FAULT_KIND_PARTITION: usize = 0;
const FAULT_KIND_HEAL: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Drain { time_ns: u64 },
    Reset,
    ValidSubset,
    DuplicateSubset,
    OutOfBoundsSubset,
    AssertTrue,
    AssertFalse,
    AssertWrongToken,
    EndBeginRun,
    SaveOracle,
    RestoreOracle,
    MergeReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScheduleModel {
    faults: Vec<ScheduledFault>,
    cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OracleModel {
    true_count: u64,
    false_count: u64,
    fatal_conflict: bool,
    completed_runs: u32,
    saved: Option<OracleModelSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OracleModelSnapshot {
    true_count: u64,
    false_count: u64,
    fatal_conflict: bool,
    completed_runs: u32,
}

pub fn run(selected: &PropertyProfile) -> Result<SuiteReport, crate::AnyCounterexample> {
    run_generated(SUITE, selected, generate, check)
        .map_err(crate::AnyCounterexample::fault_assertion)
}

fn generate(rng: &mut DeterministicRng) -> Command {
    match rng.index(COMMAND_VARIANTS) {
        COMMAND_DRAIN => Command::Drain {
            time_ns: rng.bounded_u64(FAULT_TIME_RANGE_NS),
        },
        COMMAND_RESET => Command::Reset,
        COMMAND_VALID_SUBSET => Command::ValidSubset,
        COMMAND_DUPLICATE_SUBSET => Command::DuplicateSubset,
        COMMAND_OUT_OF_BOUNDS_SUBSET => Command::OutOfBoundsSubset,
        COMMAND_ASSERT_TRUE => Command::AssertTrue,
        COMMAND_ASSERT_FALSE => Command::AssertFalse,
        COMMAND_ASSERT_WRONG_TOKEN => Command::AssertWrongToken,
        COMMAND_END_BEGIN_RUN => Command::EndBeginRun,
        COMMAND_SAVE_ORACLE => Command::SaveOracle,
        COMMAND_RESTORE_ORACLE => Command::RestoreOracle,
        COMMAND_MERGE_REPORT => Command::MergeReport,
        _ => unreachable!("bounded command selector must produce a known fault command"),
    }
}

fn initial_schedule() -> (FaultSchedule, ScheduleModel) {
    let mut actual = FaultSchedule::new();
    let mut faults = Vec::with_capacity(INITIAL_FAULTS);
    for target in 0..INITIAL_FAULTS {
        let reverse = INITIAL_FAULTS - target;
        let time_ns =
            u64::try_from(reverse).expect("fault count must fit in u64") * FAULT_TIME_QUANTUM_NS;
        let fault = match target % FAULT_KIND_COUNT {
            FAULT_KIND_PARTITION => Fault::NetworkPartition {
                side_a: vec![0],
                side_b: vec![1],
            },
            FAULT_KIND_HEAL => Fault::NetworkHeal,
            _ => Fault::ProcessKill { target },
        };
        let scheduled = ScheduledFault::new(time_ns, fault).with_label(format!("fault-{target}"));
        actual.add(scheduled.clone());
        faults.push(scheduled);
    }
    faults.sort_by_key(|fault| fault.time_ns);
    (actual, ScheduleModel { faults, cursor: 0 })
}

fn assertion_authority() -> (
    PropertyOracle,
    chaoscontrol_protocol::admission::AcceptedCatalog,
    BoundAssertionEvent,
    AssertionFingerprint,
) {
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.onixresearch.property-suite".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "state-machine-invariant".to_string(),
        },
        compatibility_id: None,
        kind: AssertionKind::Always,
        message: "state machine invariant holds".to_string(),
        source_file: "crates/chaoscontrol-property-suite/src/fault_assertion.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "property-suite".to_string(),
        category: "safety".to_string(),
    };
    let fingerprint = descriptor
        .fingerprint()
        .expect("the property assertion descriptor must be valid");
    let token = token_for_descriptors(std::slice::from_ref(&descriptor))
        .expect("the property assertion catalog token must be valid");
    let mut builder = CatalogBuilder::begin(1).expect("one assertion must be admitted");
    builder
        .insert(descriptor)
        .expect("the property assertion descriptor must be admitted");
    let catalog = builder
        .complete(token)
        .expect("the property assertion catalog must be complete");
    let event = BoundAssertionEvent {
        catalog_token: token,
        fingerprint,
        kind: AssertionKind::Always,
    };
    let mut oracle = PropertyOracle::new();
    oracle
        .activate_catalog(catalog.clone())
        .expect("the property assertion catalog must activate");
    oracle.begin_run();
    (oracle, catalog, event, fingerprint)
}

fn check(commands: &[Command]) -> Result<usize, Failure> {
    let (mut schedule, mut schedule_model) = initial_schedule();
    let (mut oracle, catalog, event, fingerprint) = assertion_authority();
    let mut oracle_model = OracleModel {
        true_count: 0,
        false_count: 0,
        fatal_conflict: false,
        completed_runs: 0,
        saved: None,
    };
    let mut saved_oracle: Option<OracleSnapshot> = None;
    let mut rejected = 0_usize;

    for (step, command) in commands.iter().enumerate() {
        match command {
            Command::Drain { time_ns } => {
                let actual_due = schedule.drain_due_indexed(*time_ns);
                let mut expected_due = Vec::new();
                while schedule_model.cursor < schedule_model.faults.len()
                    && schedule_model.faults[schedule_model.cursor].time_ns <= *time_ns
                {
                    expected_due.push(schedule_model.faults[schedule_model.cursor].clone());
                    schedule_model.cursor += 1;
                }
                let first_expected_index = schedule_model.cursor - expected_due.len();
                let expected_projection = expected_due
                    .iter()
                    .enumerate()
                    .map(|(offset, fault)| (first_expected_index + offset, fault.clone()))
                    .collect::<Vec<_>>();
                if actual_due != expected_projection {
                    return Err(Failure::new(
                        "fault-delivery-reference-agreement",
                        step,
                        format!("actual={actual_due:?}, expected={expected_projection:?}"),
                    ));
                }
            }
            Command::Reset => {
                schedule.reset();
                schedule_model.cursor = 0;
            }
            Command::ValidSubset => {
                let subset = schedule
                    .subset(&[0, VALID_SUBSET_SECOND_INDEX])
                    .map_err(|error| Failure::new("valid-fault-subset", step, error.to_string()))?;
                if subset.total() != VALID_SUBSET_SECOND_INDEX
                    || subset.entry(0) != schedule.entry(0)
                    || subset.entry(1) != schedule.entry(VALID_SUBSET_SECOND_INDEX)
                {
                    return Err(Failure::new(
                        "fault-subset-reference-agreement",
                        step,
                        "a valid subset does not preserve selected canonical entries",
                    ));
                }
            }
            Command::DuplicateSubset => {
                let before = schedule.identity();
                if schedule.subset(&[0, 0]).is_ok() || schedule.identity() != before {
                    return Err(Failure::new(
                        "duplicate-subset-no-mutation",
                        step,
                        "a duplicate subset succeeded or changed the source schedule",
                    ));
                }
                rejected += 1;
            }
            Command::OutOfBoundsSubset => {
                let before = schedule.identity();
                if schedule.subset(&[INITIAL_FAULTS]).is_ok() || schedule.identity() != before {
                    return Err(Failure::new(
                        "out-of-bounds-subset-no-mutation",
                        step,
                        "an out-of-bounds subset succeeded or changed the source schedule",
                    ));
                }
                rejected += 1;
            }
            Command::AssertTrue => {
                oracle
                    .record_bound_event(&event, true, None)
                    .map_err(|error| {
                        Failure::new("valid-assertion-continuation", step, format!("{error:?}"))
                    })?;
                oracle_model.true_count += 1;
            }
            Command::AssertFalse => {
                let satisfied =
                    oracle
                        .record_bound_event(&event, false, None)
                        .map_err(|error| {
                            Failure::new("valid-failing-assertion", step, format!("{error:?}"))
                        })?;
                if satisfied {
                    return Err(Failure::new(
                        "assertion-condition-semantics",
                        step,
                        "a false always assertion was reported as satisfied",
                    ));
                }
                oracle_model.false_count += 1;
            }
            Command::AssertWrongToken => {
                let before = oracle.structured_assertions()[&fingerprint].clone();
                let mut invalid = event.clone();
                invalid.catalog_token.0[0] ^= 1;
                if oracle.record_bound_event(&invalid, true, None).is_ok() {
                    return Err(Failure::new(
                        "assertion-evidence-binding",
                        step,
                        "an assertion with the wrong catalog token was accepted",
                    ));
                }
                let after = &oracle.structured_assertions()[&fingerprint];
                if after.hit_count != before.hit_count
                    || after.true_count != before.true_count
                    || after.false_count != before.false_count
                {
                    return Err(Failure::new(
                        "invalid-assertion-no-counter-mutation",
                        step,
                        "a rejected assertion changed coverage counters",
                    ));
                }
                oracle_model.fatal_conflict = true;
                rejected += 1;
            }
            Command::EndBeginRun => {
                oracle.end_run();
                oracle.begin_run();
                oracle_model.completed_runs += 1;
            }
            Command::SaveOracle => {
                saved_oracle = Some(oracle.snapshot());
                oracle_model.saved = Some(OracleModelSnapshot {
                    true_count: oracle_model.true_count,
                    false_count: oracle_model.false_count,
                    fatal_conflict: oracle_model.fatal_conflict,
                    completed_runs: oracle_model.completed_runs,
                });
            }
            Command::RestoreOracle => match (&saved_oracle, oracle_model.saved.clone()) {
                (Some(snapshot), Some(model_snapshot)) if !model_snapshot.fatal_conflict => {
                    oracle.restore(snapshot).map_err(|error| {
                        Failure::new("valid-oracle-snapshot-restore", step, format!("{error:?}"))
                    })?;
                    oracle_model.true_count = model_snapshot.true_count;
                    oracle_model.false_count = model_snapshot.false_count;
                    oracle_model.fatal_conflict = model_snapshot.fatal_conflict;
                    oracle_model.completed_runs = model_snapshot.completed_runs;
                }
                (Some(snapshot), Some(model_snapshot)) => {
                    let before = oracle.finalized_report_projection();
                    if oracle.restore(snapshot).is_ok()
                        || oracle.finalized_report_projection() != before
                    {
                        return Err(Failure::new(
                            "invalid-oracle-snapshot-no-mutation",
                            step,
                            "a fatal oracle snapshot restored or changed live state",
                        ));
                    }
                    debug_assert!(model_snapshot.fatal_conflict);
                    rejected += 1;
                }
                (None, None) => rejected += 1,
                _ => {
                    return Err(Failure::new(
                        "oracle-snapshot-model-alignment",
                        step,
                        "actual and model oracle snapshot availability diverged",
                    ));
                }
            },
            Command::MergeReport => {
                let report = oracle.finalized_report_projection();
                let merged = merge_oracle_reports(&[(0, report.clone()), (1, report)]);
                if oracle_model.fatal_conflict {
                    if merged.is_ok() {
                        return Err(Failure::new(
                            "fatal-assertion-merge-rejection",
                            step,
                            "a fatal assertion report was merged",
                        ));
                    }
                    rejected += 1;
                } else {
                    let merged = merged.map_err(|error| {
                        Failure::new("valid-assertion-merge", step, format!("{error:?}"))
                    })?;
                    let record = &merged.structured_assertions[&fingerprint];
                    let expected_multiplier = 2_u64;
                    if record.true_count != oracle_model.true_count * expected_multiplier
                        || record.false_count != oracle_model.false_count * expected_multiplier
                    {
                        return Err(Failure::new(
                            "assertion-merge-reference-agreement",
                            step,
                            format!("merged={record:?}, model={oracle_model:?}"),
                        ));
                    }
                }
            }
        }
        compare_schedule(step, &schedule, &schedule_model)?;
        compare_oracle(step, &oracle, fingerprint, &oracle_model)?;
        if catalog.token != event.catalog_token {
            return Err(Failure::new(
                "catalog-event-binding",
                step,
                "the retained event no longer binds its accepted catalog",
            ));
        }
    }
    Ok(rejected)
}

fn compare_schedule(
    step: usize,
    actual: &FaultSchedule,
    model: &ScheduleModel,
) -> Result<(), Failure> {
    let expected_remaining = model.faults.len() - model.cursor;
    let expected_next = model.faults.get(model.cursor).map(|fault| fault.time_ns);
    if actual.remaining() != expected_remaining || actual.next_time() != expected_next {
        return Err(Failure::new(
            "fault-schedule-state-agreement",
            step,
            format!(
                "actual_remaining={}, expected_remaining={expected_remaining}, actual_next={:?}, expected_next={expected_next:?}",
                actual.remaining(),
                actual.next_time()
            ),
        ));
    }
    Ok(())
}

fn compare_oracle(
    step: usize,
    actual: &PropertyOracle,
    fingerprint: AssertionFingerprint,
    model: &OracleModel,
) -> Result<(), Failure> {
    let record = &actual.structured_assertions()[&fingerprint];
    let expected_hits = model.true_count + model.false_count;
    let expected_status = if model.fatal_conflict {
        CatalogValidationStatus::FatalConflict
    } else {
        CatalogValidationStatus::Accepted
    };
    if record.hit_count != expected_hits
        || record.true_count != model.true_count
        || record.false_count != model.false_count
        || actual.catalog_status() != expected_status
        || actual.total_runs() != model.completed_runs
    {
        return Err(Failure::new(
            "assertion-oracle-reference-agreement",
            step,
            format!("record={record:?}, model={model:?}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_invalid_command_regressions() {
        let commands: Vec<Command> = serde_json::from_str(include_str!(
            "../../../contracts/property-coverage/fixtures/regressions/fault-assertion-invalid-continuation.json"
        ))
        .expect("the fault and assertion regression fixture must be valid JSON");
        assert!(check(&commands).is_ok());
    }
}
