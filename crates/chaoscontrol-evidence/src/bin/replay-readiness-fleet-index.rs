use std::io::Write;
use std::path::PathBuf;

use chaoscontrol_evidence::{
    sample_replay_readiness_receipt, write_fleet_triage_index_path, EvidenceResult,
};

fn usage() -> &'static str {
    "usage: replay-readiness-fleet-index --output PATH RECEIPT...\n       replay-readiness-fleet-index --sample --output PATH"
}

fn main() {
    if let Err(err) = run() {
        eprintln!("replay readiness fleet index failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    let mut output: Option<PathBuf> = None;
    let mut receipts = Vec::new();
    let mut sample = false;
    let mut sample_receipt = None;
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
                output = Some(PathBuf::from(value));
            }
            "--sample" => sample = true,
            _ => receipts.push(PathBuf::from(arg)),
        }
    }
    let output = output.ok_or_else(|| {
        chaoscontrol_evidence::EvidenceError::new(format!("missing --output\n{}", usage()))
    })?;
    if sample {
        let mut temporary = tempfile::NamedTempFile::new()?;
        serde_json::to_writer_pretty(
            &mut temporary,
            &sample_replay_readiness_receipt(true, "passed"),
        )?;
        temporary.flush()?;
        receipts.push(temporary.path().to_path_buf());
        sample_receipt = Some(temporary);
    }
    if receipts.is_empty() {
        return Err(chaoscontrol_evidence::EvidenceError::new(format!(
            "missing receipt path\n{}",
            usage()
        )));
    }
    write_fleet_triage_index_path(&receipts, &output)?;
    drop(sample_receipt);
    println!(
        "wrote {} from {} receipt(s)",
        output.display(),
        receipts.len()
    );
    Ok(())
}
