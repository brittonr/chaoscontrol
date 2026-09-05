use std::env;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Component;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use chaoscontrol_evidence::rust_automation::accepted_dogfood::{
    rewrite_public_verdict, snapshot_bug_is_candidate, summarize_attempt,
    validate_snapshot_reference, verdict_is_accepted, AttemptInput,
};
use chaoscontrol_evidence::rust_automation::time::rfc3339_utc;
use chaoscontrol_evidence::typed_operator_command::{
    CommandPlan, EnvironmentMode, EnvironmentSpec, LimitSpec, StdinSpec, TerminationScope,
    MECHANISM_REVISION, PLAN_SCHEMA,
};
use chaoscontrol_evidence::{execute_typed_operator_command_captured, observe_typed_executable};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SUCCESS_EXIT: i32 = 0;
const NO_ACCEPTED_VERDICT_EXIT: i32 = 2;
const TIMEOUT_EXIT: i32 = 124;
const DEFAULT_MAX_ATTEMPTS: usize = 1;
const DEFAULT_START_SEED: i64 = 42;
const DEFAULT_RUN_TIMEOUT_SECONDS: u64 = 240;
const DEFAULT_EXPORT_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_REPLAY_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_BOOTSTRAP_TICKS: i64 = 10_000;
const DEFAULT_MEMORY_MIB: i64 = 128;
const DEFAULT_VMS: i64 = 3;
const DEFAULT_ROUNDS: i64 = 3;
const DEFAULT_BRANCHES: i64 = 2;
const DEFAULT_TICKS: i64 = 80;
const DEFAULT_ASSERTION_ID: i64 = 1_806_003_755;
const DEFAULT_FAIL_AFTER: &str = "1";
const DEFAULT_WORKLOAD: &str = "raft";
const DEFAULT_CMDLINE: &str =
    "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after={fail_after}";
const SNAPSHOT_CHUNK_BYTES: usize = 20 * 1_024 * 1_024;
const MAX_EXPORTED_BUGS: i64 = 8;
const EXECUTABLE_MAX_BYTES: u64 = 1_073_741_824;
const STDIN_MAX_BYTES: u64 = 1_024;
const OUTPUT_MAX_BYTES: u64 = 33_554_432;
const POLL_INTERVAL_MS: u64 = 10;
const TEARDOWN_TIMEOUT_MS: u64 = 1_000;
const MAX_JSON_BYTES: u64 = 64 * 1_024 * 1_024;
const HASH_BUFFER_BYTES: usize = 64 * 1_024;

#[derive(Debug)]
struct Options {
    output: Option<std::path::PathBuf>,
    kernel: Option<std::path::PathBuf>,
    initrd: Option<std::path::PathBuf>,
    explore: std::path::PathBuf,
    cohort: Option<std::path::PathBuf>,
    evidence_prefix: String,
    refresh_output: Option<std::path::PathBuf>,
    max_attempts: usize,
    start_seed: i64,
    run_timeout_seconds: u64,
    export_timeout_seconds: u64,
    replay_timeout_seconds: u64,
    vms: i64,
    rounds: i64,
    branches: i64,
    ticks: i64,
    memory_mib: i64,
    disk_image: Option<std::path::PathBuf>,
    workload: String,
    assertion_id: i64,
    cmdline_template: String,
    fail_after_values: Vec<i64>,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(u8::try_from(NO_ACCEPTED_VERDICT_EXIT).expect("exit fits"))
        }
    }
}

fn run(args: Vec<String>) -> Result<i32, String> {
    let options = parse_args(&args)?;
    if let Some(output) = &options.refresh_output {
        refresh_existing_output(output, &options.evidence_prefix, &options.workload)?;
        return Ok(SUCCESS_EXIT);
    }
    require_kvm()?;
    let cohort_path = options
        .cohort
        .as_ref()
        .ok_or_else(|| String::from("--cohort is required"))?;
    let kernel = options
        .kernel
        .as_ref()
        .ok_or_else(|| String::from("--kernel/--initrd or KERNEL/INITRD are required"))?;
    let initrd = options
        .initrd
        .as_ref()
        .ok_or_else(|| String::from("--kernel/--initrd or KERNEL/INITRD are required"))?;
    let cohort = load_json(cohort_path)?;
    let workload_profile = cohort
        .get("workloads")
        .and_then(Value::as_array)
        .and_then(|profiles| {
            profiles.iter().find(|profile| {
                profile.get("workload").and_then(Value::as_str) == Some(&options.workload)
            })
        })
        .cloned()
        .ok_or_else(|| format!("workload {:?} is absent from the cohort", options.workload))?;
    if workload_profile
        .pointer("/assertion/compatibility_id")
        .and_then(Value::as_i64)
        != Some(options.assertion_id)
    {
        return Err(String::from(
            "--assertion-id differs from the admitted cohort",
        ));
    }
    if workload_profile
        .get("cmdline_template")
        .and_then(Value::as_str)
        != Some(&options.cmdline_template)
    {
        return Err(String::from(
            "--cmdline-template differs from the admitted cohort",
        ));
    }
    let output = match &options.output {
        Some(path) => absolute(path)?,
        None => absolute(std::path::Path::new(&format!(
            "dogfood-results/{}-accepted-verdict-dogfood-{}",
            options.workload,
            now()?
                .replace(['-', ':'], "")
                .replace('T', "-")
                .replace('Z', "")
        )))?,
    };
    prepare_output(&output)?;
    let scratch = output.join("attempts");
    fs::create_dir(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let executable = observe_typed_executable(&options.explore, EXECUTABLE_MAX_BYTES)
        .map_err(|error| error.to_string())?;
    let execution_root =
        env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let mut attempts = Vec::new();
    for attempt_index in 0..options.max_attempts {
        let attempt_number = attempt_index + 1;
        let seed = options
            .start_seed
            .checked_add(
                i64::try_from(attempt_index).map_err(|_| String::from("attempt index overflow"))?,
            )
            .ok_or_else(|| String::from("seed overflow"))?;
        let fail_after = options.fail_after_values[attempt_index % options.fail_after_values.len()];
        let run_dir = scratch.join(format!("attempt-{attempt_number:02}"));
        fs::create_dir(&run_dir).map_err(|error| format!("{}: {error}", run_dir.display()))?;
        let extra_cmdline =
            render_cmdline(&options.cmdline_template, fail_after, seed, attempt_number);
        let mut run_args = vec![
            String::from("run"),
            String::from("--kernel"),
            kernel.display().to_string(),
            String::from("--initrd"),
            initrd.display().to_string(),
            String::from("--output"),
            run_dir.display().to_string(),
            String::from("--vms"),
            options.vms.to_string(),
            String::from("--rounds"),
            options.rounds.to_string(),
            String::from("--branches"),
            options.branches.to_string(),
            String::from("--ticks"),
            options.ticks.to_string(),
            String::from("--seed"),
            seed.to_string(),
            String::from("--mode"),
            String::from("hybrid"),
            String::from("--bootstrap-budget"),
            DEFAULT_BOOTSTRAP_TICKS.to_string(),
            String::from("--memory-mb"),
            options.memory_mib.to_string(),
            String::from("--extra-cmdline"),
            extra_cmdline.clone(),
        ];
        append_disk(&mut run_args, options.disk_image.as_deref());
        let run_rc = run_command(
            executable.clone(),
            run_args,
            &execution_root,
            options.run_timeout_seconds,
            &run_dir.join("run.log"),
        )?;
        let mut export_rc = None;
        let mut reproduce_rc = None;
        let mut verdict_path = None;
        let mut verdict_value = None;
        if [SUCCESS_EXIT, 1, TIMEOUT_EXIT].contains(&run_rc)
            && run_dir.join("checkpoint.json").is_file()
        {
            let export_args = vec![
                String::from("export-bugs"),
                String::from("--checkpoint"),
                run_dir.join("checkpoint.json").display().to_string(),
                String::from("--output"),
                run_dir.display().to_string(),
                String::from("--assertion-id"),
                options.assertion_id.to_string(),
                String::from("--min-replay-parent-depth"),
                String::from("1"),
                String::from("--max-bugs"),
                MAX_EXPORTED_BUGS.to_string(),
            ];
            let code = run_command(
                executable.clone(),
                export_args,
                &execution_root,
                options.export_timeout_seconds,
                &run_dir.join("export-bugs.log"),
            )?;
            export_rc = Some(code);
            if code == SUCCESS_EXIT {
                if let Some((bug_path, bug)) = select_snapshot_bug(
                    &run_dir,
                    options.assertion_id,
                    &workload_profile["assertion"],
                )? {
                    let suffix = bug_path
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or("bug")
                        .trim_start_matches("bug_");
                    let candidate_verdict =
                        run_dir.join(format!("replay-verdict-bug{suffix}.json"));
                    let mut reproduce_args = vec![
                        String::from("reproduce"),
                        String::from("--kernel"),
                        kernel.display().to_string(),
                        String::from("--initrd"),
                        initrd.display().to_string(),
                        String::from("--bug"),
                        bug_path.display().to_string(),
                        String::from("--vms"),
                        options.vms.to_string(),
                        String::from("--seed"),
                        seed.to_string(),
                        String::from("--bootstrap-budget"),
                        DEFAULT_BOOTSTRAP_TICKS.to_string(),
                        String::from("--memory-mb"),
                        options.memory_mib.to_string(),
                        String::from("--extra-cmdline"),
                        extra_cmdline,
                        String::from("--verdict-output"),
                        candidate_verdict.display().to_string(),
                    ];
                    append_disk(&mut reproduce_args, options.disk_image.as_deref());
                    let code = run_command(
                        executable.clone(),
                        reproduce_args,
                        &execution_root,
                        options.replay_timeout_seconds,
                        &run_dir.join("reproduce.log"),
                    )?;
                    reproduce_rc = Some(code);
                    verdict_path = Some(candidate_verdict.clone());
                    if candidate_verdict.is_file() {
                        verdict_value = Some(load_json(&candidate_verdict)?);
                    }
                    if code == SUCCESS_EXIT
                        && verdict_value.as_ref().is_some_and(|verdict| {
                            verdict_is_accepted(verdict, &bug, options.assertion_id, &bug_path)
                        })
                    {
                        accept_output(
                            &output,
                            &run_dir,
                            &bug_path,
                            &candidate_verdict,
                            &options.evidence_prefix,
                            &options.workload,
                            &cohort,
                            &workload_profile,
                            &options.explore,
                            kernel,
                            initrd,
                            options.disk_image.as_deref(),
                            seed,
                            fail_after,
                            run_rc,
                            export_rc,
                            reproduce_rc,
                        )?;
                        println!(
                            "accepted snapshot-backed verdict: {}",
                            output
                                .join(candidate_verdict.file_name().expect("verdict name"))
                                .display()
                        );
                        return Ok(SUCCESS_EXIT);
                    }
                }
            }
        }
        let bugs = load_bugs(&run_dir)?;
        attempts.push(summarize_attempt(&AttemptInput {
            workload: &options.workload,
            seed,
            fail_after,
            run_exit_status: run_rc,
            export_exit_status: export_rc,
            reproduce_exit_status: reproduce_rc,
            bugs: &bugs,
            verdict_path: verdict_path.as_deref(),
            verdict: verdict_value.as_ref(),
        }));
        write_json(
            &output.join("attempts-summary.json"),
            &json!({"accepted": false, "attempts": attempts}),
        )?;
        eprintln!(
            "attempt {attempt_number}/{}: no accepted snapshot-backed verdict",
            options.max_attempts
        );
    }
    eprintln!(
        "no accepted snapshot-backed verdict after {} attempts; see {}",
        options.max_attempts,
        output.join("attempts-summary.json").display()
    );
    Ok(NO_ACCEPTED_VERDICT_EXIT)
}

#[allow(clippy::too_many_arguments)]
fn accept_output(
    output: &std::path::Path,
    run_dir: &std::path::Path,
    bug_path: &std::path::Path,
    verdict_path: &std::path::Path,
    evidence_prefix: &str,
    workload: &str,
    cohort: &Value,
    workload_profile: &Value,
    explore: &std::path::Path,
    kernel: &std::path::Path,
    initrd: &std::path::Path,
    disk_image: Option<&std::path::Path>,
    seed: i64,
    fail_after: i64,
    run_rc: i32,
    export_rc: Option<i32>,
    reproduce_rc: Option<i32>,
) -> Result<(), String> {
    let assertions = run_dir.join("assertions.json");
    let bug_name = file_name(bug_path)?;
    let verdict_name = file_name(verdict_path)?;
    fs::copy(
        &assertions,
        output.join(&bug_name).with_file_name("assertions.json"),
    )
    .map_err(|error| format!("{}: {error}", assertions.display()))?;
    fs::copy(bug_path, output.join(&bug_name))
        .map_err(|error| format!("{}: {error}", bug_path.display()))?;
    fs::copy(verdict_path, output.join(&verdict_name))
        .map_err(|error| format!("{}: {error}", verdict_path.display()))?;
    let bug = load_json(bug_path)?;
    let reference = &bug["replay_parent_snapshot_ref"];
    let snapshot = safe_snapshot_path(run_dir, reference)?;
    let snapshots = output.join("snapshots");
    fs::create_dir_all(&snapshots).map_err(|error| format!("{}: {error}", snapshots.display()))?;
    let copied_snapshot = snapshots.join(file_name(&snapshot)?);
    fs::copy(&snapshot, &copied_snapshot)
        .map_err(|error| format!("{}: {error}", snapshot.display()))?;
    rewrite_public_paths(output, &bug_name, &verdict_name, evidence_prefix)?;
    chunk_snapshot(&copied_snapshot)?;
    let runtime_artifacts = runtime_artifacts(explore, kernel, initrd, disk_image)?;
    write_proof_receipt(
        output,
        cohort,
        workload_profile,
        &bug_name,
        &verdict_name,
        evidence_prefix,
        runtime_artifacts,
    )?;
    let bugs = load_bugs(run_dir)?;
    let verdict = load_json(verdict_path)?;
    let mut summary = summarize_attempt(&AttemptInput {
        workload,
        seed,
        fail_after,
        run_exit_status: run_rc,
        export_exit_status: export_rc,
        reproduce_exit_status: reproduce_rc,
        bugs: &bugs,
        verdict_path: Some(verdict_path),
        verdict: Some(&verdict),
    });
    summary["accepted"] = Value::Bool(true);
    summary["accepted_bug"] = Value::String(format!("{evidence_prefix}/{bug_name}"));
    summary["accepted_verdict"] = Value::String(format!("{evidence_prefix}/{verdict_name}"));
    summary["verdict"]["path"] = summary["accepted_verdict"].clone();
    summary["receipt"] = Value::String(format!("{evidence_prefix}/proof-receipt.json"));
    write_json(
        &output.join("accepted-snapshot-verdict-summary.json"),
        &summary,
    )
}

fn run_command(
    executable: chaoscontrol_evidence::typed_operator_command::ExecutableRef,
    args: Vec<String>,
    execution_root: &std::path::Path,
    timeout_seconds: u64,
    log: &std::path::Path,
) -> Result<i32, String> {
    let timeout_ms = timeout_seconds
        .checked_mul(1_000)
        .ok_or_else(|| String::from("command timeout overflow"))?;
    let plan = CommandPlan {
        schema: String::from(PLAN_SCHEMA),
        mechanism_revision: String::from(MECHANISM_REVISION),
        executable,
        args,
        working_directory: String::from("."),
        environment: EnvironmentSpec {
            mode: EnvironmentMode::Clear,
            entries: Vec::new(),
        },
        stdin: StdinSpec::Null,
        limits: LimitSpec {
            timeout_ms,
            stdin_max_bytes: STDIN_MAX_BYTES,
            stdout_max_bytes: OUTPUT_MAX_BYTES,
            stderr_max_bytes: OUTPUT_MAX_BYTES,
            poll_interval_ms: POLL_INTERVAL_MS,
            teardown_timeout_ms: TEARDOWN_TIMEOUT_MS,
        },
        accepted_exit_codes: vec![0, 1],
        reject_stdout_truncation: true,
        reject_stderr_truncation: true,
        termination_scope: TerminationScope::ProcessGroup,
        evidence_eligible: true,
    };
    let captured = execute_typed_operator_command_captured(&plan, execution_root)
        .map_err(|error| error.to_string())?;
    let parent = log.parent().unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let mut file = fs::File::create(log).map_err(|error| format!("{}: {error}", log.display()))?;
    file.write_all(&captured.stdout)
        .map_err(|error| format!("{}: {error}", log.display()))?;
    file.write_all(&captured.stderr)
        .map_err(|error| format!("{}: {error}", log.display()))?;
    if captured.observation.stdout.truncated || captured.observation.stderr.truncated {
        return Err(format!(
            "{}: bounded command output was truncated",
            log.display()
        ));
    }
    if captured.observation.completion == "timed-out" {
        writeln!(
            file,
            "\n[accepted-snapshot-verdict-dogfood] timeout after {timeout_seconds}s"
        )
        .map_err(|error| format!("{}: {error}", log.display()))?;
        return Ok(TIMEOUT_EXIT);
    }
    Ok(captured.observation.exit_code.unwrap_or(1))
}

fn select_snapshot_bug(
    run_dir: &std::path::Path,
    assertion_id: i64,
    assertion_profile: &Value,
) -> Result<Option<(std::path::PathBuf, Value)>, String> {
    for path in matching_files(run_dir, "bug_", ".json")? {
        let bug = load_json(&path)?;
        if !snapshot_bug_is_candidate(&bug, assertion_id, assertion_profile) {
            continue;
        }
        let reference = &bug["replay_parent_snapshot_ref"];
        let artifact = safe_snapshot_path(run_dir, reference)?;
        if !artifact.is_file() {
            return Err(format!(
                "missing snapshot artifact for {}: {}",
                path.display(),
                artifact.display()
            ));
        }
        validate_snapshot_reference(reference, &sha256(&artifact)?)?;
        return Ok(Some((path, bug)));
    }
    Ok(None)
}

fn safe_snapshot_path(
    run_dir: &std::path::Path,
    reference: &Value,
) -> Result<std::path::PathBuf, String> {
    let raw = reference
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("snapshot ref path is missing"))?;
    let relative = std::path::Path::new(raw);
    let mut components = relative.components();
    if relative.is_absolute()
        || components.next() != Some(Component::Normal("snapshots".as_ref()))
        || components.any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("unconfined snapshot ref path: {raw}"));
    }
    Ok(run_dir.join(relative))
}

fn rewrite_public_paths(
    output: &std::path::Path,
    bug_name: &str,
    verdict_name: &str,
    evidence_prefix: &str,
) -> Result<(), String> {
    let bug = load_json(&output.join(bug_name))?;
    let reference = &bug["replay_parent_snapshot_ref"];
    let public_bug = format!("{evidence_prefix}/{bug_name}");
    let public_snapshot = format!(
        "{evidence_prefix}/{}",
        reference.get("path").and_then(Value::as_str).unwrap_or("")
    );
    let verdict_path = output.join(verdict_name);
    let mut verdict = load_json(&verdict_path)?;
    let original_verdict = verdict.clone();
    let old_bug = verdict
        .get("bug_path")
        .and_then(Value::as_str)
        .unwrap_or(&public_bug)
        .to_string();
    rewrite_public_verdict(
        &mut verdict,
        &old_bug,
        &public_bug,
        &public_snapshot,
        &sha256(&output.join(bug_name))?,
        reference
            .get("digest")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )?;
    if verdict == original_verdict {
        Ok(())
    } else {
        write_json(&verdict_path, &verdict)
    }
}

fn refresh_existing_output(
    output: &std::path::Path,
    evidence_prefix: &str,
    workload: &str,
) -> Result<(), String> {
    let mut summary = load_json(&output.join("accepted-snapshot-verdict-summary.json"))?;
    let bug_name = basename(summary.get("accepted_bug"))?;
    let verdict_name = basename(summary.get("accepted_verdict"))?;
    let public_verdict = format!("{evidence_prefix}/{verdict_name}");
    summary["verdict"]["path"] = Value::String(public_verdict.clone());
    summary["accepted_verdict"] = Value::String(public_verdict.clone());
    write_json(
        &output.join("accepted-snapshot-verdict-summary.json"),
        &summary,
    )?;
    rewrite_public_paths(output, &bug_name, &verdict_name, evidence_prefix)?;
    let receipt_path = output.join("proof-receipt.json");
    let mut receipt = load_json(&receipt_path)?;
    let mut updated = false;
    if let Some(artifacts) = receipt.get_mut("artifacts").and_then(Value::as_array_mut) {
        for artifact in artifacts {
            if artifact.get("path").and_then(Value::as_str) == Some(&public_verdict) {
                artifact["sha256"] = Value::String(sha256(&output.join(&verdict_name))?);
                updated = true;
            }
        }
    }
    if !updated {
        return Err(format!("receipt lacks verdict artifact: {public_verdict}"));
    }
    let _ = workload;
    write_json(&receipt_path, &receipt)
}

fn write_proof_receipt(
    output: &std::path::Path,
    cohort: &Value,
    profile: &Value,
    bug_name: &str,
    verdict_name: &str,
    evidence_prefix: &str,
    runtime_artifacts: Vec<Value>,
) -> Result<(), String> {
    let bug = load_json(&output.join(bug_name))?;
    let reference = &bug["replay_parent_snapshot_ref"];
    let artifacts = vec![
        json!({"path": format!("{evidence_prefix}/assertions.json"), "sha256": sha256(&output.join("assertions.json"))?}),
        json!({"path": format!("{evidence_prefix}/{bug_name}"), "sha256": sha256(&output.join(bug_name))?}),
        json!({"path": format!("{evidence_prefix}/{verdict_name}"), "sha256": sha256(&output.join(verdict_name))?}),
        json!({"path": format!("{evidence_prefix}/{}", reference.get("path").and_then(Value::as_str).unwrap_or("")), "sha256": reference.get("digest").cloned().unwrap_or(Value::Null)}),
    ];
    let receipt = json!({
        "schema_version": 1, "status": "accepted",
        "scope": "bounded snapshot-backed replay proof for the recorded workload and cohort",
        "cohort_id": cohort.get("cohort_id").cloned().unwrap_or(Value::Null),
        "source_revision": cohort.get("source_revision").cloned().unwrap_or(Value::Null),
        "workload": profile.get("workload").cloned().unwrap_or(Value::Null),
        "assertion": profile.get("assertion").cloned().unwrap_or(Value::Null),
        "bounds": profile.get("bounds").cloned().unwrap_or(Value::Null),
        "execution": cohort.get("execution").cloned().unwrap_or(Value::Null),
        "kvm_observation": {"readable": true, "writable": true},
        "runtime_artifacts": runtime_artifacts,
        "snapshot_policy": cohort.get("snapshot_policy").cloned().unwrap_or(Value::Null),
        "replay_policy": cohort.get("replay_policy").cloned().unwrap_or(Value::Null),
        "artifacts": artifacts,
        "non_claims": cohort.get("non_claims").cloned().unwrap_or(Value::Null),
    });
    write_json(&output.join("proof-receipt.json"), &receipt)
}

fn runtime_artifacts(
    explore: &std::path::Path,
    kernel: &std::path::Path,
    initrd: &std::path::Path,
    disk: Option<&std::path::Path>,
) -> Result<Vec<Value>, String> {
    let mut facts = vec![
        artifact("host-binary", explore)?,
        artifact("guest-kernel", kernel)?,
        artifact("guest-initrd", initrd)?,
    ];
    if let Some(path) = disk {
        facts.push(artifact("guest-disk", path)?);
    }
    Ok(facts)
}

fn artifact(role: &str, path: &std::path::Path) -> Result<Value, String> {
    Ok(json!({"role": role, "path": path, "sha256": sha256(path)?}))
}

fn chunk_snapshot(snapshot: &std::path::Path) -> Result<(), String> {
    let size = fs::metadata(snapshot)
        .map_err(|error| format!("{}: {error}", snapshot.display()))?
        .len();
    if size <= u64::try_from(SNAPSHOT_CHUNK_BYTES).expect("chunk bound fits") {
        return Ok(());
    }
    let mut reader = BufReader::new(
        fs::File::open(snapshot).map_err(|error| format!("{}: {error}", snapshot.display()))?,
    );
    let mut chunks = Vec::new();
    let mut index = 0_usize;
    loop {
        let mut data = vec![0_u8; SNAPSHOT_CHUNK_BYTES];
        let count = reader
            .read(&mut data)
            .map_err(|error| format!("{}: {error}", snapshot.display()))?;
        if count == 0 {
            break;
        }
        data.truncate(count);
        let name = format!("{}.part{index:02}", file_name(snapshot)?);
        fs::write(snapshot.with_file_name(&name), &data)
            .map_err(|error| format!("{}: {error}", name))?;
        chunks.push(json!({"path": format!("snapshots/{name}"), "size": count, "sha256": format!("{:x}", Sha256::digest(&data))}));
        index += 1;
    }
    let manifest = json!({
        "schema_version": 1, "original_path": file_name(snapshot)?, "original_size": size,
        "original_sha256": sha256(snapshot)?.trim_start_matches("sha256:"), "chunks": chunks,
    });
    write_json(
        &snapshot.with_file_name(format!("{}.chunks.json", file_name(snapshot)?)),
        &manifest,
    )?;
    fs::remove_file(snapshot).map_err(|error| format!("{}: {error}", snapshot.display()))
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        output: None,
        kernel: env::var_os("KERNEL").map(std::path::PathBuf::from),
        initrd: env::var_os("INITRD").map(std::path::PathBuf::from),
        explore: std::path::PathBuf::from(
            env::var("CHAOSCONTROL_EXPLORE")
                .unwrap_or_else(|_| String::from("chaoscontrol-explore")),
        ),
        cohort: None,
        evidence_prefix: String::new(),
        refresh_output: None,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        start_seed: DEFAULT_START_SEED,
        run_timeout_seconds: DEFAULT_RUN_TIMEOUT_SECONDS,
        export_timeout_seconds: DEFAULT_EXPORT_TIMEOUT_SECONDS,
        replay_timeout_seconds: DEFAULT_REPLAY_TIMEOUT_SECONDS,
        vms: DEFAULT_VMS,
        rounds: DEFAULT_ROUNDS,
        branches: DEFAULT_BRANCHES,
        ticks: DEFAULT_TICKS,
        memory_mib: DEFAULT_MEMORY_MIB,
        disk_image: None,
        workload: String::from(DEFAULT_WORKLOAD),
        assertion_id: DEFAULT_ASSERTION_ID,
        cmdline_template: String::from(DEFAULT_CMDLINE),
        fail_after_values: vec![DEFAULT_FAIL_AFTER.parse().expect("default integer")],
    };
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--output" => options.output = Some(std::path::PathBuf::from(value)),
            "--kernel" => options.kernel = Some(std::path::PathBuf::from(value)),
            "--initrd" => options.initrd = Some(std::path::PathBuf::from(value)),
            "--explore" => options.explore = std::path::PathBuf::from(value),
            "--cohort" => options.cohort = Some(std::path::PathBuf::from(value)),
            "--evidence-prefix" => options.evidence_prefix = value.clone(),
            "--refresh-output" => options.refresh_output = Some(std::path::PathBuf::from(value)),
            "--max-attempts" => options.max_attempts = parse(value, flag)?,
            "--start-seed" => options.start_seed = parse(value, flag)?,
            "--run-timeout" => options.run_timeout_seconds = parse(value, flag)?,
            "--export-timeout" => options.export_timeout_seconds = parse(value, flag)?,
            "--repro-timeout" => options.replay_timeout_seconds = parse(value, flag)?,
            "--vms" => options.vms = parse(value, flag)?,
            "--rounds" => options.rounds = parse(value, flag)?,
            "--branches" => options.branches = parse(value, flag)?,
            "--ticks" => options.ticks = parse(value, flag)?,
            "--memory-mb" => options.memory_mib = parse(value, flag)?,
            "--disk-image" => options.disk_image = Some(std::path::PathBuf::from(value)),
            "--workload" => options.workload = value.clone(),
            "--assertion-id" => options.assertion_id = parse(value, flag)?,
            "--cmdline-template" => options.cmdline_template = value.clone(),
            "--fail-after-values" => {
                options.fail_after_values = value
                    .split(',')
                    .filter(|part| !part.is_empty())
                    .map(|part| parse(part, flag))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            _ => return Err(format!("unknown argument: {flag}")),
        }
        index += 2;
    }
    if options.evidence_prefix.is_empty() {
        return Err(String::from("--evidence-prefix is required"));
    }
    if options.max_attempts == 0 || options.fail_after_values.is_empty() {
        return Err(String::from(
            "attempt and fail-after selections must not be empty",
        ));
    }
    Ok(options)
}

fn parse<T: std::str::FromStr>(value: &str, flag: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|error| format!("{flag}: {error}"))
}

fn render_cmdline(template: &str, fail_after: i64, seed: i64, attempt: usize) -> String {
    template
        .replace("{fail_after}", &fail_after.to_string())
        .replace("{seed}", &seed.to_string())
        .replace("{attempt}", &attempt.to_string())
}

fn append_disk(args: &mut Vec<String>, disk: Option<&std::path::Path>) {
    if let Some(path) = disk {
        args.push(String::from("--disk-image"));
        args.push(path.display().to_string());
    }
}

fn load_bugs(root: &std::path::Path) -> Result<Vec<(String, Value)>, String> {
    matching_files(root, "bug_", ".json")?
        .into_iter()
        .map(|path| Ok((file_name(&path)?, load_json(&path)?)))
        .collect()
}

fn matching_files(
    root: &std::path::Path,
    prefix: &str,
    suffix: &str,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths = fs::read_dir(root)
        .map_err(|error| format!("{}: {error}", root.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn load_json(path: &std::path::Path) -> Result<Value, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.len() > MAX_JSON_BYTES {
        return Err(format!("{} exceeds JSON byte bound", path.display()));
    }
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))
}

fn write_json(path: &std::path::Path, value: &Value) -> Result<(), String> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|error| format!("{}: {error}", path.display()))?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn sha256(path: &std::path::Path) -> Result<String, String> {
    let mut reader = BufReader::new(
        fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?,
    );
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn prepare_output(output: &std::path::Path) -> Result<(), String> {
    if output.exists()
        && fs::read_dir(output)
            .map_err(|error| format!("{}: {error}", output.display()))?
            .next()
            .is_some()
    {
        return Err(format!(
            "output directory is not empty: {}",
            output.display()
        ));
    }
    fs::create_dir_all(output).map_err(|error| format!("{}: {error}", output.display()))
}

fn require_kvm() -> Result<(), String> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open("/dev/kvm")
        .map(|_| ())
        .map_err(|_| String::from("/dev/kvm must be readable and writable"))
}

fn absolute(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()
            .map_err(|error| format!("current directory: {error}"))?
            .join(path))
    }
}

fn basename(value: Option<&Value>) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .and_then(|text| std::path::Path::new(text).file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| String::from("artifact path has no UTF-8 basename"))
}

fn file_name(path: &std::path::Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| format!("{} has no UTF-8 file name", path.display()))
}

fn now() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before Unix epoch: {error}"))?
        .as_secs();
    rfc3339_utc(seconds)
}
