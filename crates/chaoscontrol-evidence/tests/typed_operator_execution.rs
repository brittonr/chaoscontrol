// r[verify chaoscontrol.typed_operator_commands.execution]
// r[verify chaoscontrol.typed_operator_commands.evidence]
// r[verify chaoscontrol.typed_operator_commands.validation]

use std::path::Path;

#[path = "support/typed_command_fixture.rs"]
mod typed_command_fixture_support;

use chaoscontrol_evidence::execute_typed_operator_command;
use chaoscontrol_evidence::typed_operator_command::{
    CommandPlan, EnvironmentMode, EnvironmentSpec, ExecutableRef, LimitSpec, StdinSpec,
    TerminationScope, MECHANISM_REVISION, PLAN_SCHEMA,
};

const EXECUTABLE_MAX_BYTES: u64 = 67_108_864;
const DEFAULT_TIMEOUT_MS: u64 = 1_000;
const INPUT_MAX_BYTES: u64 = 1_024;
const DEFAULT_OUTPUT_MAX_BYTES: u64 = 4_096;
const POLL_INTERVAL_MS: u64 = 5;
const TEARDOWN_TIMEOUT_MS: u64 = 500;
const ACCEPTED_EXIT_CODE: i32 = 7;
const FLOOD_BYTES: usize = 4_096;
const FLOOD_OUTPUT_MAX_BYTES: u64 = 16;
const TIMEOUT_TEST_MS: u64 = 20;
const SLEEP_TEST_MS: u64 = 1_000;

fn fixture_plan(action: &str, first: &str, second: &str) -> CommandPlan {
    let fixture = typed_command_fixture_support::fixture_spec(action, first, second);
    CommandPlan {
        schema: PLAN_SCHEMA.to_string(),
        mechanism_revision: MECHANISM_REVISION.to_string(),
        executable: ExecutableRef {
            path: fixture.executable.display().to_string(),
            blake3: fixture.executable_blake3,
            maximum_bytes: EXECUTABLE_MAX_BYTES,
        },
        args: fixture.args,
        working_directory: ".".to_string(),
        environment: EnvironmentSpec {
            mode: EnvironmentMode::Clear,
            entries: fixture.environment,
        },
        stdin: StdinSpec::Null,
        limits: LimitSpec {
            timeout_ms: DEFAULT_TIMEOUT_MS,
            stdin_max_bytes: INPUT_MAX_BYTES,
            stdout_max_bytes: DEFAULT_OUTPUT_MAX_BYTES,
            stderr_max_bytes: DEFAULT_OUTPUT_MAX_BYTES,
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

#[test]
fn typed_command_fixture_child() {
    typed_command_fixture_support::run_child();
}

fn execute(
    plan: &CommandPlan,
    root: &Path,
) -> chaoscontrol_evidence::typed_operator_command::CommandObservation {
    execute_typed_operator_command(plan, root).expect("typed command executes")
}

#[test]
fn passes_shell_metacharacters_as_one_literal_argument() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("literal.txt");
    let literal = "; $(touch should-not-exist) | > & *";
    let plan = fixture_plan("write-literal", literal, &output.display().to_string());

    let observation = execute(&plan, temp.path());

    assert_eq!(
        std::fs::read_to_string(output).expect("literal output"),
        literal
    );
    assert_eq!(observation.disposition, "succeeded");
    assert_eq!(observation.mechanism_revision, MECHANISM_REVISION);
    assert_eq!(observation.cancellation, "not-requested");
    assert_eq!(observation.teardown, "completed");
}

#[test]
fn accepts_an_explicit_nonzero_exit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = fixture_plan("exit", &ACCEPTED_EXIT_CODE.to_string(), "");
    plan.accepted_exit_codes = vec![ACCEPTED_EXIT_CODE];

    let observation = execute(&plan, temp.path());

    assert_eq!(observation.exit_code, Some(ACCEPTED_EXIT_CODE));
    assert_eq!(observation.disposition, "succeeded");
}

#[test]
fn output_flood_is_bounded_and_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = fixture_plan("flood", &FLOOD_BYTES.to_string(), "");
    plan.limits.stdout_max_bytes = FLOOD_OUTPUT_MAX_BYTES;

    let observation = execute(&plan, temp.path());

    assert_eq!(observation.disposition, "output-limit-exceeded");
    assert!(observation.stdout.truncated);
    assert_eq!(
        observation.stdout.retained_bytes,
        FLOOD_OUTPUT_MAX_BYTES as usize
    );
}

#[test]
fn timeout_records_owned_teardown() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = fixture_plan("sleep", &SLEEP_TEST_MS.to_string(), "");
    plan.limits.timeout_ms = TIMEOUT_TEST_MS;
    plan.limits.teardown_timeout_ms = TIMEOUT_TEST_MS;
    plan.limits.poll_interval_ms = POLL_INTERVAL_MS;

    let observation = execute(&plan, temp.path());

    assert_eq!(observation.completion, "timed-out");
    assert_eq!(observation.disposition, "timed-out");
    assert_eq!(observation.teardown, "completed");
}

#[cfg(unix)]
#[test]
fn signal_termination_is_not_success() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = fixture_plan("abort", "", "");

    let observation = execute(&plan, temp.path());

    assert_eq!(observation.completion, "exited");
    assert_eq!(observation.disposition, "exit-failed");
    assert!(observation.signal.is_some());
}

#[test]
fn executable_identity_mismatch_fails_before_spawn() {
    let temp = tempfile::tempdir().expect("tempdir");
    let marker = temp.path().join("must-not-exist");
    let mut plan = fixture_plan("write-literal", "unexpected", &marker.display().to_string());
    plan.executable.blake3 =
        "1111111111111111111111111111111111111111111111111111111111111111".to_string();

    let error = execute_typed_operator_command(&plan, temp.path())
        .expect_err("identity mismatch must fail before spawn");

    assert!(error.message().contains("identity mismatch"));
    assert!(!marker.exists());
}
