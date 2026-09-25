//! Replays a legacy run's and a Campaign run's `frontier-trace.jsonl` through
//! the other frontier policy and writes a parity receipt.
//!
//! Exit status: 0 when every decision field of both runs agrees (cutover
//! ready), 1 when a decision field drifted (the receipt is still written),
//! 2 for usage, input, or receipt-writing errors.

use chaoscontrol_explore::explorer::parity::{check_trace_pair, write_receipt};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: check-frontier-parity --legacy <frontier-trace.jsonl> --campaign <frontier-trace.jsonl> --receipt <receipt.json>";
/// Both runs agree in every decision field.
const EXIT_READY: u8 = 0;
/// At least one decision field drifted.
const EXIT_DRIFT: u8 = 1;
/// Usage, input, or receipt error.
const EXIT_ERROR: u8 = 2;

struct Arguments {
    legacy: PathBuf,
    campaign: PathBuf,
    receipt: PathBuf,
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let (mut legacy, mut campaign, mut receipt) = (None, None, None);
    while let Some(flag) = arguments.next() {
        let slot = match flag.as_str() {
            "--legacy" => &mut legacy,
            "--campaign" => &mut campaign,
            "--receipt" => &mut receipt,
            other => return Err(format!("unknown argument {other}")),
        };
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} requires a path"))?;
        if slot.replace(PathBuf::from(value)).is_some() {
            return Err(format!("{flag} given twice"));
        }
    }
    Ok(Arguments {
        legacy: legacy.ok_or("--legacy is required")?,
        campaign: campaign.ok_or("--campaign is required")?,
        receipt: receipt.ok_or("--receipt is required")?,
    })
}

fn main() -> ExitCode {
    let arguments = match parse(std::env::args().skip(1)) {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(EXIT_ERROR);
        }
    };
    let receipt = match check_trace_pair(&arguments.legacy, &arguments.campaign) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!("frontier parity check failed: {error}");
            return ExitCode::from(EXIT_ERROR);
        }
    };
    if let Err(error) = write_receipt(&arguments.receipt, &receipt) {
        eprintln!("frontier parity receipt: {error}");
        return ExitCode::from(EXIT_ERROR);
    }
    for run in &receipt.runs {
        let report = &run.report;
        println!(
            "{}: {} rounds, decision fields {}, semantic work {:?}",
            report.case,
            report.rounds_compared,
            if report.decision_fields.equal {
                "equal".to_owned()
            } else {
                format!("drifted ({} values)", report.decision_fields.drift_count)
            },
            report.semantic_work.answer
        );
    }
    match receipt.cutover_readiness() {
        Ok(()) => {
            println!(
                "frontier parity ok: receipt {}",
                arguments.receipt.display()
            );
            ExitCode::from(EXIT_READY)
        }
        Err(drift) => {
            eprintln!("{drift}");
            ExitCode::from(EXIT_DRIFT)
        }
    }
}
