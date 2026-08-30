use chaoscontrol_smr::phenomena::{
    bind_history, check_history, validate_history, validate_report_for_history, CheckOutcome,
    Dependency, DependencyKind, HistoryOperation, ObservationGap, OperationKind, OperationStatus,
    Phenomenon, ReadObservation,
};

const SOURCE_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FIRST_VERSION: u64 = 1;
const SECOND_VERSION: u64 = 2;
const THIRD_SEQUENCE: u64 = 2;
const ABORTED_READ_BINDING_COUNT: usize = 2;

fn write(
    operation_id: &str,
    sequence: u64,
    status: OperationStatus,
    version: u64,
    dependencies: Vec<Dependency>,
) -> HistoryOperation {
    HistoryOperation {
        operation_id: operation_id.to_string(),
        process: format!("process-{operation_id}"),
        sequence,
        status,
        kind: OperationKind::Write {
            key: "register".to_string(),
            version,
            value: format!("value-{version}"),
        },
        dependencies,
    }
}

fn read_from(operation_id: &str, sequence: u64, write_id: &str, version: u64) -> HistoryOperation {
    HistoryOperation {
        operation_id: operation_id.to_string(),
        process: format!("process-{operation_id}"),
        sequence,
        status: OperationStatus::Committed,
        kind: OperationKind::Read {
            key: "register".to_string(),
            observation: ReadObservation::Write {
                operation_id: write_id.to_string(),
                version,
                value: format!("value-{version}"),
            },
        },
        dependencies: vec![Dependency {
            predecessor: write_id.to_string(),
            kind: DependencyKind::WriteRead,
        }],
    }
}

fn history(operations: Vec<HistoryOperation>) -> chaoscontrol_smr::phenomena::PhenomenaHistory {
    bind_history("fixture", SOURCE_DIGEST, operations, Vec::new()).expect("history binds")
}

fn phenomena(report: &chaoscontrol_smr::phenomena::PhenomenaReport) -> Vec<Phenomenon> {
    report
        .violations
        .iter()
        .map(|violation| violation.phenomenon)
        .collect()
}

#[test]
fn detects_aborted_read_with_bound_operations() {
    let history = history(vec![
        write(
            "write-aborted",
            0,
            OperationStatus::Aborted,
            FIRST_VERSION,
            Vec::new(),
        ),
        read_from("read", 1, "write-aborted", FIRST_VERSION),
    ]);
    let report = check_history(&history).expect("check");
    assert!(phenomena(&report).contains(&Phenomenon::AbortedRead));
    let violation = report
        .violations
        .iter()
        .find(|item| item.phenomenon == Phenomenon::AbortedRead)
        .expect("aborted-read violation");
    assert_eq!(violation.operations.len(), ABORTED_READ_BINDING_COUNT);
    assert!(violation
        .operations
        .iter()
        .all(|binding| binding.operation_blake3.starts_with("blake3:")));
}

#[test]
fn detects_intermediate_read() {
    let history = history(vec![
        write(
            "write-intermediate",
            0,
            OperationStatus::Intermediate,
            FIRST_VERSION,
            Vec::new(),
        ),
        read_from("read", 1, "write-intermediate", FIRST_VERSION),
    ]);
    assert!(
        phenomena(&check_history(&history).expect("check")).contains(&Phenomenon::IntermediateRead)
    );
}

#[test]
fn detects_garbage_read() {
    let read = HistoryOperation {
        operation_id: "read-garbage".to_string(),
        process: "reader".to_string(),
        sequence: 0,
        status: OperationStatus::Committed,
        kind: OperationKind::Read {
            key: "register".to_string(),
            observation: ReadObservation::Unattributed {
                value: "unknown".to_string(),
            },
        },
        dependencies: Vec::new(),
    };
    assert!(
        phenomena(&check_history(&history(vec![read])).expect("check"))
            .contains(&Phenomenon::GarbageRead)
    );
}

#[test]
fn detects_stale_read() {
    let first = write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    );
    let second = write(
        "write-two",
        1,
        OperationStatus::Committed,
        SECOND_VERSION,
        vec![Dependency {
            predecessor: "write-one".to_string(),
            kind: DependencyKind::WriteWrite,
        }],
    );
    let stale = read_from("read-stale", THIRD_SEQUENCE, "write-one", FIRST_VERSION);
    assert!(
        phenomena(&check_history(&history(vec![first, second, stale])).expect("check"))
            .contains(&Phenomenon::StaleRead)
    );
}

#[test]
fn detects_lost_write_without_a_dependency_path() {
    let first = write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    );
    let second = write(
        "write-two",
        1,
        OperationStatus::Committed,
        SECOND_VERSION,
        Vec::new(),
    );
    assert!(
        phenomena(&check_history(&history(vec![first, second])).expect("check"))
            .contains(&Phenomenon::LostWrite)
    );
}

#[test]
fn detects_write_cycle() {
    let first = write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        vec![Dependency {
            predecessor: "write-two".to_string(),
            kind: DependencyKind::WriteWrite,
        }],
    );
    let second = write(
        "write-two",
        1,
        OperationStatus::Committed,
        SECOND_VERSION,
        vec![Dependency {
            predecessor: "write-one".to_string(),
            kind: DependencyKind::WriteWrite,
        }],
    );
    let report = check_history(&history(vec![first, second])).expect("check");
    assert!(phenomena(&report).contains(&Phenomenon::WriteCycle));
}

#[test]
fn clean_history_has_no_violations() {
    let first = write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    );
    let second = write(
        "write-two",
        1,
        OperationStatus::Committed,
        SECOND_VERSION,
        vec![Dependency {
            predecessor: "write-one".to_string(),
            kind: DependencyKind::WriteWrite,
        }],
    );
    let read = read_from("read-latest", THIRD_SEQUENCE, "write-two", SECOND_VERSION);
    let report = check_history(&history(vec![first, second, read])).expect("check");
    assert_eq!(report.outcome, CheckOutcome::Complete);
    assert!(report.violations.is_empty());
}

#[test]
fn observation_gap_returns_bounded_insufficient_data() {
    let first = write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    );
    let second = write(
        "write-two",
        1,
        OperationStatus::Committed,
        SECOND_VERSION,
        Vec::new(),
    );
    let history = bind_history(
        "fixture",
        SOURCE_DIGEST,
        vec![first, second],
        vec![ObservationGap {
            left_operation: "write-one".to_string(),
            right_operation: "write-two".to_string(),
            reason: "observer lost ordering event".to_string(),
        }],
    )
    .expect("history binds");
    let report = check_history(&history).expect("check");
    assert_eq!(report.outcome, CheckOutcome::InsufficientData);
    assert!(report.violations.is_empty());
    assert_eq!(report.insufficient_pairs.len(), 1);
}

#[test]
fn rejects_unclassifiable_records_and_identity_drift() {
    let mut invalid = write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    );
    invalid.operation_id.clear();
    let error = bind_history("fixture", SOURCE_DIGEST, vec![invalid], Vec::new())
        .expect_err("missing identity");
    assert!(error.operation_id.is_some());

    let bound_history = history(vec![write(
        "write-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    )]);
    let mut report = check_history(&bound_history).expect("check");
    report.history_id =
        "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    assert!(validate_report_for_history(&report, &bound_history).is_err());

    let first = write(
        "write-order-one",
        0,
        OperationStatus::Committed,
        FIRST_VERSION,
        Vec::new(),
    );
    let second = write(
        "write-order-two",
        1,
        OperationStatus::Committed,
        SECOND_VERSION,
        vec![Dependency {
            predecessor: "write-order-one".to_string(),
            kind: DependencyKind::WriteWrite,
        }],
    );
    let mut reordered = history(vec![first, second]);
    reordered.operations.swap(0, 1);
    assert_eq!(
        validate_history(&reordered)
            .expect_err("non-canonical operation order")
            .class,
        "history-order"
    );
}
