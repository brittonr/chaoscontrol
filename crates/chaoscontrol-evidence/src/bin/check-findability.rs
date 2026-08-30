use chaoscontrol_evidence::{
    check_findability_artifact_path, read_findability_artifact_path, write_findability_report_path,
    EvidenceError, EvidenceResult,
};
use chaoscontrol_sim_core::findability::{FindabilityReport, FindabilityStatus};

const INDEPENDENCE_VIOLATION_EXIT_STATUS: i32 = 2;
const INSUFFICIENT_SAMPLES_EXIT_STATUS: i32 = 3;

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
            match report.status {
                FindabilityStatus::IndependenceViolation => {
                    std::process::exit(INDEPENDENCE_VIOLATION_EXIT_STATUS);
                }
                FindabilityStatus::InsufficientSamples => {
                    std::process::exit(INSUFFICIENT_SAMPLES_EXIT_STATUS);
                }
                FindabilityStatus::Fitted | FindabilityStatus::NoBugObserved => {}
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("ERROR: {}", error.message());
            std::process::exit(1);
        }
    }
}

fn run() -> EvidenceResult<Option<FindabilityReport>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command, path] if command == "validate" => {
            let artifact = read_findability_artifact_path(path)?;
            println!(
                "artifact={} generation={} subtrees={}",
                artifact.artifact_blake3,
                artifact.generation_id,
                artifact.subtrees.len()
            );
            Ok(None)
        }
        [command, path] if command == "check" => Ok(Some(check_findability_artifact_path(path)?)),
        [command, artifact_path, report_path] if command == "check" => {
            write_findability_report_path(artifact_path, report_path)?;
            Ok(Some(check_findability_artifact_path(artifact_path)?))
        }
        _ => Err(EvidenceError::new(
            "usage: check-findability validate <rounds.json> | check <rounds.json> [report.json]",
        )),
    }
}
