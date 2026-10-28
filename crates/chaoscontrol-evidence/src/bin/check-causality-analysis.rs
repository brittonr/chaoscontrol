use chaoscontrol_evidence::{
    read_causality_receipt_path, read_causality_request_path, EvidenceError, EvidenceResult,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR: {}", error.message());
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
        [command, request_path] if command == "validate" => {
            let request = read_causality_request_path(request_path)?;
            println!(
                "request={} steps={} candidates={} snapshots={}",
                request.request_blake3,
                request.steps.len(),
                request.candidates.len(),
                request.snapshot_blake3s.len()
            );
            Ok(())
        }
        [command, request_path, receipt_path] if command == "validate" => {
            let request = read_causality_request_path(request_path)?;
            let receipt = read_causality_receipt_path(&request, receipt_path)?;
            println!(
                "receipt={} minimized_steps={} probable_causes={} partial={}",
                receipt.receipt_blake3,
                receipt.minimization.minimized_steps.len(),
                receipt.attribution.probable_causes.len(),
                receipt.minimization.budget_exhausted || receipt.attribution.partial
            );
            Ok(())
        }
        _ => Err(EvidenceError::new(
            "usage: check-causality-analysis validate <request.json> [receipt.json]",
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
