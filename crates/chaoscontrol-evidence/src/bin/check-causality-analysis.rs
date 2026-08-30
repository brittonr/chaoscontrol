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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
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
