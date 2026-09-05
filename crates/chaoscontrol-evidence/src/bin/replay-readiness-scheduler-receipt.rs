use chaoscontrol_evidence::{
    execute_replay_readiness_fleet_scheduler_receipt_path,
    execute_replay_readiness_hosted_shared_state_receipt_path,
    execute_replay_readiness_multi_hypervisor_campaign_receipt_path,
    execute_replay_readiness_networked_hosted_scheduler_receipt_path,
    execute_replay_readiness_scheduler_receipt_path, sample_replay_readiness_fleet_scheduler_plan,
    sample_replay_readiness_fleet_scheduler_receipt,
    sample_replay_readiness_hosted_shared_state_plan,
    sample_replay_readiness_hosted_shared_state_receipt,
    sample_replay_readiness_multi_hypervisor_campaign_plan,
    sample_replay_readiness_multi_hypervisor_campaign_receipt,
    sample_replay_readiness_networked_hosted_scheduler_plan,
    sample_replay_readiness_networked_hosted_scheduler_receipt,
    sample_replay_readiness_scheduler_receipt,
    validate_replay_readiness_fleet_scheduler_receipt_path,
    validate_replay_readiness_hosted_shared_state_receipt_path,
    validate_replay_readiness_multi_hypervisor_campaign_receipt_path,
    validate_replay_readiness_networked_hosted_scheduler_receipt_path,
    validate_replay_readiness_scheduler_execution_receipt_path,
    validate_replay_readiness_scheduler_receipt_path,
    write_replay_readiness_fleet_scheduler_receipt_path,
    write_replay_readiness_hosted_shared_state_receipt_path,
    write_replay_readiness_multi_hypervisor_campaign_dashboard_path,
    write_replay_readiness_multi_hypervisor_campaign_receipt_path,
    write_replay_readiness_networked_hosted_scheduler_receipt_path,
    write_replay_readiness_scheduler_receipt_path, EvidenceResult,
};

fn usage() -> &'static str {
    "usage: replay-readiness-scheduler-receipt --sample --output PATH\n       replay-readiness-scheduler-receipt --check PATH\n       replay-readiness-scheduler-receipt --run-plan PLAN --output PATH\n       replay-readiness-scheduler-receipt --materialize-ci-plans DIR --replay-readiness PATH\n       replay-readiness-scheduler-receipt --materialize-command-plan PATH --executable PATH [--argument VALUE ...]\n       replay-readiness-scheduler-receipt --check-execution PATH\n       replay-readiness-scheduler-receipt --sample-fleet --output PATH\n       replay-readiness-scheduler-receipt --sample-fleet-plan --output PATH\n       replay-readiness-scheduler-receipt --run-fleet-plan PLAN --output PATH\n       replay-readiness-scheduler-receipt --check-fleet PATH\n       replay-readiness-scheduler-receipt --sample-multi-hypervisor --output PATH\n       replay-readiness-scheduler-receipt --sample-multi-hypervisor-plan --output PATH\n       replay-readiness-scheduler-receipt --run-multi-hypervisor-plan PLAN --output PATH\n       replay-readiness-scheduler-receipt --check-multi-hypervisor PATH\n       replay-readiness-scheduler-receipt --render-multi-hypervisor-dashboard PATH --output PATH\n       replay-readiness-scheduler-receipt --sample-hosted-shared-state --output PATH\n       replay-readiness-scheduler-receipt --sample-hosted-shared-state-plan --output PATH\n       replay-readiness-scheduler-receipt --run-hosted-shared-state-plan PLAN --output PATH\n       replay-readiness-scheduler-receipt --check-hosted-shared-state PATH"
}

#[derive(Default)]
struct Args {
    output: Option<std::path::PathBuf>,
    ci_plan_output: Option<std::path::PathBuf>,
    command_plan_output: Option<std::path::PathBuf>,
    replay_readiness: Option<std::path::PathBuf>,
    executable: Option<std::path::PathBuf>,
    arguments: Vec<String>,
    check: Option<std::path::PathBuf>,
    run_plan: Option<std::path::PathBuf>,
    run_fleet_plan: Option<std::path::PathBuf>,
    check_execution: Option<std::path::PathBuf>,
    check_fleet: Option<std::path::PathBuf>,
    run_multi_hypervisor_plan: Option<std::path::PathBuf>,
    check_multi_hypervisor: Option<std::path::PathBuf>,
    render_multi_hypervisor_dashboard: Option<std::path::PathBuf>,
    run_hosted_shared_state_plan: Option<std::path::PathBuf>,
    check_hosted_shared_state: Option<std::path::PathBuf>,
    run_networked_hosted_plan: Option<std::path::PathBuf>,
    check_networked_hosted: Option<std::path::PathBuf>,
    sample: bool,
    sample_fleet: bool,
    sample_fleet_plan: bool,
    sample_multi_hypervisor: bool,
    sample_multi_hypervisor_plan: bool,
    sample_hosted_shared_state: bool,
    sample_hosted_shared_state_plan: bool,
    sample_networked_hosted: bool,
    sample_networked_hosted_plan: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("replay readiness scheduler receipt failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> EvidenceResult<()> {
    let args = parse_args()?;
    if args.replay_readiness.is_some() && args.ci_plan_output.is_none() {
        return Err(chaoscontrol_evidence::EvidenceError::new(format!(
            "--replay-readiness requires --materialize-ci-plans\n{}",
            usage()
        )));
    }
    if (args.executable.is_some() || !args.arguments.is_empty())
        && args.command_plan_output.is_none()
    {
        return Err(chaoscontrol_evidence::EvidenceError::new(format!(
            "--executable and --argument require --materialize-command-plan\n{}",
            usage()
        )));
    }
    let mode_count = args.sample as usize
        + args.sample_fleet as usize
        + args.sample_fleet_plan as usize
        + args.sample_multi_hypervisor as usize
        + args.sample_multi_hypervisor_plan as usize
        + args.sample_hosted_shared_state as usize
        + args.sample_hosted_shared_state_plan as usize
        + args.sample_networked_hosted as usize
        + args.sample_networked_hosted_plan as usize
        + args.check.is_some() as usize
        + args.run_plan.is_some() as usize
        + args.ci_plan_output.is_some() as usize
        + args.command_plan_output.is_some() as usize
        + args.check_execution.is_some() as usize
        + args.run_fleet_plan.is_some() as usize
        + args.check_fleet.is_some() as usize
        + args.run_multi_hypervisor_plan.is_some() as usize
        + args.check_multi_hypervisor.is_some() as usize
        + args.render_multi_hypervisor_dashboard.is_some() as usize
        + args.run_hosted_shared_state_plan.is_some() as usize
        + args.check_hosted_shared_state.is_some() as usize
        + args.run_networked_hosted_plan.is_some() as usize
        + args.check_networked_hosted.is_some() as usize;
    if mode_count != 1 {
        return Err(chaoscontrol_evidence::EvidenceError::new(format!(
            "choose exactly one mode\n{}",
            usage()
        )));
    }

    if let Some(output_path) = args.command_plan_output {
        let executable = args.executable.ok_or_else(|| {
            chaoscontrol_evidence::EvidenceError::new(
                "--materialize-command-plan requires --executable PATH",
            )
        })?;
        chaoscontrol_evidence::write_typed_command_plan(&output_path, executable, args.arguments)?;
        println!("wrote typed command plan to {}", output_path.display());
    } else if let Some(output_root) = args.ci_plan_output {
        let executable = args.replay_readiness.ok_or_else(|| {
            chaoscontrol_evidence::EvidenceError::new(
                "--materialize-ci-plans requires --replay-readiness PATH",
            )
        })?;
        let count = chaoscontrol_evidence::write_ci_scheduler_plans(&output_root, executable)?;
        println!(
            "wrote {count} typed replay-readiness CI plans to {}",
            output_root.display()
        );
    } else if args.sample {
        let output = require_output(args.output)?;
        write_replay_readiness_scheduler_receipt_path(&output)?;
        println!(
            "wrote {} ({})",
            output.display(),
            chaoscontrol_evidence::validate_replay_readiness_scheduler_receipt(
                &sample_replay_readiness_scheduler_receipt()
            )?
        );
    } else if let Some(path) = args.check {
        println!(
            "{}",
            validate_replay_readiness_scheduler_receipt_path(path)?
        );
    } else if let Some(plan) = args.run_plan {
        let output = require_output(args.output)?;
        let summary = execute_replay_readiness_scheduler_receipt_path(plan, &output)?;
        println!("wrote {} ({summary})", output.display());
    } else if let Some(path) = args.check_execution {
        println!(
            "{}",
            validate_replay_readiness_scheduler_execution_receipt_path(path)?
        );
    } else if args.sample_fleet {
        let output = require_output(args.output)?;
        write_replay_readiness_fleet_scheduler_receipt_path(&output)?;
        println!(
            "wrote {} ({})",
            output.display(),
            chaoscontrol_evidence::validate_replay_readiness_fleet_scheduler_receipt(
                &sample_replay_readiness_fleet_scheduler_receipt()
            )?
        );
    } else if args.sample_fleet_plan {
        let output = require_output(args.output)?;
        write_json(&output, &sample_replay_readiness_fleet_scheduler_plan())?;
        println!(
            "wrote {} (replay-readiness-fleet-scheduler-plan)",
            output.display()
        );
    } else if let Some(plan) = args.run_fleet_plan {
        let output = require_output(args.output)?;
        let summary = execute_replay_readiness_fleet_scheduler_receipt_path(plan, &output)?;
        println!("wrote {} ({summary})", output.display());
    } else if let Some(path) = args.check_fleet {
        println!(
            "{}",
            validate_replay_readiness_fleet_scheduler_receipt_path(path)?
        );
    } else if args.sample_multi_hypervisor {
        let output = require_output(args.output)?;
        write_replay_readiness_multi_hypervisor_campaign_receipt_path(&output)?;
        println!(
            "wrote {} ({})",
            output.display(),
            chaoscontrol_evidence::validate_replay_readiness_multi_hypervisor_campaign_receipt(
                &sample_replay_readiness_multi_hypervisor_campaign_receipt()
            )?
        );
    } else if args.sample_multi_hypervisor_plan {
        let output = require_output(args.output)?;
        write_json(
            &output,
            &sample_replay_readiness_multi_hypervisor_campaign_plan(),
        )?;
        println!(
            "wrote {} (replay-readiness-local-multi-hypervisor-campaign-plan)",
            output.display()
        );
    } else if let Some(plan) = args.run_multi_hypervisor_plan {
        let output = require_output(args.output)?;
        let summary =
            execute_replay_readiness_multi_hypervisor_campaign_receipt_path(plan, &output)?;
        println!("wrote {} ({summary})", output.display());
    } else if let Some(path) = args.check_multi_hypervisor {
        println!(
            "{}",
            validate_replay_readiness_multi_hypervisor_campaign_receipt_path(path)?
        );
    } else if let Some(path) = args.render_multi_hypervisor_dashboard {
        let output = require_output(args.output)?;
        let summary =
            write_replay_readiness_multi_hypervisor_campaign_dashboard_path(path, &output)?;
        println!("wrote {} ({summary})", output.display());
    } else if args.sample_hosted_shared_state {
        let output = require_output(args.output)?;
        write_replay_readiness_hosted_shared_state_receipt_path(&output)?;
        println!(
            "wrote {} ({})",
            output.display(),
            chaoscontrol_evidence::validate_replay_readiness_hosted_shared_state_receipt(
                &sample_replay_readiness_hosted_shared_state_receipt()
            )?
        );
    } else if args.sample_hosted_shared_state_plan {
        let output = require_output(args.output)?;
        write_json(&output, &sample_replay_readiness_hosted_shared_state_plan())?;
        println!(
            "wrote {} (replay-readiness-hosted-shared-state-plan)",
            output.display()
        );
    } else if let Some(plan) = args.run_hosted_shared_state_plan {
        let output = require_output(args.output)?;
        let summary = execute_replay_readiness_hosted_shared_state_receipt_path(plan, &output)?;
        println!("wrote {} ({summary})", output.display());
    } else if let Some(path) = args.check_hosted_shared_state {
        println!(
            "{}",
            validate_replay_readiness_hosted_shared_state_receipt_path(path)?
        );
    } else if args.sample_networked_hosted {
        let output = require_output(args.output)?;
        write_replay_readiness_networked_hosted_scheduler_receipt_path(&output)?;
        println!(
            "wrote {} ({})",
            output.display(),
            chaoscontrol_evidence::validate_replay_readiness_networked_hosted_scheduler_receipt(
                &sample_replay_readiness_networked_hosted_scheduler_receipt()
            )?
        );
    } else if args.sample_networked_hosted_plan {
        let output = require_output(args.output)?;
        write_json(
            &output,
            &sample_replay_readiness_networked_hosted_scheduler_plan(),
        )?;
        println!(
            "wrote {} (replay-readiness-networked-hosted-scheduler-plan)",
            output.display()
        );
    } else if let Some(plan) = args.run_networked_hosted_plan {
        let output = require_output(args.output)?;
        let summary =
            execute_replay_readiness_networked_hosted_scheduler_receipt_path(plan, &output)?;
        println!("wrote {} ({summary})", output.display());
    } else if let Some(path) = args.check_networked_hosted {
        println!(
            "{}",
            validate_replay_readiness_networked_hosted_scheduler_receipt_path(path)?
        );
    }
    Ok(())
}

fn parse_args() -> EvidenceResult<Args> {
    let mut parsed = Args::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--output" => parsed.output = Some(next_path(&mut args, "--output")?),
            "--materialize-ci-plans" => {
                parsed.ci_plan_output = Some(next_path(&mut args, "--materialize-ci-plans")?);
            }
            "--materialize-command-plan" => {
                parsed.command_plan_output =
                    Some(next_path(&mut args, "--materialize-command-plan")?);
            }
            "--executable" => {
                parsed.executable = Some(next_path(&mut args, "--executable")?);
            }
            "--argument" => {
                parsed.arguments.push(
                    args.next()
                        .ok_or_else(|| {
                            chaoscontrol_evidence::EvidenceError::new(
                                "--argument requires one UTF-8 value",
                            )
                        })?
                        .into_string()
                        .map_err(|_| {
                            chaoscontrol_evidence::EvidenceError::new(
                                "--argument requires one UTF-8 value",
                            )
                        })?,
                );
            }
            "--replay-readiness" => {
                parsed.replay_readiness = Some(next_path(&mut args, "--replay-readiness")?);
            }
            "--check" => parsed.check = Some(next_path(&mut args, "--check")?),
            "--run-plan" => parsed.run_plan = Some(next_path(&mut args, "--run-plan")?),
            "--check-execution" => {
                parsed.check_execution = Some(next_path(&mut args, "--check-execution")?);
            }
            "--run-fleet-plan" => {
                parsed.run_fleet_plan = Some(next_path(&mut args, "--run-fleet-plan")?);
            }
            "--check-fleet" => parsed.check_fleet = Some(next_path(&mut args, "--check-fleet")?),
            "--run-multi-hypervisor-plan" => {
                parsed.run_multi_hypervisor_plan =
                    Some(next_path(&mut args, "--run-multi-hypervisor-plan")?);
            }
            "--check-multi-hypervisor" => {
                parsed.check_multi_hypervisor =
                    Some(next_path(&mut args, "--check-multi-hypervisor")?);
            }
            "--render-multi-hypervisor-dashboard" => {
                parsed.render_multi_hypervisor_dashboard =
                    Some(next_path(&mut args, "--render-multi-hypervisor-dashboard")?);
            }
            "--run-hosted-shared-state-plan" => {
                parsed.run_hosted_shared_state_plan =
                    Some(next_path(&mut args, "--run-hosted-shared-state-plan")?);
            }
            "--check-hosted-shared-state" => {
                parsed.check_hosted_shared_state =
                    Some(next_path(&mut args, "--check-hosted-shared-state")?);
            }
            "--run-networked-hosted-plan" => {
                parsed.run_networked_hosted_plan =
                    Some(next_path(&mut args, "--run-networked-hosted-plan")?);
            }
            "--check-networked-hosted" => {
                parsed.check_networked_hosted =
                    Some(next_path(&mut args, "--check-networked-hosted")?);
            }
            "--sample" => parsed.sample = true,
            "--sample-fleet" => parsed.sample_fleet = true,
            "--sample-fleet-plan" => parsed.sample_fleet_plan = true,
            "--sample-multi-hypervisor" => parsed.sample_multi_hypervisor = true,
            "--sample-multi-hypervisor-plan" => parsed.sample_multi_hypervisor_plan = true,
            "--sample-hosted-shared-state" => parsed.sample_hosted_shared_state = true,
            "--sample-hosted-shared-state-plan" => parsed.sample_hosted_shared_state_plan = true,
            "--sample-networked-hosted" => parsed.sample_networked_hosted = true,
            "--sample-networked-hosted-plan" => parsed.sample_networked_hosted_plan = true,
            _ => {
                return Err(chaoscontrol_evidence::EvidenceError::new(format!(
                    "unexpected argument: {}\n{}",
                    arg.to_string_lossy(),
                    usage()
                )));
            }
        }
    }
    Ok(parsed)
}

fn next_path(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    flag: &str,
) -> EvidenceResult<std::path::PathBuf> {
    args.next()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| chaoscontrol_evidence::EvidenceError::new(format!("{flag} requires a path")))
}

fn require_output(output: Option<std::path::PathBuf>) -> EvidenceResult<std::path::PathBuf> {
    output.ok_or_else(|| chaoscontrol_evidence::EvidenceError::new("--output requires a path"))
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) -> EvidenceResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
