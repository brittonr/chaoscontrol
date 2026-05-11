use std::path::PathBuf;

use chaoscontrol_evidence::{
    execute_replay_readiness_fleet_scheduler_receipt_path,
    execute_replay_readiness_scheduler_receipt_path, sample_replay_readiness_fleet_scheduler_plan,
    sample_replay_readiness_fleet_scheduler_receipt, sample_replay_readiness_scheduler_receipt,
    validate_replay_readiness_fleet_scheduler_receipt_path,
    validate_replay_readiness_scheduler_execution_receipt_path,
    validate_replay_readiness_scheduler_receipt_path,
    write_replay_readiness_fleet_scheduler_receipt_path,
    write_replay_readiness_scheduler_receipt_path, EvidenceResult,
};

fn usage() -> &'static str {
    "usage: replay-readiness-scheduler-receipt --sample --output PATH\n       replay-readiness-scheduler-receipt --check PATH\n       replay-readiness-scheduler-receipt --run-plan PLAN --output PATH\n       replay-readiness-scheduler-receipt --check-execution PATH\n       replay-readiness-scheduler-receipt --sample-fleet --output PATH\n       replay-readiness-scheduler-receipt --sample-fleet-plan --output PATH\n       replay-readiness-scheduler-receipt --run-fleet-plan PLAN --output PATH\n       replay-readiness-scheduler-receipt --check-fleet PATH"
}

fn main() {
    if let Err(err) = run() {
        eprintln!("replay readiness scheduler receipt failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    let mut output: Option<PathBuf> = None;
    let mut check: Option<PathBuf> = None;
    let mut run_plan: Option<PathBuf> = None;
    let mut run_fleet_plan: Option<PathBuf> = None;
    let mut check_execution: Option<PathBuf> = None;
    let mut check_fleet: Option<PathBuf> = None;
    let mut sample = false;
    let mut sample_fleet = false;
    let mut sample_fleet_plan = false;
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
            "--check" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--check requires a path")
                })?;
                check = Some(PathBuf::from(value));
            }
            "--run-plan" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--run-plan requires a path")
                })?;
                run_plan = Some(PathBuf::from(value));
            }
            "--run-fleet-plan" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--run-fleet-plan requires a path")
                })?;
                run_fleet_plan = Some(PathBuf::from(value));
            }
            "--check-execution" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--check-execution requires a path")
                })?;
                check_execution = Some(PathBuf::from(value));
            }
            "--check-fleet" => {
                let value = args.next().ok_or_else(|| {
                    chaoscontrol_evidence::EvidenceError::new("--check-fleet requires a path")
                })?;
                check_fleet = Some(PathBuf::from(value));
            }
            "--sample" => sample = true,
            "--sample-fleet" => sample_fleet = true,
            "--sample-fleet-plan" => sample_fleet_plan = true,
            _ => {
                return Err(chaoscontrol_evidence::EvidenceError::new(format!(
                    "unexpected argument: {}\n{}",
                    arg.to_string_lossy(),
                    usage()
                )));
            }
        }
    }

    match (
        sample,
        sample_fleet,
        sample_fleet_plan,
        output,
        check,
        run_plan,
        run_fleet_plan,
        check_execution,
        check_fleet,
    ) {
        (true, false, false, Some(output), None, None, None, None, None) => {
            write_replay_readiness_scheduler_receipt_path(&output)?;
            println!(
                "wrote {} ({})",
                output.display(),
                chaoscontrol_evidence::validate_replay_readiness_scheduler_receipt(
                    &sample_replay_readiness_scheduler_receipt()
                )?
            );
        }
        (false, false, false, None, Some(path), None, None, None, None) => {
            println!(
                "{}",
                validate_replay_readiness_scheduler_receipt_path(path)?
            );
        }
        (false, false, false, Some(output), None, Some(plan), None, None, None) => {
            let summary = execute_replay_readiness_scheduler_receipt_path(plan, &output)?;
            println!("wrote {} ({summary})", output.display());
        }
        (false, false, false, None, None, None, None, Some(path), None) => {
            println!(
                "{}",
                validate_replay_readiness_scheduler_execution_receipt_path(path)?
            );
        }
        (false, true, false, Some(output), None, None, None, None, None) => {
            write_replay_readiness_fleet_scheduler_receipt_path(&output)?;
            println!(
                "wrote {} ({})",
                output.display(),
                chaoscontrol_evidence::validate_replay_readiness_fleet_scheduler_receipt(
                    &sample_replay_readiness_fleet_scheduler_receipt()
                )?
            );
        }
        (false, false, true, Some(output), None, None, None, None, None) => {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(
                &output,
                serde_json::to_vec_pretty(&sample_replay_readiness_fleet_scheduler_plan())?,
            )?;
            println!(
                "wrote {} (replay-readiness-fleet-scheduler-plan)",
                output.display()
            );
        }
        (false, false, false, Some(output), None, None, Some(plan), None, None) => {
            let summary = execute_replay_readiness_fleet_scheduler_receipt_path(plan, &output)?;
            println!("wrote {} ({summary})", output.display());
        }
        (false, false, false, None, None, None, None, None, Some(path)) => {
            println!(
                "{}",
                validate_replay_readiness_fleet_scheduler_receipt_path(path)?
            );
        }
        _ => {
            return Err(chaoscontrol_evidence::EvidenceError::new(format!(
                "choose exactly one mode: --sample --output PATH, --check PATH, --run-plan PLAN --output PATH, --check-execution PATH, --sample-fleet --output PATH, --sample-fleet-plan --output PATH, --run-fleet-plan PLAN --output PATH, or --check-fleet PATH\n{}",
                usage()
            )));
        }
    }
    Ok(())
}
