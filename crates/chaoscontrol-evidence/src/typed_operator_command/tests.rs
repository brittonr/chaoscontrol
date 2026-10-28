// r[verify chaoscontrol.typed_operator_commands.validation]

use super::*;

const DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const TIMEOUT_MS: u64 = 1_000;
const INPUT_BYTES: u64 = 64;
const OUTPUT_BYTES: u64 = 4_096;
const POLL_MS: u64 = 10;
const TEARDOWN_MS: u64 = 500;

fn valid_plan() -> CommandPlan {
    CommandPlan {
        schema: String::from(PLAN_SCHEMA),
        mechanism_revision: String::from(MECHANISM_REVISION),
        executable: ExecutableRef {
            path: String::from("/nix/store/example/bin/tool"),
            blake3: String::from(DIGEST),
            maximum_bytes: OUTPUT_BYTES,
        },
        args: vec![String::from("literal;not-shell")],
        working_directory: String::from("."),
        environment: EnvironmentSpec {
            mode: EnvironmentMode::Clear,
            entries: Vec::new(),
        },
        stdin: StdinSpec::Null,
        limits: LimitSpec {
            timeout_ms: TIMEOUT_MS,
            stdin_max_bytes: INPUT_BYTES,
            stdout_max_bytes: OUTPUT_BYTES,
            stderr_max_bytes: OUTPUT_BYTES,
            poll_interval_ms: POLL_MS,
            teardown_timeout_ms: TEARDOWN_MS,
        },
        accepted_exit_codes: vec![0],
        reject_stdout_truncation: true,
        reject_stderr_truncation: true,
        termination_scope: TerminationScope::ProcessGroup,
        evidence_eligible: true,
    }
}

#[test]
fn complete_plan_is_deterministic() {
    let plan = valid_plan();
    assert_eq!(validate_plan(&plan), Ok(()));
    assert_eq!(command_identity(&plan), command_identity(&plan));
    assert_eq!(
        command_display(&plan),
        "/nix/store/example/bin/tool literal;not-shell"
    );
}

#[test]
fn legacy_traversal_environment_identity_and_limit_fail_closed() {
    assert!(parse_plan(&Value::String(String::from("tool --flag")))
        .unwrap_err()
        .contains("legacy"));

    let mut traversal = valid_plan();
    traversal.working_directory = String::from("../escape");
    assert!(validate_plan(&traversal).unwrap_err().contains("traversal"));

    let mut ambient = valid_plan();
    ambient.environment.mode = EnvironmentMode::Inherit;
    assert!(validate_plan(&ambient).unwrap_err().contains("clear"));

    let mut missing_identity = valid_plan();
    missing_identity.executable.blake3.clear();
    assert!(validate_plan(&missing_identity)
        .unwrap_err()
        .contains("identity"));

    let mut zero_timeout = valid_plan();
    zero_timeout.limits.timeout_ms = 0;
    assert!(validate_plan(&zero_timeout).unwrap_err().contains("limits"));

    let mut nul_executable = valid_plan();
    nul_executable.executable.path.push('\0');
    assert!(validate_plan(&nul_executable)
        .unwrap_err()
        .contains("executable path contains NUL"));

    let mut nul_working_directory = valid_plan();
    nul_working_directory.working_directory = String::from("a\0b");
    assert!(validate_plan(&nul_working_directory)
        .unwrap_err()
        .contains("working directory contains NUL"));

    let mut too_many_arguments = valid_plan();
    too_many_arguments.args = vec![String::new(); MAX_COMMAND_ARGUMENTS + 1];
    assert!(validate_plan(&too_many_arguments)
        .unwrap_err()
        .contains("argument count"));

    let mut oversized_arguments = valid_plan();
    oversized_arguments.args = vec![String::from("x").repeat(MAX_COMMAND_ARGUMENT_BYTES + 1)];
    assert!(validate_plan(&oversized_arguments)
        .unwrap_err()
        .contains("argument bytes"));

    let mut too_many_environment_entries = valid_plan();
    too_many_environment_entries.environment.entries = (0..=MAX_ENVIRONMENT_ENTRIES)
        .map(|index| EnvironmentEntry {
            name: format!("K{index}"),
            value: String::new(),
        })
        .collect();
    assert!(validate_plan(&too_many_environment_entries)
        .unwrap_err()
        .contains("environment entry count"));
}

#[test]
fn oversized_stdin_hex_is_rejected_before_decoding() {
    let mut plan = valid_plan();
    plan.stdin = StdinSpec::Bytes {
        hex: "00".repeat(usize::try_from(INPUT_BYTES).unwrap() + 1),
        blake3: String::from(DIGEST),
    };
    assert!(validate_plan(&plan)
        .unwrap_err()
        .contains("stdin exceeds its admitted byte bound"));
}

#[test]
fn explicit_stdin_requires_matching_identity() {
    let bytes = b"input";
    let digest = blake3::hash(bytes).to_hex().to_string();
    let spec = StdinSpec::Bytes {
        hex: String::from("696e707574"),
        blake3: digest,
    };
    assert_eq!(stdin_bytes(&spec), Ok(bytes.to_vec()));

    let wrong = StdinSpec::Bytes {
        hex: String::from("696e707574"),
        blake3: String::from(DIGEST),
    };
    assert!(stdin_bytes(&wrong).unwrap_err().contains("mismatch"));
}

#[test]
fn cancellation_teardown_and_overclaim_cases_fail_closed() {
    assert_eq!(completion_name(Completion::Cancelled), "cancelled");
    assert_eq!(disposition_name(Disposition::Cancelled), "cancelled");

    let mut child_only = valid_plan();
    child_only.termination_scope = TerminationScope::Child;
    assert!(validate_plan(&child_only)
        .unwrap_err()
        .contains("process-group teardown"));

    let mut missing_teardown = valid_plan();
    missing_teardown.limits.teardown_timeout_ms = 0;
    assert!(validate_plan(&missing_teardown)
        .unwrap_err()
        .contains("TeardownTimeoutZero"));

    let mut duplicate_exit = valid_plan();
    duplicate_exit.accepted_exit_codes = vec![0, 0];
    assert!(validate_plan(&duplicate_exit)
        .unwrap_err()
        .contains("AcceptedExitCodeDuplicate"));
}

#[test]
fn execution_shell_has_no_command_interpreter_path() {
    let source = include_str!("../replay_readiness_orchestration.rs");
    for forbidden in [
        "std::process::Command",
        "Command::new",
        "sh -c",
        ".arg(\"-c\")",
    ] {
        assert!(
            !source.contains(forbidden),
            "execution shell contains forbidden interpreter path {forbidden:?}"
        );
    }
}
