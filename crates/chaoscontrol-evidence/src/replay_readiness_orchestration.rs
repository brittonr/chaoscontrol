//! Process and clock shell for replay-readiness orchestration.

// r[impl chaoscontrol.architecture_modules.evidence]
// r[impl chaoscontrol.typed_operator_commands.execution]
// r[impl chaoscontrol.typed_operator_commands.mechanism]
// r[impl chaoscontrol.typed_operator_commands.evidence]

use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bounded_exec::{run, CommandSpec, EnvironmentMode, Input, RunRequest, TerminationScope};

use crate::typed_operator_command::{
    command_identity, completion_name, disposition_name, execution_limits, outcome_policy,
    stdin_bytes, CommandObservation, CommandPlan, EnvironmentMode as PlanEnvironmentMode,
    ExecutableRef, StreamObservation, TerminationScope as PlanTerminationScope,
};
use crate::{ensure, EvidenceError, EvidenceResult};

const HASH_BUFFER_BYTES: usize = 8_192;

/// Execute one admitted command through the pinned bounded process mechanism.
pub(crate) fn run_plan_command(
    plan: &CommandPlan,
    execution_root: &Path,
) -> EvidenceResult<CommandObservation> {
    let executable = PathBuf::from(&plan.executable.path);
    let executable_blake3 = hash_file_bounded(&executable, plan.executable.maximum_bytes)?;
    ensure(
        executable_blake3 == plan.executable.blake3,
        format!(
            "typed command executable identity mismatch: expected={} actual={executable_blake3}",
            plan.executable.blake3
        ),
    )?;
    let execution_root = std::fs::canonicalize(execution_root).map_err(|error| {
        EvidenceError::new(format!(
            "typed command execution root {}: {error}",
            execution_root.display()
        ))
    })?;
    let working_directory = execution_root.join(&plan.working_directory);
    ensure(
        working_directory.is_dir(),
        format!(
            "typed command working directory is not a directory: {}",
            working_directory.display()
        ),
    )?;
    let input_bytes = stdin_bytes(&plan.stdin).map_err(EvidenceError::new)?;
    let input = if matches!(plan.stdin, crate::typed_operator_command::StdinSpec::Null) {
        Input::Null
    } else {
        Input::Bytes(input_bytes)
    };
    let request = RunRequest {
        command: CommandSpec {
            program: executable,
            args: plan.args.iter().map(OsString::from).collect(),
            current_dir: working_directory,
            environment_mode: match plan.environment.mode {
                PlanEnvironmentMode::Clear => EnvironmentMode::Clear,
                PlanEnvironmentMode::Inherit => EnvironmentMode::Inherit,
            },
            environment: plan
                .environment
                .entries
                .iter()
                .map(|entry| (OsString::from(&entry.name), OsString::from(&entry.value)))
                .collect(),
            input,
        },
        limits: execution_limits(plan.limits).map_err(EvidenceError::new)?,
        termination_scope: match plan.termination_scope {
            PlanTerminationScope::Child => TerminationScope::Child,
            PlanTerminationScope::ProcessGroup => TerminationScope::ProcessGroup,
        },
        outcome_policy: outcome_policy(plan).map_err(EvidenceError::new)?,
    };
    let output = run(request)
        .map_err(|error| EvidenceError::new(format!("bounded-exec failed: {error}")))?;
    let command_identity_blake3 = command_identity(plan).map_err(EvidenceError::new)?;
    let completion = completion_name(output.completion);
    Ok(CommandObservation {
        schema: "chaoscontrol.typed-command-observation.v1",
        mechanism_revision: crate::typed_operator_command::MECHANISM_REVISION,
        command_identity_blake3,
        executable_blake3,
        completion,
        disposition: disposition_name(output.disposition),
        exit_code: output.exit_code,
        signal: output.signal,
        cancellation: if completion == "cancelled" {
            "requested"
        } else {
            "not-requested"
        },
        teardown: "completed",
        stdout: stream_observation(&output.stdout),
        stderr: stream_observation(&output.stderr),
    })
}

pub(crate) fn observe_executable_reference(
    path: &Path,
    maximum_bytes: u64,
) -> EvidenceResult<ExecutableRef> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        EvidenceError::new(format!(
            "typed command executable path {}: {error}",
            path.display()
        ))
    })?;
    let blake3 = hash_file_bounded(&canonical, maximum_bytes)?;
    Ok(ExecutableRef {
        path: canonical.display().to_string(),
        blake3,
        maximum_bytes,
    })
}

fn stream_observation(output: &bounded_exec::CapturedOutput) -> StreamObservation {
    StreamObservation {
        observed_bytes: output.observed_bytes,
        retained_bytes: output.bytes.len(),
        retained_blake3: blake3::hash(&output.bytes).to_hex().to_string(),
        truncated: output.truncated,
    }
}

fn hash_file_bounded(path: &Path, maximum_bytes: u64) -> EvidenceResult<String> {
    let mut file = File::open(path).map_err(|error| {
        EvidenceError::new(format!(
            "typed command executable open {}: {error}",
            path.display()
        ))
    })?;
    let metadata = file.metadata().map_err(|error| {
        EvidenceError::new(format!(
            "typed command executable metadata {}: {error}",
            path.display()
        ))
    })?;
    ensure(
        metadata.is_file(),
        format!("typed command executable is not a file: {}", path.display()),
    )?;
    ensure(
        metadata.len() <= maximum_bytes,
        format!(
            "typed command executable exceeds byte bound: size={} maximum={maximum_bytes}",
            metadata.len()
        ),
    )?;
    let mut hasher = blake3::Hasher::new();
    let mut observed_bytes = 0_u64;
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let remaining = maximum_bytes.saturating_sub(observed_bytes);
        let next_read_bound = remaining.saturating_add(1);
        let next_read = usize::try_from(next_read_bound)
            .unwrap_or(usize::MAX)
            .min(HASH_BUFFER_BYTES);
        let read = file.read(&mut buffer[..next_read]).map_err(|error| {
            EvidenceError::new(format!(
                "typed command executable read {}: {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        observed_bytes = observed_bytes
            .checked_add(u64::try_from(read).expect("bounded read length fits u64"))
            .ok_or_else(|| EvidenceError::new("typed command executable byte count overflow"))?;
        ensure(
            observed_bytes <= maximum_bytes,
            format!(
                "typed command executable grew beyond byte bound: observed={observed_bytes} maximum={maximum_bytes}"
            ),
        )?;
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Observe the host wall clock for receipt metadata.
#[allow(unknown_lints)]
#[allow(
    ambient_clock,
    reason = "receipt writer shell timestamps bounded local scheduler evidence"
)]
pub(crate) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
