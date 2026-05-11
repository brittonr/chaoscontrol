use std::path::Path;

use chaoscontrol_evidence::{
    check_consistency_history_path, read_consistency_history_path, read_consistency_report_path,
    validate_consistency_report, validate_consistency_report_for_history, CheckerVerdict,
    EvidenceError, EvidenceResult,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {}", err.message());
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let root = Path::new(&root);
    let good_path = root.join("dogfood-results/consistency-checker-fixtures/register-good.json");
    let good_report_path =
        root.join("dogfood-results/consistency-checker-fixtures/register-good.report.json");
    let good_history = read_consistency_history_path(&good_path)?;
    let good_report = read_consistency_report_path(&good_report_path)?;
    validate_consistency_report_for_history(&good_report, &good_history)?;
    let good = check_consistency_history_path(&good_path)?;
    validate_consistency_report(&good)?;
    require(
        good.verdict == CheckerVerdict::Passed,
        "known-good consistency history did not pass",
    )?;

    let bad_path = root.join("dogfood-results/consistency-checker-fixtures/register-bad.json");
    let bad_report_path =
        root.join("dogfood-results/consistency-checker-fixtures/register-bad.report.json");
    let bad_history = read_consistency_history_path(&bad_path)?;
    let bad_report = read_consistency_report_path(&bad_report_path)?;
    validate_consistency_report_for_history(&bad_report, &bad_history)?;
    let bad = check_consistency_history_path(&bad_path)?;
    validate_consistency_report(&bad)?;
    require(
        bad.verdict == CheckerVerdict::Failed,
        "known-bad consistency history did not fail",
    )?;
    require(
        bad.counterexample.as_ref().is_some_and(|counterexample| {
            counterexample
                .operation_ids
                .iter()
                .any(|id| id == "op-read-1")
        }),
        "known-bad consistency history did not cite op-read-1 counterexample",
    )?;
    println!(
        "consistency checker fixtures ok: good={} bad={}",
        good.history_sha256, bad.history_sha256
    );
    Ok(())
}

fn require(condition: bool, message: impl Into<String>) -> EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(EvidenceError::new(message.into()))
    }
}
