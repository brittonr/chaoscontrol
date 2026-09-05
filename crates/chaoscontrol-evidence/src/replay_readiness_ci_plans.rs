//! CI plan materialization for typed replay-readiness commands.

// r[impl chaoscontrol.typed_operator_commands.plan]
// r[impl chaoscontrol.typed_operator_commands.evidence]

use serde_json::{json, Value};

use crate::replay_readiness_orchestration::observe_executable_reference;
use crate::replay_readiness_publication::write_bytes;
use crate::replay_readiness_surfaces::{
    sample_fleet_scheduler_plan, sample_hosted_shared_state_plan,
    sample_multi_hypervisor_campaign_plan, sample_networked_hosted_scheduler_plan,
    sample_scheduler_receipt,
};
use crate::typed_operator_command::{
    CommandPlan, EnvironmentMode, EnvironmentSpec, ExecutableRef, LimitSpec, StdinSpec,
    TerminationScope,
};
use crate::EvidenceResult;

const EXECUTABLE_MAX_BYTES: u64 = 1_073_741_824;
const TIMEOUT_MS: u64 = 30_000;
const STDIN_MAX_BYTES: u64 = 1_024;
const OUTPUT_MAX_BYTES: u64 = 1_048_576;
const POLL_INTERVAL_MS: u64 = 10;
const TEARDOWN_TIMEOUT_MS: u64 = 1_000;
const PLAN_FILE_COUNT: usize = 5;

/// Observe one executable and write one typed command plan.
pub fn write_typed_command_plan(
    output_path: impl AsRef<std::path::Path>,
    executable_path: impl AsRef<std::path::Path>,
    args: Vec<String>,
) -> EvidenceResult<()> {
    let executable = observe_executable_reference(executable_path.as_ref(), EXECUTABLE_MAX_BYTES)?;
    let plan = command_with_args(&executable, args);
    let mut bytes = serde_json::to_vec_pretty(&plan)?;
    bytes.push(b'\n');
    write_bytes(output_path.as_ref(), &bytes)
}

/// Observe one packaged executable and write all CI scheduler plans.
pub fn write_ci_scheduler_plans(
    output_root: impl AsRef<std::path::Path>,
    executable_path: impl AsRef<std::path::Path>,
) -> EvidenceResult<usize> {
    let output_root = output_root.as_ref();
    let executable = observe_executable_reference(executable_path.as_ref(), EXECUTABLE_MAX_BYTES)?;
    let plans = build_ci_scheduler_plans(output_root, &executable)?;
    for (path, value) in &plans {
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        write_bytes(path, &bytes)?;
    }
    Ok(plans.len())
}

fn build_ci_scheduler_plans(
    output_root: &std::path::Path,
    executable: &ExecutableRef,
) -> EvidenceResult<Vec<(std::path::PathBuf, Value)>> {
    let path = |name: &str| output_root.join(name).display().to_string();

    let mut scheduler = sample_scheduler_receipt();
    scheduler["source_decision_receipt"] = json!(path("decision-receipt.json"));
    set_command_entry(
        &mut scheduler["run_plan"][0],
        executable,
        &path("scheduled-run-1.json"),
    )?;
    set_command_entry(
        &mut scheduler["run_plan"][1],
        executable,
        &path("scheduled-run-2.json"),
    )?;

    let mut fleet = sample_fleet_scheduler_plan();
    fleet["queue"]["state_path"] = json!(path("fleet-scheduler-state.json"));
    fleet["operator_decisions"] = json!([path("decision-receipt.json")]);
    set_command_entry(
        &mut fleet["queue"]["entries"][0],
        executable,
        &path("fleet-scheduled-run-1.json"),
    )?;
    set_command_entry(
        &mut fleet["queue"]["entries"][1],
        executable,
        &path("fleet-scheduled-run-2.json"),
    )?;

    let mut multi = sample_multi_hypervisor_campaign_plan();
    multi["state_path"] = json!(path("local-multi-hypervisor-campaign-state.json"));
    multi["artifact_index_path"] = json!(path("local-multi-hypervisor-artifact-index.json"));
    multi["follow_up_policy"] = json!({"enabled": false, "reproduce": false, "minimize": false});
    multi["hypervisors"][0]["artifact_root"] = json!(path("local-hv-a"));
    multi["hypervisors"][1]["artifact_root"] = json!(path("local-hv-b"));
    multi["operator_decisions"] = json!([path("decision-receipt.json")]);
    multi["queue"]["entries"][0]["expected_bug_artifacts"] = json!([]);
    multi["queue"]["entries"][1]["expected_bug_artifacts"] = json!([]);
    set_command_entry(
        &mut multi["queue"]["entries"][0],
        executable,
        &path("local-multi-hypervisor-run-1.json"),
    )?;
    set_command_entry(
        &mut multi["queue"]["entries"][1],
        executable,
        &path("local-multi-hypervisor-run-2.json"),
    )?;

    let mut hosted = sample_hosted_shared_state_plan();
    hosted["queue"]["state_path"] = json!(path("hosted-shared-queue-state.json"));
    hosted["decision_store"]["path"] = json!(path("hosted-shared-decision-store.json"));
    set_command_entry(
        &mut hosted["queue"]["entries"][0],
        executable,
        &path("hosted-run-1.json"),
    )?;
    set_command_entry(
        &mut hosted["queue"]["entries"][1],
        executable,
        &path("hosted-run-2.json"),
    )?;

    let mut networked = sample_networked_hosted_scheduler_plan();
    networked["queue"]["state_snapshot_path"] = json!(path("networked-hosted-queue-state.json"));
    networked["decision_store"]["state_snapshot_path"] =
        json!(path("networked-hosted-decision-store.json"));
    set_command_entry(
        &mut networked["queue"]["entries"][0],
        executable,
        &path("networked-run-1.json"),
    )?;
    set_command_entry(
        &mut networked["queue"]["entries"][1],
        executable,
        &path("networked-run-2.json"),
    )?;

    let plans = vec![
        (output_root.join("scheduler-execution-plan.json"), scheduler),
        (output_root.join("fleet-scheduler-plan.json"), fleet),
        (
            output_root.join("local-multi-hypervisor-campaign-plan.json"),
            multi,
        ),
        (output_root.join("hosted-shared-state-plan.json"), hosted),
        (
            output_root.join("networked-hosted-scheduler-plan.json"),
            networked,
        ),
    ];
    debug_assert_eq!(plans.len(), PLAN_FILE_COUNT);
    Ok(plans)
}

fn set_command_entry(
    entry: &mut Value,
    executable: &ExecutableRef,
    receipt_path: &str,
) -> EvidenceResult<()> {
    entry["receipt_path"] = json!(receipt_path);
    entry["command_plan"] = serde_json::to_value(command_with_args(
        executable,
        vec!["--receipt".to_string(), receipt_path.to_string()],
    ))?;
    Ok(())
}

fn command_with_args(executable: &ExecutableRef, args: Vec<String>) -> CommandPlan {
    CommandPlan {
        schema: crate::typed_operator_command::PLAN_SCHEMA.to_string(),
        mechanism_revision: crate::typed_operator_command::MECHANISM_REVISION.to_string(),
        executable: executable.clone(),
        args,
        working_directory: ".".to_string(),
        environment: EnvironmentSpec {
            mode: EnvironmentMode::Clear,
            entries: Vec::new(),
        },
        stdin: StdinSpec::Null,
        limits: LimitSpec {
            timeout_ms: TIMEOUT_MS,
            stdin_max_bytes: STDIN_MAX_BYTES,
            stdout_max_bytes: OUTPUT_MAX_BYTES,
            stderr_max_bytes: OUTPUT_MAX_BYTES,
            poll_interval_ms: POLL_INTERVAL_MS,
            teardown_timeout_ms: TEARDOWN_TIMEOUT_MS,
        },
        accepted_exit_codes: vec![0],
        reject_stdout_truncation: true,
        reject_stderr_truncation: true,
        termination_scope: TerminationScope::ProcessGroup,
        evidence_eligible: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn pure_plan_builder_uses_typed_commands_and_expected_paths() {
        let executable = ExecutableRef {
            path: "/nix/store/example/bin/replay-readiness".to_string(),
            blake3: DIGEST.to_string(),
            maximum_bytes: EXECUTABLE_MAX_BYTES,
        };
        let plans = build_ci_scheduler_plans(std::path::Path::new("/tmp/ci-plans"), &executable)
            .expect("build CI plans");
        assert_eq!(plans.len(), PLAN_FILE_COUNT);
        for (_, plan) in &plans {
            let encoded = serde_json::to_string(plan).expect("encode plan");
            assert!(encoded.contains("command_plan"));
            assert!(!encoded.contains("\"command\":\"replay-readiness --"));
        }
        assert_eq!(
            plans[0].1["run_plan"][0]["receipt_path"],
            "/tmp/ci-plans/scheduled-run-1.json"
        );
    }
}
