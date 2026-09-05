use chaoscontrol_evidence::{
    sample_replay_readiness_decision_receipt, validate_replay_readiness_decision_receipt_path,
    write_replay_readiness_decision_receipt_path, EvidenceResult,
};

fn usage() -> &'static str {
    "usage: replay-readiness-decision-receipt --sample --output PATH\n       replay-readiness-decision-receipt --check PATH"
}

fn main() {
    if let Err(err) = run() {
        eprintln!("replay readiness decision receipt failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    let mut output: Option<std::path::PathBuf> = None;
    let mut check: Option<std::path::PathBuf> = None;
    let mut sample = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--output" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--output requires a path")
                })?;
                output = Some(std::path::PathBuf::from(value));
            }
            "--check" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--check requires a path")
                })?;
                check = Some(std::path::PathBuf::from(value));
            }
            "--sample" => sample = true,
            _ => {
                return Err(chaoscontrol_evidence::EvidenceError::new(format!(
                    "unexpected argument: {}\n{}",
                    arg.to_string_lossy(),
                    usage()
                )));
            }
        }
    }

    match (sample, output, check) {
        (true, Some(output), None) => {
            write_replay_readiness_decision_receipt_path(&output)?;
            println!(
                "wrote {} ({})",
                output.display(),
                chaoscontrol_evidence::validate_replay_readiness_decision_receipt(
                    &sample_replay_readiness_decision_receipt()
                )?
            );
        }
        (false, None, Some(path)) => {
            println!("{}", validate_replay_readiness_decision_receipt_path(path)?);
        }
        _ => {
            return Err(chaoscontrol_evidence::EvidenceError::new(format!(
                "choose exactly one mode: --sample --output PATH or --check PATH\n{}",
                usage()
            )));
        }
    }
    Ok(())
}
