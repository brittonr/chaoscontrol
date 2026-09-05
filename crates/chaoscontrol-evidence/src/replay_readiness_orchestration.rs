//! Process and clock shell for replay-readiness orchestration.

// r[impl chaoscontrol.architecture_modules.evidence]
// r[impl chaoscontrol.typed_operator_commands.execution]
// r[impl chaoscontrol.typed_operator_commands.mechanism]
// r[impl chaoscontrol.typed_operator_commands.evidence]

use std::io::Read;

const HASH_BUFFER_BYTES: usize = 8_192;

/// Captured process data and its structured observation.
#[derive(Debug)]
pub struct CapturedPlanCommand {
    pub observation: crate::typed_operator_command::CommandObservation,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Execute one admitted command through the pinned bounded process mechanism.
pub(crate) fn run_plan_command(
    plan: &crate::typed_operator_command::CommandPlan,
    execution_root: &std::path::Path,
) -> crate::EvidenceResult<crate::typed_operator_command::CommandObservation> {
    Ok(run_plan_command_captured(plan, execution_root)?.observation)
}

/// Execute one admitted command and retain its bounded output for a Rust shell.
pub fn run_plan_command_captured(
    plan: &crate::typed_operator_command::CommandPlan,
    execution_root: &std::path::Path,
) -> crate::EvidenceResult<CapturedPlanCommand> {
    let executable = std::path::PathBuf::from(&plan.executable.path);
    let executable_blake3 = hash_file_bounded(&executable, plan.executable.maximum_bytes)?;
    crate::ensure(
        executable_blake3 == plan.executable.blake3,
        format!(
            "typed command executable identity mismatch: expected={} actual={executable_blake3}",
            plan.executable.blake3
        ),
    )?;
    let execution_root = std::fs::canonicalize(execution_root).map_err(|error| {
        crate::EvidenceError::new(format!(
            "typed command execution root {}: {error}",
            execution_root.display()
        ))
    })?;
    let working_directory = execution_root.join(&plan.working_directory);
    crate::ensure(
        working_directory.is_dir(),
        format!(
            "typed command working directory is not a directory: {}",
            working_directory.display()
        ),
    )?;
    let input_bytes = crate::typed_operator_command::stdin_bytes(&plan.stdin)
        .map_err(crate::EvidenceError::new)?;
    let input = if matches!(plan.stdin, crate::typed_operator_command::StdinSpec::Null) {
        ::bounded_exec::Input::Null
    } else {
        ::bounded_exec::Input::Bytes(input_bytes)
    };
    let request = ::bounded_exec::RunRequest {
        command: ::bounded_exec::CommandSpec {
            program: executable,
            args: plan.args.iter().map(::std::ffi::OsString::from).collect(),
            current_dir: working_directory,
            environment_mode: match plan.environment.mode {
                crate::typed_operator_command::EnvironmentMode::Clear => {
                    ::bounded_exec::EnvironmentMode::Clear
                }
                crate::typed_operator_command::EnvironmentMode::Inherit => {
                    ::bounded_exec::EnvironmentMode::Inherit
                }
            },
            environment: plan
                .environment
                .entries
                .iter()
                .map(|entry| {
                    (
                        ::std::ffi::OsString::from(&entry.name),
                        ::std::ffi::OsString::from(&entry.value),
                    )
                })
                .collect(),
            input,
        },
        limits: crate::typed_operator_command::execution_limits(plan.limits)
            .map_err(crate::EvidenceError::new)?,
        termination_scope: match plan.termination_scope {
            crate::typed_operator_command::TerminationScope::Child => {
                ::bounded_exec::TerminationScope::Child
            }
            crate::typed_operator_command::TerminationScope::ProcessGroup => {
                ::bounded_exec::TerminationScope::ProcessGroup
            }
        },
        outcome_policy: crate::typed_operator_command::outcome_policy(plan)
            .map_err(crate::EvidenceError::new)?,
    };
    let output = ::bounded_exec::run(request)
        .map_err(|error| crate::EvidenceError::new(format!("bounded-exec failed: {error}")))?;
    let command_identity_blake3 =
        crate::typed_operator_command::command_identity(plan).map_err(crate::EvidenceError::new)?;
    let completion = crate::typed_operator_command::completion_name(output.completion);
    let observation = crate::typed_operator_command::CommandObservation {
        schema: "chaoscontrol.typed-command-observation.v1",
        mechanism_revision: crate::typed_operator_command::MECHANISM_REVISION,
        command_identity_blake3,
        executable_blake3,
        completion,
        disposition: crate::typed_operator_command::disposition_name(output.disposition),
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
    };
    Ok(CapturedPlanCommand {
        observation,
        stdout: output.stdout.bytes,
        stderr: output.stderr.bytes,
    })
}

pub fn observe_executable_reference(
    path: &std::path::Path,
    maximum_bytes: u64,
) -> crate::EvidenceResult<crate::typed_operator_command::ExecutableRef> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        crate::EvidenceError::new(format!(
            "typed command executable path {}: {error}",
            path.display()
        ))
    })?;
    let blake3 = hash_file_bounded(&canonical, maximum_bytes)?;
    Ok(crate::typed_operator_command::ExecutableRef {
        path: canonical.display().to_string(),
        blake3,
        maximum_bytes,
    })
}

fn stream_observation(
    output: &bounded_exec::CapturedOutput,
) -> crate::typed_operator_command::StreamObservation {
    crate::typed_operator_command::StreamObservation {
        observed_bytes: output.observed_bytes,
        retained_bytes: output.bytes.len(),
        retained_blake3: blake3::hash(&output.bytes).to_hex().to_string(),
        truncated: output.truncated,
    }
}

fn hash_file_bounded(path: &std::path::Path, maximum_bytes: u64) -> crate::EvidenceResult<String> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        crate::EvidenceError::new(format!(
            "typed command executable open {}: {error}",
            path.display()
        ))
    })?;
    let metadata = file.metadata().map_err(|error| {
        crate::EvidenceError::new(format!(
            "typed command executable metadata {}: {error}",
            path.display()
        ))
    })?;
    crate::ensure(
        metadata.is_file(),
        format!("typed command executable is not a file: {}", path.display()),
    )?;
    crate::ensure(
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
            crate::EvidenceError::new(format!(
                "typed command executable read {}: {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        observed_bytes = observed_bytes
            .checked_add(u64::try_from(read).expect("bounded read length fits u64"))
            .ok_or_else(|| {
                crate::EvidenceError::new("typed command executable byte count overflow")
            })?;
        crate::ensure(
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
    ::std::time::SystemTime::now()
        .duration_since(::std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
