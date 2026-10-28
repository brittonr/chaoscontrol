use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use chaoscontrol_evidence::rust_automation::local_kvm::{campaign_plan, parse_workloads};
use chaoscontrol_evidence::rust_automation::time::rfc3339_utc;
use chaoscontrol_evidence::typed_operator_command::{
    CommandPlan, EnvironmentEntry, EnvironmentMode, EnvironmentSpec, LimitSpec, StdinSpec,
    TerminationScope, MECHANISM_REVISION, PLAN_SCHEMA,
};
use chaoscontrol_evidence::{execute_typed_operator_command_captured, observe_typed_executable};
use serde_json::{json, Value};

const DEFAULT_OUT: &str = "dogfood-results/local-multi-hypervisor-kvm-smoke-latest";
const DEFAULT_WORKLOADS: &str = "raft,rust-workload";
const DEFAULT_REPLAY: &str = "replay-readiness";
const DEFAULT_SCHEDULER: &str = "replay-readiness-scheduler-receipt";
const EXECUTABLE_MAX_BYTES: u64 = 1_073_741_824;
const ORCHESTRATOR_TIMEOUT_MS: u64 = 900_000;
const STDIN_MAX_BYTES: u64 = 1_024;
const OUTPUT_MAX_BYTES: u64 = 8_388_608;
const MAX_RECEIPT_BYTES: u64 = 64 * 1024 * 1024;
const POLL_INTERVAL_MS: u64 = 10;
const TEARDOWN_TIMEOUT_MS: u64 = 1_000;
const USAGE_EXIT: u8 = 2;

#[derive(Debug)]
struct Options {
    out: PathBuf,
    workloads: Vec<String>,
    replay_readiness: PathBuf,
    scheduler: PathBuf,
    dogfood_extra: Vec<String>,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(USAGE_EXIT)
        }
    }
}

fn run(args: Vec<String>) -> Result<i32, String> {
    require_kvm()?;
    let options = parse_args(&args)?;
    let out = absolute(&options.out)?;
    prepare_output(&out)?;
    let started_at = now()?;
    let replay_executable =
        observe_typed_executable(&options.replay_readiness, EXECUTABLE_MAX_BYTES)
            .map_err(|error| error.to_string())?;
    let scheduler_executable = observe_typed_executable(&options.scheduler, EXECUTABLE_MAX_BYTES)
        .map_err(|error| error.to_string())?;
    let command_plans = options
        .workloads
        .iter()
        .enumerate()
        .map(|(index, workload)| {
            let number = index + 1;
            let receipt = out
                .join("run-receipts")
                .join(format!("{number:02}-{workload}-replay-readiness.json"));
            let mut args = vec![
                String::from("--receipt"),
                receipt.display().to_string(),
                String::from("--dogfood"),
                workload.clone(),
                String::from("--"),
                String::from("--output"),
                out.join("dogfood")
                    .join(format!("{number:02}-{workload}"))
                    .display()
                    .to_string(),
            ];
            args.extend(options.dogfood_extra.clone());
            // The enclosing campaign still bounds the complete proof sequence.
            let plan = command_plan(
                replay_executable.clone(),
                args,
                ORCHESTRATOR_TIMEOUT_MS,
                vec![0],
            );
            let value = serde_json::to_value(&plan).map_err(|error| error.to_string())?;
            write_json(&out.join(format!("command-plan-{number:02}.json")), &value)?;
            Ok(value)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let plan_path = out.join("campaign-plan.json");
    let receipt_path = out.join("campaign-receipt.json");
    let plan = campaign_plan(&out, &options.workloads, &command_plans)?;
    write_json(&plan_path, &plan)?;

    let scheduler_plan = command_plan(
        scheduler_executable,
        vec![
            String::from("--run-multi-hypervisor-plan"),
            plan_path.display().to_string(),
            String::from("--output"),
            receipt_path.display().to_string(),
        ],
        ORCHESTRATOR_TIMEOUT_MS,
        vec![0, 1, 2],
    );
    let execution_root =
        env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let captured = execute_typed_operator_command_captured(&scheduler_plan, &execution_root)
        .map_err(|error| error.to_string())?;
    let scheduler_exit_code = captured.observation.exit_code.unwrap_or(1);
    let exit_code = if scheduler_exit_code == 0 && captured.observation.disposition == "succeeded" {
        let bytes = read_bounded(&receipt_path, MAX_RECEIPT_BYTES)?;
        let receipt = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{}: {error}", receipt_path.display()))?;
        campaign_exit_code(&receipt)?
    } else {
        scheduler_exit_code.max(1)
    };
    let stdout = String::from_utf8_lossy(&captured.stdout);
    let stderr = String::from_utf8_lossy(&captured.stderr);
    let command_output = format!("{stdout}{stderr}");
    let summary = command_output.lines().last().unwrap_or("");
    write_summary(
        &out,
        summary,
        &receipt_path,
        &plan_path,
        exit_code,
        &command_output,
    )?;
    write_json(
        &out.join("metadata.json"),
        &json!({
            "schema_version": 1,
            "command": "local-multi-hypervisor-kvm-smoke",
            "started_at": started_at,
            "finished_at": now()?,
            "exit_code": exit_code,
            "scheduler_exit_code": scheduler_exit_code,
            "scheduler_observation": captured.observation,
            "workloads": options.workloads,
            "artifacts": {
                "plan": plan_path,
                "receipt": receipt_path,
                "queue_state": out.join("campaign-state.json"),
                "summary": out.join("summary.txt"),
            },
            "scope": "bounded local KVM multi-hypervisor campaign smoke only; not a hosted service, not a shared remote queue, not cross-machine scheduling, not fleet-scale throughput",
        }),
    )?;
    print!("{command_output}");
    if exit_code == 0 {
        println!(
            "local multi-hypervisor KVM smoke artifacts: {}",
            out.display()
        );
    } else {
        eprintln!(
            "local multi-hypervisor KVM smoke failed; artifacts: {}",
            out.display()
        );
    }
    Ok(exit_code)
}

fn campaign_exit_code(receipt: &Value) -> Result<i32, String> {
    chaoscontrol_evidence::validate_replay_readiness_multi_hypervisor_campaign_receipt(receipt)
        .map_err(|error| error.to_string())?;
    let passed = receipt["status"] == "recorded"
        && receipt["runs"].as_array().is_some_and(|runs| {
            runs.iter().all(|run| {
                run["status"] == "passed"
                    && run["exit_code"] == 0
                    && run["receipt_summary"].as_str().is_some_and(|summary| {
                        summary.starts_with("replay-readiness status=passed ")
                    })
            })
        });
    Ok(if passed { 0 } else { 1 })
}

fn command_plan(
    executable: chaoscontrol_evidence::typed_operator_command::ExecutableRef,
    args: Vec<String>,
    timeout_ms: u64,
    accepted_exit_codes: Vec<i32>,
) -> CommandPlan {
    CommandPlan {
        schema: String::from(PLAN_SCHEMA),
        mechanism_revision: String::from(MECHANISM_REVISION),
        executable,
        args,
        working_directory: String::from("."),
        environment: EnvironmentSpec {
            mode: EnvironmentMode::Clear,
            entries: vec![EnvironmentEntry {
                name: String::from("CHAOSCONTROL_WATCHDOG_US"),
                value: String::from("5000000"),
            }],
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
        accepted_exit_codes,
        reject_stdout_truncation: true,
        reject_stderr_truncation: true,
        termination_scope: TerminationScope::ProcessGroup,
        evidence_eligible: true,
    }
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut workloads = String::from(DEFAULT_WORKLOADS);
    let mut replay = env::var("REPLAY_READINESS").unwrap_or_else(|_| String::from(DEFAULT_REPLAY));
    let mut scheduler = env::var("REPLAY_READINESS_SCHEDULER_RECEIPT")
        .unwrap_or_else(|_| String::from(DEFAULT_SCHEDULER));
    let mut dogfood_extra = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "--workloads" | "--replay-readiness" | "--scheduler-receipt" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{} requires a value", args[index]))?;
                match args[index].as_str() {
                    "--out" => out = PathBuf::from(value),
                    "--workloads" => workloads = value.clone(),
                    "--replay-readiness" => replay = value.clone(),
                    _ => scheduler = value.clone(),
                }
                index += 2;
            }
            "--" => {
                dogfood_extra.extend_from_slice(&args[index + 1..]);
                break;
            }
            value => {
                dogfood_extra.push(value.to_string());
                index += 1;
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(String::from("--out must not be empty"));
    }
    let workloads = parse_workloads(&workloads)?;
    Ok(Options {
        out,
        workloads,
        replay_readiness: PathBuf::from(replay),
        scheduler: PathBuf::from(scheduler),
        dogfood_extra,
    })
}

fn require_kvm() -> Result<(), String> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open("/dev/kvm")
        .map(|_| ())
        .map_err(|_| {
            String::from("local multi-hypervisor KVM smoke requires read/write access to /dev/kvm")
        })
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()
            .map_err(|error| format!("current directory: {error}"))?
            .join(path))
    }
}

fn prepare_output(out: &Path) -> Result<(), String> {
    if out.exists()
        && fs::read_dir(out)
            .map_err(|error| format!("{}: {error}", out.display()))?
            .next()
            .is_some()
    {
        return Err(format!("output directory is not empty: {}", out.display()));
    }
    fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
    fs::create_dir(out.join("run-receipts"))
        .map_err(|error| format!("{}: {error}", out.display()))?;
    fs::create_dir(out.join("dogfood")).map_err(|error| format!("{}: {error}", out.display()))
}

fn write_summary(
    out: &Path,
    summary: &str,
    receipt: &Path,
    plan: &Path,
    exit_code: i32,
    command_output: &str,
) -> Result<(), String> {
    let status = if exit_code == 0 { "passed" } else { "failed" };
    let summary = if summary.trim().is_empty() {
        "<none>"
    } else {
        summary.trim()
    };
    let text = format!(
        "local multi-hypervisor KVM smoke\nstatus={status}\nsummary={summary}\nplan={}\nreceipt={}\nqueue_state={}\nscope=bounded-local-kvm-multi-hypervisor-not-hosted-not-shared-remote-queue-not-cross-machine\nraw_log_scraping=false\n\nrunner output:\n{}\n",
        plan.display(), receipt.display(), out.join("campaign-state.json").display(), command_output.trim()
    );
    write_atomic(&out.join("summary.txt"), text.as_bytes())
        .map_err(|error| format!("{}: {error}", out.join("summary.txt").display()))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{} is not a bounded regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = chaoscontrol_evidence::bounded_file::output_parent(path);
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("{}: encode failed: {error}", path.display()))?;
    bytes.push(b'\n');
    write_atomic(path, &bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn now() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before Unix epoch: {error}"))?
        .as_secs();
    rfc3339_utc(seconds)
}

#[cfg(test)]
mod tests {
    use chaoscontrol_evidence::sample_replay_readiness_multi_hypervisor_campaign_receipt;
    use serde_json::json;

    use super::{campaign_exit_code, parse_args};

    #[test]
    fn empty_output_directory_is_rejected() {
        let args = vec!["--out".to_string(), String::new()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn recorded_campaign_requires_every_workload_report_to_pass() {
        let mut receipt = sample_replay_readiness_multi_hypervisor_campaign_receipt();
        assert_eq!(campaign_exit_code(&receipt).unwrap(), 0);

        receipt["runs"][0]["receipt_summary"] =
            json!("replay-readiness status=failed dogfood=raft:fail scope=bounded");
        assert_eq!(campaign_exit_code(&receipt).unwrap(), 1);
    }

    #[test]
    fn recorded_label_cannot_mask_a_failed_run() {
        let mut receipt = sample_replay_readiness_multi_hypervisor_campaign_receipt();
        receipt["runs"][0]["status"] = json!("failed");
        receipt["runs"][0]["exit_code"] = json!(124);
        receipt["runs"][0]["receipt_summary"] = json!(null);
        assert_eq!(campaign_exit_code(&receipt).unwrap(), 1);

        for status in ["partial", "failed"] {
            receipt["status"] = json!(status);
            assert_eq!(campaign_exit_code(&receipt).unwrap(), 1);
        }
    }

    #[test]
    fn incomplete_receipt_cannot_pass_as_recorded() {
        assert!(campaign_exit_code(&json!({"status": "recorded"})).is_err());
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn receipt_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, MAX_RECEIPT_BYTES).is_err());

        let fifo = root.path().join("receipt.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, MAX_RECEIPT_BYTES).is_err());
    }

    #[test]
    fn atomic_output_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomic(&link, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&link).unwrap(), b"replacement");
    }
}
