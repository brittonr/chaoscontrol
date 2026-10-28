//! Pure typed-command admission and evidence DTOs.
//!
//! This module does not read files, inspect environment state, spawn processes,
//! access clocks, or publish output. The orchestration shell supplies those effects.

// r[impl chaoscontrol.typed_operator_commands.plan]
// r[impl chaoscontrol.typed_operator_commands.boundary]
// r[impl chaoscontrol.typed_operator_commands.functional_core]
// r[impl chaoscontrol.typed_operator_commands.legacy]

use std::collections::BTreeSet;
use std::path::{Component, Path};

use bounded_exec::{Completion, Disposition, ExecutionLimits, OutcomePolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLAN_SCHEMA: &str = "chaoscontrol.typed-command-plan.v1";
pub const MECHANISM_REVISION: &str = "29dac88ecded94457572db3fdfaaaab95fa91525";
const BLAKE3_HEX_LENGTH: usize = 64;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_RADIX: u32 = 16;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandPlan {
    pub schema: String,
    pub mechanism_revision: String,
    pub executable: ExecutableRef,
    pub args: Vec<String>,
    pub working_directory: String,
    pub environment: EnvironmentSpec,
    pub stdin: StdinSpec,
    pub limits: LimitSpec,
    pub accepted_exit_codes: Vec<i32>,
    pub reject_stdout_truncation: bool,
    pub reject_stderr_truncation: bool,
    pub termination_scope: TerminationScope,
    pub evidence_eligible: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutableRef {
    pub path: String,
    pub blake3: String,
    pub maximum_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSpec {
    pub mode: EnvironmentMode,
    pub entries: Vec<EnvironmentEntry>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentMode {
    Clear,
    Inherit,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentEntry {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum StdinSpec {
    Null,
    Bytes { hex: String, blake3: String },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LimitSpec {
    pub timeout_ms: u64,
    pub stdin_max_bytes: u64,
    pub stdout_max_bytes: u64,
    pub stderr_max_bytes: u64,
    pub poll_interval_ms: u64,
    pub teardown_timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TerminationScope {
    Child,
    ProcessGroup,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommandObservation {
    pub schema: &'static str,
    pub mechanism_revision: &'static str,
    pub command_identity_blake3: String,
    pub executable_blake3: String,
    pub completion: &'static str,
    pub disposition: &'static str,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub cancellation: &'static str,
    pub teardown: &'static str,
    pub stdout: StreamObservation,
    pub stderr: StreamObservation,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StreamObservation {
    pub observed_bytes: usize,
    pub retained_bytes: usize,
    pub retained_blake3: String,
    pub truncated: bool,
}

pub fn parse_plan(value: &Value) -> Result<CommandPlan, String> {
    if value.is_string() {
        return Err(String::from(
            "legacy free-form command text cannot execute; provide a typed command plan",
        ));
    }
    let plan: CommandPlan = serde_json::from_value(value.clone())
        .map_err(|error| format!("typed command plan decode failed: {error}"))?;
    validate_plan(&plan)?;
    Ok(plan)
}

/// Maximum number of argv entries admitted for one typed command.
pub const MAX_COMMAND_ARGUMENTS: usize = 4_096;
/// Maximum total bytes across all argv entries.
pub const MAX_COMMAND_ARGUMENT_BYTES: usize = 1024 * 1024;
/// Maximum number of environment entries admitted for one typed command.
pub const MAX_ENVIRONMENT_ENTRIES: usize = 1_024;
/// Maximum total bytes across environment names and values.
pub const MAX_ENVIRONMENT_BYTES: usize = 1024 * 1024;
/// Maximum number of accepted exit codes.
pub const MAX_ACCEPTED_EXIT_CODES: usize = 256;

pub fn validate_plan(plan: &CommandPlan) -> Result<(), String> {
    require(
        plan.schema == PLAN_SCHEMA,
        "typed command plan schema is unsupported",
    )?;
    require(
        plan.mechanism_revision == MECHANISM_REVISION,
        "bounded-exec mechanism revision drift",
    )?;
    require(!plan.executable.path.is_empty(), "executable path is empty")?;
    require(
        !plan.executable.path.contains('\0'),
        "executable path contains NUL",
    )?;
    require(
        Path::new(&plan.executable.path).is_absolute(),
        "executable path must be absolute",
    )?;
    require(
        valid_blake3(&plan.executable.blake3),
        "executable BLAKE3 identity is invalid",
    )?;
    require(
        plan.executable.maximum_bytes > 0,
        "executable byte bound must be positive",
    )?;
    validate_relative_directory(&plan.working_directory)?;
    require(
        plan.environment.mode == EnvironmentMode::Clear,
        "evidence command environment mode must be clear",
    )?;
    require(
        plan.environment.entries.len() <= MAX_ENVIRONMENT_ENTRIES,
        "environment entry count exceeds the bound",
    )?;
    let mut environment_bytes = 0_usize;
    let mut environment_names = BTreeSet::new();
    for entry in &plan.environment.entries {
        environment_bytes = environment_bytes
            .checked_add(entry.name.len())
            .and_then(|total| total.checked_add(entry.value.len()))
            .ok_or_else(|| String::from("environment byte count overflow"))?;
        require(
            environment_bytes <= MAX_ENVIRONMENT_BYTES,
            "environment bytes exceed the bound",
        )?;
        require(
            !entry.name.is_empty() && !entry.name.contains(['\0', '=']),
            "environment name is invalid",
        )?;
        require(
            !entry.value.contains('\0'),
            "environment value contains NUL",
        )?;
        require(
            environment_names.insert(entry.name.as_str()),
            "environment contains a duplicate name",
        )?;
    }
    require(
        plan.args.len() <= MAX_COMMAND_ARGUMENTS,
        "argument count exceeds the bound",
    )?;
    let mut argument_bytes = 0_usize;
    for argument in &plan.args {
        require(!argument.contains('\0'), "argument contains NUL")?;
        argument_bytes = argument_bytes
            .checked_add(argument.len())
            .ok_or_else(|| String::from("argument byte count overflow"))?;
        require(
            argument_bytes <= MAX_COMMAND_ARGUMENT_BYTES,
            "argument bytes exceed the bound",
        )?;
    }
    let limits = execution_limits(plan.limits)?;
    limits
        .validate()
        .map_err(|error| format!("invalid command limits: {error:?}"))?;
    if let StdinSpec::Bytes { hex, .. } = &plan.stdin {
        require(
            hex.len() / HEX_CHARACTERS_PER_BYTE <= limits.stdin_max_bytes,
            "stdin exceeds its admitted byte bound",
        )?;
    }
    let input = stdin_bytes(&plan.stdin)?;
    require(
        input.len() <= limits.stdin_max_bytes,
        "stdin exceeds its admitted byte bound",
    )?;
    require(
        plan.accepted_exit_codes.len() <= MAX_ACCEPTED_EXIT_CODES,
        "accepted exit code count exceeds the bound",
    )?;
    OutcomePolicy::new(
        plan.accepted_exit_codes.clone(),
        plan.reject_stdout_truncation,
        plan.reject_stderr_truncation,
    )
    .map_err(|error| format!("invalid accepted-exit policy: {error:?}"))?;
    if plan.evidence_eligible {
        require(
            plan.termination_scope == TerminationScope::ProcessGroup,
            "evidence-eligible commands require process-group teardown",
        )?;
    }
    Ok(())
}

pub fn execution_limits(spec: LimitSpec) -> Result<ExecutionLimits, String> {
    Ok(ExecutionLimits {
        timeout_ms: spec.timeout_ms,
        stdin_max_bytes: usize::try_from(spec.stdin_max_bytes)
            .map_err(|_| String::from("stdin byte bound does not fit this platform"))?,
        stdout_max_bytes: usize::try_from(spec.stdout_max_bytes)
            .map_err(|_| String::from("stdout byte bound does not fit this platform"))?,
        stderr_max_bytes: usize::try_from(spec.stderr_max_bytes)
            .map_err(|_| String::from("stderr byte bound does not fit this platform"))?,
        poll_interval_ms: spec.poll_interval_ms,
        teardown_timeout_ms: spec.teardown_timeout_ms,
    })
}

pub fn outcome_policy(plan: &CommandPlan) -> Result<OutcomePolicy, String> {
    require(
        plan.accepted_exit_codes.len() <= MAX_ACCEPTED_EXIT_CODES,
        "accepted exit code count exceeds the bound",
    )?;
    OutcomePolicy::new(
        plan.accepted_exit_codes.clone(),
        plan.reject_stdout_truncation,
        plan.reject_stderr_truncation,
    )
    .map_err(|error| format!("invalid accepted-exit policy: {error:?}"))
}

pub fn stdin_bytes(spec: &StdinSpec) -> Result<Vec<u8>, String> {
    match spec {
        StdinSpec::Null => Ok(Vec::new()),
        StdinSpec::Bytes { hex, blake3 } => {
            require(valid_blake3(blake3), "stdin BLAKE3 identity is invalid")?;
            require(
                hex.len() % HEX_CHARACTERS_PER_BYTE == 0,
                "stdin hex has an odd length",
            )?;
            let bytes = hex
                .as_bytes()
                .as_chunks::<HEX_CHARACTERS_PER_BYTE>()
                .0
                .iter()
                .map(|pair| {
                    let text = std::str::from_utf8(pair)
                        .map_err(|_| String::from("stdin hex is not UTF-8"))?;
                    u8::from_str_radix(text, HEX_RADIX)
                        .map_err(|_| String::from("stdin hex contains a non-hex byte"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let actual = blake3::hash(&bytes).to_hex().to_string();
            require(actual == *blake3, "stdin BLAKE3 identity mismatch")?;
            Ok(bytes)
        }
    }
}

pub fn command_identity(plan: &CommandPlan) -> Result<String, String> {
    let bytes = serde_json::to_vec(plan)
        .map_err(|error| format!("typed command identity serialization failed: {error}"))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Render a compatibility-only command display string.
///
/// This value is not executable and does not preserve an argument parser.
#[must_use]
pub fn command_display(plan: &CommandPlan) -> String {
    std::iter::once(plan.executable.path.as_str())
        .chain(plan.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn completion_name(completion: Completion) -> &'static str {
    match completion {
        Completion::Exited => "exited",
        Completion::TimedOut => "timed-out",
        Completion::Cancelled => "cancelled",
    }
}

pub fn disposition_name(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::Succeeded => "succeeded",
        Disposition::ExitFailed => "exit-failed",
        Disposition::TimedOut => "timed-out",
        Disposition::Cancelled => "cancelled",
        Disposition::OutputLimitExceeded(_) => "output-limit-exceeded",
    }
}

fn validate_relative_directory(value: &str) -> Result<(), String> {
    require(!value.is_empty(), "working directory is empty")?;
    require(!value.contains('\0'), "working directory contains NUL")?;
    let mut saw_component = false;
    for component in Path::new(value).components() {
        match component {
            Component::Normal(_) | Component::CurDir => saw_component = true,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(String::from(
                    "working directory must be capability-relative without traversal",
                ));
            }
        }
    }
    require(saw_component, "working directory is empty")
}

fn valid_blake3(value: &str) -> bool {
    value.len() == BLAKE3_HEX_LENGTH && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(String::from(message))
    }
}

#[cfg(test)]
mod tests;
