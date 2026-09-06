use sha2::Digest;

pub const HISTORY_SCHEMA_VERSION: u64 = 1;
pub const CHECKER_REPORT_SCHEMA_VERSION: u64 = 1;
pub const REGISTER_MODEL: &str = "single-register-sequential";
const REQUIRED_SCOPE_FRAGMENT: &str = "not snapshot replay proof";

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct OperationHistory {
    pub schema_version: u64,
    pub history_id: String,
    pub workload: String,
    pub model: String,
    pub source_artifact: String,
    pub operations: Vec<HistoryOperation>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct RegisterHistoryAdapterConfig {
    pub history_id: String,
    pub workload: String,
    pub source_artifact: String,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct RegisterWorkloadHistoryAdapter {
    config: RegisterHistoryAdapterConfig,
    operations: Vec<HistoryOperation>,
}

impl RegisterWorkloadHistoryAdapter {
    pub fn new(config: RegisterHistoryAdapterConfig) -> crate::EvidenceResult<Self> {
        validate_adapter_config(&config)?;
        Ok(Self {
            config,
            operations: Vec::new(),
        })
    }

    pub fn record_write_ok(
        &mut self,
        operation_id: impl Into<String>,
        process: impl Into<String>,
        invoked_at: u64,
        completed_at: u64,
        value: i64,
    ) -> crate::EvidenceResult<()> {
        self.record_operation(HistoryOperation {
            operation_id: operation_id.into(),
            process: process.into(),
            invoked_at,
            completed_at,
            invocation: OperationInvocation::Write { value },
            completion: OperationCompletion::Ok { value: None },
        })
    }

    pub fn record_read_ok(
        &mut self,
        operation_id: impl Into<String>,
        process: impl Into<String>,
        invoked_at: u64,
        completed_at: u64,
        value: Option<i64>,
    ) -> crate::EvidenceResult<()> {
        self.record_operation(HistoryOperation {
            operation_id: operation_id.into(),
            process: process.into(),
            invoked_at,
            completed_at,
            invocation: OperationInvocation::Read,
            completion: OperationCompletion::Ok { value },
        })
    }

    pub fn record_failed(
        &mut self,
        operation_id: impl Into<String>,
        process: impl Into<String>,
        invoked_at: u64,
        completed_at: u64,
        invocation: OperationInvocation,
        error: impl Into<String>,
    ) -> crate::EvidenceResult<()> {
        self.record_operation(HistoryOperation {
            operation_id: operation_id.into(),
            process: process.into(),
            invoked_at,
            completed_at,
            invocation,
            completion: OperationCompletion::Failed {
                error: error.into(),
            },
        })
    }

    pub fn record_operation(&mut self, operation: HistoryOperation) -> crate::EvidenceResult<()> {
        validate_operation(&operation)?;
        require(
            !self
                .operations
                .iter()
                .any(|existing| existing.operation_id == operation.operation_id),
            format!("duplicate operation_id {:?}", operation.operation_id),
        )?;
        self.operations.push(operation);
        Ok(())
    }

    pub fn emit_history(self) -> crate::EvidenceResult<OperationHistory> {
        require(
            !self.operations.is_empty(),
            "adapter history must contain at least one typed operation",
        )?;
        let mut limitations = self.config.limitations;
        if !limitations
            .iter()
            .any(|item| item.contains("not parsed from raw logs"))
        {
            limitations.push(
                "emitted from typed workload adapter events; not parsed from raw logs".to_string(),
            );
        }
        let history = OperationHistory {
            schema_version: HISTORY_SCHEMA_VERSION,
            history_id: self.config.history_id,
            workload: self.config.workload,
            model: REGISTER_MODEL.to_string(),
            source_artifact: self.config.source_artifact,
            operations: self.operations,
            limitations,
        };
        validate_history(&history)?;
        Ok(history)
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct HistoryOperation {
    pub operation_id: String,
    pub process: String,
    pub invoked_at: u64,
    pub completed_at: u64,
    pub invocation: OperationInvocation,
    pub completion: OperationCompletion,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationInvocation {
    Read,
    Write { value: i64 },
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationCompletion {
    Ok { value: Option<i64> },
    Failed { error: String },
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ConsistencyCheckReport {
    pub schema_version: u64,
    pub checker: String,
    pub model: String,
    pub history_id: String,
    pub history_sha256: String,
    pub checked_operations: usize,
    pub verdict: CheckerVerdict,
    pub limitations: Vec<String>,
    pub counterexample: Option<Counterexample>,
    pub scope: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckerVerdict {
    Passed,
    Failed,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct Counterexample {
    pub operation_ids: Vec<String>,
    pub reason: String,
}

pub trait ConsistencyChecker {
    fn name(&self) -> &'static str;
    fn model(&self) -> &'static str;
    fn check(&self, history: &OperationHistory) -> crate::EvidenceResult<ConsistencyCheckReport>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SingleRegisterChecker;

impl ConsistencyChecker for SingleRegisterChecker {
    fn name(&self) -> &'static str {
        "single-register-checker"
    }

    fn model(&self) -> &'static str {
        REGISTER_MODEL
    }

    fn check(&self, history: &OperationHistory) -> crate::EvidenceResult<ConsistencyCheckReport> {
        validate_history(history)?;
        require(
            history.model == self.model(),
            format!(
                "unsupported checker model: history model {:?}, checker model {:?}",
                history.model,
                self.model()
            ),
        )?;

        let history_sha256 = history_digest(history)?;
        let mut value: Option<i64> = None;
        let mut counterexample = None;
        let mut checked = 0usize;
        let mut ops = history.operations.clone();
        ops.sort_by_key(|op| (op.completed_at, op.invoked_at, op.operation_id.clone()));

        for op in &ops {
            match (&op.invocation, &op.completion) {
                (
                    OperationInvocation::Write { value: written },
                    OperationCompletion::Ok { value: _ },
                ) => {
                    value = Some(*written);
                    checked += 1;
                }
                (OperationInvocation::Read, OperationCompletion::Ok { value: observed }) => {
                    checked += 1;
                    if observed != &value {
                        counterexample = Some(Counterexample {
                            operation_ids: vec![op.operation_id.clone()],
                            reason: format!(
                                "read observed {observed:?} but completion-order register value was {value:?}"
                            ),
                        });
                        break;
                    }
                }
                (_, OperationCompletion::Failed { .. }) => {}
            }
        }

        let verdict = if counterexample.is_some() {
            CheckerVerdict::Failed
        } else {
            CheckerVerdict::Passed
        };
        Ok(ConsistencyCheckReport {
            schema_version: CHECKER_REPORT_SCHEMA_VERSION,
            checker: self.name().to_string(),
            model: self.model().to_string(),
            history_id: history.history_id.clone(),
            history_sha256,
            checked_operations: checked,
            verdict,
            limitations: vec![
                "single-register completion-order checker; not a full Jepsen linearizability proof".to_string(),
                "semantic checker evidence is not snapshot replay proof by itself".to_string(),
            ],
            counterexample,
            scope: "bounded semantic consistency-checker evidence; not snapshot replay proof, not deterministic replay proof, not hosted-product parity".to_string(),
        })
    }
}

pub fn validate_history_path(path: impl AsRef<std::path::Path>) -> crate::EvidenceResult<String> {
    let history = read_history_path(path)?;
    validate_history(&history)?;
    Ok(format!(
        "history={} model={} operations={} source={}",
        history.history_id,
        history.model,
        history.operations.len(),
        history.source_artifact
    ))
}

pub fn check_history_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<ConsistencyCheckReport> {
    let history = read_history_path(path)?;
    SingleRegisterChecker.check(&history)
}

pub fn write_adapter_sample_history_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut adapter = RegisterWorkloadHistoryAdapter::new(RegisterHistoryAdapterConfig {
        history_id: "adapter-register-good".to_string(),
        workload: "register-adapter-fixture".to_string(),
        source_artifact: path.display().to_string(),
        limitations: vec!["fixture emitted by typed workload adapter".to_string()],
    })?;
    adapter.record_write_ok("adapter-write-1", "client-a", 1, 2, 7)?;
    adapter.record_read_ok("adapter-read-1", "client-b", 3, 4, Some(7))?;
    let history = adapter.emit_history()?;
    std::fs::write(path, serde_json::to_vec_pretty(&history)?)?;
    Ok(())
}

pub fn write_sample_history_path(
    path: impl AsRef<std::path::Path>,
    bad: bool,
) -> crate::EvidenceResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let history = if bad {
        sample_bad_history()
    } else {
        sample_good_history()
    };
    validate_history(&history)?;
    std::fs::write(path, serde_json::to_vec_pretty(&history)?)?;
    Ok(())
}

pub fn write_check_report_path(
    history_path: impl AsRef<std::path::Path>,
    report_path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<()> {
    let report_path = report_path.as_ref();
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let report = check_history_path(history_path)?;
    validate_report(&report)?;
    std::fs::write(report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

pub fn validate_report(report: &ConsistencyCheckReport) -> crate::EvidenceResult<()> {
    require(
        report.schema_version == CHECKER_REPORT_SCHEMA_VERSION,
        format!("checker report schema_version must be {CHECKER_REPORT_SCHEMA_VERSION}"),
    )?;
    require(
        !report.checker.is_empty(),
        "checker report checker must be non-empty",
    )?;
    require(
        report.model == REGISTER_MODEL,
        format!("unsupported checker report model {:?}", report.model),
    )?;
    require(
        !report.history_sha256.is_empty(),
        "checker report history_sha256 must be non-empty",
    )?;
    require(
        report.scope.contains(REQUIRED_SCOPE_FRAGMENT),
        "checker report scope must state that checker evidence is not snapshot replay proof",
    )?;
    let limitations = report.limitations.join("\n").to_ascii_lowercase();
    require(
        limitations.contains("not snapshot replay proof"),
        "checker report limitations must separate checker evidence from replay proof",
    )?;
    if report.verdict == CheckerVerdict::Failed {
        let counterexample = report.counterexample.as_ref().ok_or_else(|| {
            crate::EvidenceError::new("failed checker report lacks counterexample")
        })?;
        require(
            !counterexample.operation_ids.is_empty(),
            "failed checker counterexample must cite operation ids",
        )?;
    }
    Ok(())
}

pub fn validate_report_for_history(
    report: &ConsistencyCheckReport,
    history: &OperationHistory,
) -> crate::EvidenceResult<()> {
    validate_report(report)?;
    require(
        report.history_id == history.history_id,
        format!(
            "checker report history_id {:?} does not match history {:?}",
            report.history_id, history.history_id
        ),
    )?;
    let expected_digest = history_digest(history)?;
    require(
        report.history_sha256 == expected_digest,
        format!(
            "stale checker report digest {:?} does not match current history {expected_digest:?}",
            report.history_sha256
        ),
    )?;
    Ok(())
}

pub fn validate_history(history: &OperationHistory) -> crate::EvidenceResult<()> {
    require(
        history.schema_version == HISTORY_SCHEMA_VERSION,
        format!("history schema_version must be {HISTORY_SCHEMA_VERSION}"),
    )?;
    require(
        !history.history_id.is_empty(),
        "history_id must be non-empty",
    )?;
    require(
        !history.workload.is_empty(),
        "history workload must be non-empty",
    )?;
    require(
        history.model == REGISTER_MODEL,
        format!("unsupported history model {:?}", history.model),
    )?;
    require(
        !history.source_artifact.is_empty(),
        "history source_artifact must be non-empty",
    )?;
    require(
        !history.operations.is_empty(),
        "history must contain at least one operation",
    )?;
    let mut operation_ids = std::collections::BTreeSet::new();
    let mut process_by_id = std::collections::BTreeMap::new();
    for op in &history.operations {
        require(
            operation_ids.insert(op.operation_id.clone()),
            format!("duplicate operation_id {:?}", op.operation_id),
        )?;
        require(
            !op.process.is_empty(),
            "operation process must be non-empty",
        )?;
        if let Some(previous) = process_by_id.insert(op.operation_id.clone(), op.process.clone()) {
            require(
                previous == op.process,
                format!(
                    "ambiguous process identity for operation {:?}",
                    op.operation_id
                ),
            )?;
        }
        validate_operation(op)?;
    }
    Ok(())
}

fn validate_adapter_config(config: &RegisterHistoryAdapterConfig) -> crate::EvidenceResult<()> {
    require(
        !config.history_id.is_empty(),
        "adapter history_id must be non-empty",
    )?;
    require(
        !config.workload.is_empty(),
        "adapter workload must be non-empty",
    )?;
    require(
        !config.source_artifact.is_empty(),
        "adapter source_artifact must be non-empty",
    )?;
    require(
        !config.source_artifact.ends_with(".log"),
        "adapter source_artifact must reference typed history output, not a raw log",
    )?;
    Ok(())
}

fn validate_operation(op: &HistoryOperation) -> crate::EvidenceResult<()> {
    require(
        !op.operation_id.is_empty(),
        "operation_id must be non-empty",
    )?;
    require(
        !op.process.is_empty(),
        "operation process must be non-empty",
    )?;
    require(
        op.invoked_at <= op.completed_at,
        format!("{}: completion precedes invocation", op.operation_id),
    )?;
    match (&op.invocation, &op.completion) {
        (OperationInvocation::Read, OperationCompletion::Ok { value: Some(_) }) => {}
        (OperationInvocation::Read, OperationCompletion::Ok { value: None }) => {}
        (OperationInvocation::Write { .. }, OperationCompletion::Ok { value: None }) => {}
        (OperationInvocation::Write { .. }, OperationCompletion::Ok { value: Some(_) }) => {
            return Err(crate::EvidenceError::new(format!(
                "{}: write completion must not return a value",
                op.operation_id
            )))
        }
        (_, OperationCompletion::Failed { error }) => {
            require(
                !error.is_empty(),
                "failed operation error must be non-empty",
            )?;
        }
    }
    Ok(())
}

pub fn history_digest(history: &OperationHistory) -> crate::EvidenceResult<String> {
    let bytes = serde_json::to_vec(history)?;
    let mut hasher = ::sha2::Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub fn read_history_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<OperationHistory> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|err| crate::EvidenceError::new(format!("{}: {err}", path.display())))?;
    serde_json::from_str(&text).map_err(|err| {
        crate::EvidenceError::new(format!("{}: invalid history JSON: {err}", path.display()))
    })
}

pub fn read_report_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<ConsistencyCheckReport> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|err| crate::EvidenceError::new(format!("{}: {err}", path.display())))?;
    serde_json::from_str(&text).map_err(|err| {
        crate::EvidenceError::new(format!(
            "{}: invalid checker report JSON: {err}",
            path.display()
        ))
    })
}

pub fn sample_good_history() -> OperationHistory {
    OperationHistory {
        schema_version: HISTORY_SCHEMA_VERSION,
        history_id: "sample-register-good".to_string(),
        workload: "register-fixture".to_string(),
        model: REGISTER_MODEL.to_string(),
        source_artifact: "dogfood-results/consistency-checker-fixtures/register-good.json"
            .to_string(),
        limitations: vec!["fixture history for bounded checker validation".to_string()],
        operations: vec![
            HistoryOperation {
                operation_id: "op-write-1".to_string(),
                process: "client-a".to_string(),
                invoked_at: 1,
                completed_at: 2,
                invocation: OperationInvocation::Write { value: 7 },
                completion: OperationCompletion::Ok { value: None },
            },
            HistoryOperation {
                operation_id: "op-read-1".to_string(),
                process: "client-b".to_string(),
                invoked_at: 3,
                completed_at: 4,
                invocation: OperationInvocation::Read,
                completion: OperationCompletion::Ok { value: Some(7) },
            },
        ],
    }
}

pub fn sample_bad_history() -> OperationHistory {
    let mut history = sample_good_history();
    history.history_id = "sample-register-bad".to_string();
    history.source_artifact =
        "dogfood-results/consistency-checker-fixtures/register-bad.json".to_string();
    history.operations[1].completion = OperationCompletion::Ok { value: Some(8) };
    history
}

fn require(condition: bool, message: impl Into<String>) -> crate::EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(crate::EvidenceError::new(message.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_history_passes_and_bad_history_fails() {
        let good = SingleRegisterChecker
            .check(&sample_good_history())
            .expect("good history checks");
        assert_eq!(good.verdict, CheckerVerdict::Passed);
        validate_report(&good).expect("good report validates");

        let bad = SingleRegisterChecker
            .check(&sample_bad_history())
            .expect("bad history checks");
        assert_eq!(bad.verdict, CheckerVerdict::Failed);
        assert_eq!(
            bad.counterexample.expect("counterexample").operation_ids,
            vec!["op-read-1".to_string()]
        );
    }

    #[test]
    fn malformed_histories_fail_closed() {
        let mut duplicate = sample_good_history();
        duplicate.operations[1].operation_id = duplicate.operations[0].operation_id.clone();
        assert!(validate_history(&duplicate)
            .expect_err("duplicate rejected")
            .message()
            .contains("duplicate operation_id"));

        let mut missing_process = sample_good_history();
        missing_process.operations[0].process.clear();
        assert!(validate_history(&missing_process)
            .expect_err("missing process rejected")
            .message()
            .contains("process"));

        let mut missing_source = sample_good_history();
        missing_source.source_artifact.clear();
        assert!(validate_history(&missing_source)
            .expect_err("missing source rejected")
            .message()
            .contains("source_artifact"));

        let missing_completion = serde_json::json!({
            "schema_version": HISTORY_SCHEMA_VERSION,
            "history_id": "missing-completion",
            "workload": "register-fixture",
            "model": REGISTER_MODEL,
            "source_artifact": "fixture.json",
            "operations": [{
                "operation_id": "op-1",
                "process": "client-a",
                "invoked_at": 1,
                "completed_at": 2,
                "invocation": {"kind": "read"}
            }],
            "limitations": []
        });
        assert!(serde_json::from_value::<OperationHistory>(missing_completion).is_err());

        let mut unsupported = sample_good_history();
        unsupported.model = "unsupported-register-model".to_string();
        assert!(validate_history(&unsupported)
            .expect_err("unsupported model rejected")
            .message()
            .contains("unsupported history model"));
    }

    #[test]
    fn typed_workload_adapter_emits_valid_history_without_raw_log_scraping() {
        let mut adapter = RegisterWorkloadHistoryAdapter::new(RegisterHistoryAdapterConfig {
            history_id: "adapter-register-good".to_string(),
            workload: "register-adapter-fixture".to_string(),
            source_artifact:
                "dogfood-results/consistency-checker-fixtures/adapter-register-good.json"
                    .to_string(),
            limitations: vec!["fixture emitted by typed workload adapter".to_string()],
        })
        .expect("adapter config is valid");
        adapter
            .record_write_ok("adapter-write-1", "client-a", 1, 2, 7)
            .expect("write recorded");
        adapter
            .record_read_ok("adapter-read-1", "client-b", 3, 4, Some(7))
            .expect("read recorded");

        let history = adapter.emit_history().expect("history emitted");
        validate_history(&history).expect("adapter history validates");
        assert_eq!(history.model, REGISTER_MODEL);
        assert!(history
            .limitations
            .iter()
            .any(|item| item.contains("typed workload adapter")));
        assert_eq!(
            SingleRegisterChecker
                .check(&history)
                .expect("adapter history checks")
                .verdict,
            CheckerVerdict::Passed
        );
    }

    #[test]
    fn adapter_rejects_raw_log_source_and_bad_operations() {
        let raw_log_config = RegisterHistoryAdapterConfig {
            history_id: "raw-log".to_string(),
            workload: "register-adapter-fixture".to_string(),
            source_artifact: "run.log".to_string(),
            limitations: vec![],
        };
        assert!(RegisterWorkloadHistoryAdapter::new(raw_log_config)
            .expect_err("raw log source rejected")
            .message()
            .contains("not a raw log"));

        let mut adapter = RegisterWorkloadHistoryAdapter::new(RegisterHistoryAdapterConfig {
            history_id: "bad-op".to_string(),
            workload: "register-adapter-fixture".to_string(),
            source_artifact: "typed-history.json".to_string(),
            limitations: vec![],
        })
        .expect("adapter config is valid");
        assert!(adapter
            .record_write_ok("write-backwards", "client-a", 5, 4, 7)
            .expect_err("bad operation rejected")
            .message()
            .contains("completion precedes invocation"));
    }

    #[test]
    fn stale_reports_fail_closed() {
        let history = sample_good_history();
        let mut report = SingleRegisterChecker
            .check(&history)
            .expect("report checks");
        report.history_sha256 = "sha256:stale".to_string();
        assert!(validate_report_for_history(&report, &history)
            .expect_err("stale report rejected")
            .message()
            .contains("stale checker report digest"));
    }

    #[test]
    fn overclaimed_reports_fail_closed() {
        let mut report = SingleRegisterChecker
            .check(&sample_good_history())
            .expect("report checks");
        report.scope = "proves deterministic replay and product parity".to_string();
        assert!(validate_report(&report)
            .expect_err("overclaim rejected")
            .message()
            .contains("not snapshot replay proof"));
    }
}
