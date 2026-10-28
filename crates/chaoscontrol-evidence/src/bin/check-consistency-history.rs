use std::path::PathBuf;

use chaoscontrol_evidence::{
    check_consistency_history_path, validate_consistency_history_path,
    write_adapter_sample_consistency_history_path, write_consistency_check_report_path,
    write_sample_consistency_history_path, CheckerVerdict, EvidenceError, EvidenceResult,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {}", err.message());
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    run_with(std::env::args().skip(1).collect())
}

fn run_with(args: Vec<String>) -> EvidenceResult<()> {
    if args.iter().any(String::is_empty) {
        return Err(EvidenceError::new("path arguments must not be empty"));
    }
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
        [cmd, path] if cmd == "adapter-sample" => {
            let path = PathBuf::from(path);
            write_adapter_sample_consistency_history_path(&path)?;
            println!("wrote {}", path.display());
        }
        _ => {
            return Err(EvidenceError::new(
                "usage: check-consistency-history validate <history.json> | check <history.json> [report.json] | sample-good <path> | sample-bad <path> | adapter-sample <path>",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_paths_are_rejected() {
        assert!(run_with(vec!["sample-good".to_string(), String::new()]).is_err());
    }
}
