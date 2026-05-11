use std::path::PathBuf;

use chaoscontrol_evidence::{
    check_consistency_history_path, validate_consistency_history_path,
    write_consistency_check_report_path, write_sample_consistency_history_path, CheckerVerdict,
    EvidenceError, EvidenceResult,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {}", err.message());
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [cmd, path] if cmd == "validate" => {
            println!("{}", validate_consistency_history_path(path)?);
        }
        [cmd, history_path] if cmd == "check" => {
            let report = check_consistency_history_path(history_path)?;
            chaoscontrol_evidence::validate_consistency_report(&report)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.verdict == CheckerVerdict::Failed {
                std::process::exit(2);
            }
        }
        [cmd, history_path, report_path] if cmd == "check" => {
            write_consistency_check_report_path(history_path, report_path)?;
            let report = check_consistency_history_path(history_path)?;
            println!(
                "wrote {} verdict={:?} operations={}",
                report_path, report.verdict, report.checked_operations
            );
            if report.verdict == CheckerVerdict::Failed {
                std::process::exit(2);
            }
        }
        [cmd, path] if cmd == "sample-good" => {
            write_consistency_sample(path, false)?;
        }
        [cmd, path] if cmd == "sample-bad" => {
            write_consistency_sample(path, true)?;
        }
        _ => {
            return Err(EvidenceError::new(
                "usage: check-consistency-history validate <history.json> | check <history.json> [report.json] | sample-good <path> | sample-bad <path>",
            ));
        }
    }
    Ok(())
}

fn write_consistency_sample(path: &str, bad: bool) -> EvidenceResult<()> {
    let path = PathBuf::from(path);
    write_sample_consistency_history_path(&path, bad)?;
    println!("wrote {}", path.display());
    Ok(())
}
