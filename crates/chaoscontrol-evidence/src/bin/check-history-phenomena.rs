use chaoscontrol_evidence::{
    check_consistency_phenomena_path, check_phenomena_history_path,
    validate_phenomena_history_path, write_phenomena_report_path, EvidenceError, EvidenceResult,
};
use chaoscontrol_smr::phenomena::{CheckOutcome, PhenomenaReport};

const VIOLATION_EXIT_STATUS: i32 = 2;
const INSUFFICIENT_DATA_EXIT_STATUS: i32 = 3;

fn main() {
    match run() {
        Ok(Some(report)) => {
            let json = match serde_json::to_string_pretty(&report) {
                Ok(json) => json,
                Err(error) => {
                    eprintln!("ERROR: validated report did not serialize: {error}");
                    std::process::exit(1);
                }
            };
            println!("{json}");
            if report.outcome == CheckOutcome::InsufficientData {
                std::process::exit(INSUFFICIENT_DATA_EXIT_STATUS);
            }
            if !report.violations.is_empty() {
                std::process::exit(VIOLATION_EXIT_STATUS);
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("ERROR: {}", error.message());
            std::process::exit(1);
        }
    }
}

fn run() -> EvidenceResult<Option<PhenomenaReport>> {
    run_with(std::env::args().skip(1).collect())
}

fn run_with(args: Vec<String>) -> EvidenceResult<Option<PhenomenaReport>> {
    if args.iter().any(String::is_empty) {
        return Err(EvidenceError::new("path arguments must not be empty"));
    }
    match args.as_slice() {
        [command, path] if command == "validate" => {
            println!("{}", validate_phenomena_history_path(path)?);
            Ok(None)
        }
        [command, path] if command == "check" => {
            Ok(Some(check_phenomena_history_path(path)?))
        }
        [command, path] if command == "check-round" => {
            Ok(Some(check_consistency_phenomena_path(path)?))
        }
        [command, history_path, report_path] if command == "check" => {
            write_phenomena_report_path(history_path, report_path)?;
            Ok(Some(check_phenomena_history_path(history_path)?))
        }
        _ => Err(EvidenceError::new(
            "usage: check-history-phenomena validate <history.json> | check <history.json> [report.json] | check-round <operation-history.json>",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_paths_are_rejected() {
        assert!(run_with(vec!["validate".to_string(), String::new()]).is_err());
    }
}
