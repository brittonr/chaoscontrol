use chaoscontrol_evidence::{
    check_consistency_phenomena_path, check_phenomena_history_path, read_phenomena_history_path,
    write_phenomena_report_path, RegisterHistoryAdapterConfig, RegisterWorkloadHistoryAdapter,
};
use chaoscontrol_smr::phenomena::{
    bind_history, Dependency, DependencyKind, HistoryOperation, OperationKind, OperationStatus,
    Phenomenon,
};
use std::os::unix::fs::symlink;

const SOURCE_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FIRST_VERSION: u64 = 1;
const SECOND_VERSION: u64 = 2;
const FIRST_VALUE: i64 = 1;
const SECOND_VALUE: i64 = 2;
const ROUND_OPERATION_COUNT: usize = 2;
const SECOND_WRITE_INVOKED_AT: u64 = 3;
const SECOND_WRITE_COMPLETED_AT: u64 = 4;

fn write(
    operation_id: &str,
    sequence: u64,
    version: u64,
    dependencies: Vec<Dependency>,
) -> HistoryOperation {
    HistoryOperation {
        operation_id: operation_id.to_string(),
        process: format!("process-{operation_id}"),
        sequence,
        status: OperationStatus::Committed,
        kind: OperationKind::Write {
            key: "register".to_string(),
            version,
            value: format!("value-{version}"),
        },
        dependencies,
    }
}

fn write_history(path: &std::path::Path, causal: bool) {
    let dependencies = if causal {
        vec![Dependency {
            predecessor: "write-one".to_string(),
            kind: DependencyKind::WriteWrite,
        }]
    } else {
        Vec::new()
    };
    let history = bind_history(
        "shell-fixture",
        SOURCE_DIGEST,
        vec![
            write("write-one", 0, FIRST_VERSION, Vec::new()),
            write("write-two", 1, SECOND_VERSION, dependencies),
        ],
        Vec::new(),
    )
    .expect("history binds");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&history).expect("history serializes"),
    )
    .expect("history writes");
}

#[test]
fn shell_reads_checks_and_publishes_bound_report() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let history_path = directory.path().join("history.json");
    let report_path = directory.path().join("report.json");
    write_history(&history_path, false);

    let history = read_phenomena_history_path(&history_path).expect("history reads");
    let report = check_phenomena_history_path(&history_path).expect("history checks");
    assert_eq!(report.history_id, history.history_id);
    assert!(report
        .violations
        .iter()
        .any(|violation| violation.phenomenon == Phenomenon::LostWrite));
    write_phenomena_report_path(&history_path, &report_path).expect("report writes");
    assert!(report_path.is_file());
    assert!(write_phenomena_report_path(&history_path, &report_path).is_err());
}

#[test]
fn shell_adapts_existing_typed_round_history() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source_path = directory.path().join("round-history.json");
    let mut adapter = RegisterWorkloadHistoryAdapter::new(RegisterHistoryAdapterConfig {
        history_id: "round-history".to_string(),
        workload: "register-fixture".to_string(),
        source_artifact: "typed-round-events".to_string(),
        limitations: vec!["fixture uses typed operation events".to_string()],
    })
    .expect("adapter");
    adapter
        .record_write_ok("write-one", "client-a", 0, 1, FIRST_VALUE)
        .expect("first write");
    adapter
        .record_write_ok(
            "write-two",
            "client-b",
            SECOND_WRITE_INVOKED_AT,
            SECOND_WRITE_COMPLETED_AT,
            SECOND_VALUE,
        )
        .expect("second write");
    let source = adapter.emit_history().expect("typed round history");
    std::fs::write(
        &source_path,
        serde_json::to_vec_pretty(&source).expect("source serializes"),
    )
    .expect("source writes");

    let report = check_consistency_phenomena_path(&source_path).expect("round history checks");
    assert!(report.violations.is_empty());
    assert_eq!(report.checked_operations, ROUND_OPERATION_COUNT);
}

#[test]
fn shell_rejects_symlinks_and_history_identity_drift() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let history_path = directory.path().join("history.json");
    let link_path = directory.path().join("history-link.json");
    write_history(&history_path, true);
    symlink(&history_path, &link_path).expect("history symlink");
    assert!(read_phenomena_history_path(&link_path).is_err());

    let mut history: chaoscontrol_smr::phenomena::PhenomenaHistory =
        serde_json::from_slice(&std::fs::read(&history_path).expect("history bytes"))
            .expect("history parses");
    history.history_id =
        "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    std::fs::write(
        &history_path,
        serde_json::to_vec_pretty(&history).expect("drifted history serializes"),
    )
    .expect("drifted history writes");
    assert!(check_phenomena_history_path(&history_path).is_err());
}
