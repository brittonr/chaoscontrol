//! Guest diagnostic request contract.
//!
//! One bounded host-to-supervisor diagnostic channel. Versions 1/2 carry a
//! legacy diagnostic with wall-time watchdog; version 3 carries an exact declared
//! observation profile with relative paths, explicit environment, virtual-tick
//! deadline, and output bounds. The guest independently admits that profile.
//! This pure contract executes nothing and does not certify output correctness.

// r[impl chaoscontrol.guest_diagnostic.request_admission]
/// Legacy diagnostic encoding version with bounded stdin.
pub const DIAGNOSTIC_PROTOCOL_VERSION: u16 = 2;

/// Legacy diagnostic request version without stdin.
pub const DIAGNOSTIC_PROTOCOL_VERSION_V1: u16 = 1;

/// Declared observation requests carry a canonical profile in `stdin`.
pub const OBSERVATION_PROTOCOL_VERSION: u16 = 3;

/// Maximum encoded stdin bytes for version 2 requests.
pub const MAX_DIAGNOSTIC_STDIN_BYTES: usize = 4096;

/// Maximum number of arguments, excluding the executable.
pub const MAX_DIAGNOSTIC_ARGS: usize = 16;

/// Maximum encoded length of one argument.
pub const MAX_DIAGNOSTIC_ARG_BYTES: usize = 1024;

/// Maximum encoded length of all arguments together.
pub const MAX_DIAGNOSTIC_TOTAL_ARG_BYTES: usize = 4096;

/// Largest captured byte count for each diagnostic output stream.
pub const MAX_DIAGNOSTIC_CAPTURE_BYTES: usize = 64 * 1024;

/// Largest admitted diagnostic timeout, in milliseconds.
pub const MAX_DIAGNOSTIC_TIMEOUT_MS: u32 = 30_000;

/// Largest admitted request encoding: header, arguments, and timeout.
pub const MAX_DIAGNOSTIC_REQUEST_BYTES: usize = crate::PAYLOAD_MAX;

/// One admitted diagnostic request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRequest {
    /// Required protocol version.
    pub version: u16,
    /// BLAKE3 identity of the executable bytes.
    pub executable: [u8; 32],
    /// Arguments, excluding the executable.
    pub argv: Vec<String>,
    /// Bounded stdin in v2, canonical observation profile in v3; empty in v1.
    pub stdin: Vec<u8>,
    /// Legacy watchdog; v3 requires the fixed sentinel but uses only virtual ticks.
    pub timeout_ms: u32,
}

/// Reason a diagnostic request was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticError {
    /// The version is not supported.
    UnsupportedVersion {
        /// Version found in the request.
        found: u16,
    },
    /// The executable identity is all zeroes.
    MissingExecutable,
    /// No arguments were supplied.
    MissingArguments,
    /// Too many arguments were supplied.
    TooManyArguments {
        /// Count found in the request.
        found: usize,
    },
    /// One argument is empty or longer than the limit.
    InvalidArgument {
        /// Argument position.
        index: usize,
        /// Encoded length found.
        len: usize,
    },
    /// The encoded arguments exceed the total byte limit.
    ArgumentsTooLarge {
        /// Total encoded bytes found.
        found: usize,
    },
    /// The timeout is zero or above the limit.
    InvalidTimeout {
        /// Timeout found in the request.
        found: u32,
    },
    /// The stdin payload exceeds the version's bound or is present in version 1.
    InputTooLarge {
        /// Input length found in the request.
        found: usize,
    },
    /// The encoding is truncated, malformed, or has trailing bytes.
    MalformedEncoding,
    /// An argument is not valid UTF-8.
    NonUtf8Argument {
        /// Argument position.
        index: usize,
    },
    /// The encoding exceeds the request byte limit.
    RequestTooLarge {
        /// Encoded size found.
        found: usize,
    },
    /// The observation profile is malformed, over-bound, or differs from its envelope.
    InvalidObservation,
}

/// Admit one request without encoding it.
pub fn admit(
    version: u16,
    executable: &[u8; 32],
    argv: &[String],
    stdin: &[u8],
    timeout_ms: u32,
) -> Result<(), DiagnosticError> {
    if version != DIAGNOSTIC_PROTOCOL_VERSION
        && version != DIAGNOSTIC_PROTOCOL_VERSION_V1
        && version != OBSERVATION_PROTOCOL_VERSION
    {
        return Err(DiagnosticError::UnsupportedVersion { found: version });
    }
    if version == DIAGNOSTIC_PROTOCOL_VERSION_V1 && !stdin.is_empty() {
        return Err(DiagnosticError::InputTooLarge { found: stdin.len() });
    }
    if stdin.len() > MAX_DIAGNOSTIC_STDIN_BYTES {
        return Err(DiagnosticError::InputTooLarge { found: stdin.len() });
    }
    if executable.iter().all(|byte| *byte == 0) {
        return Err(DiagnosticError::MissingExecutable);
    }
    if argv.is_empty() && version != OBSERVATION_PROTOCOL_VERSION {
        return Err(DiagnosticError::MissingArguments);
    }
    if argv.len() > MAX_DIAGNOSTIC_ARGS {
        return Err(DiagnosticError::TooManyArguments { found: argv.len() });
    }
    let mut total = 0;
    for (index, argument) in argv.iter().enumerate() {
        let len = argument.len();
        if len == 0 || len > MAX_DIAGNOSTIC_ARG_BYTES {
            return Err(DiagnosticError::InvalidArgument { index, len });
        }
        total += len;
    }
    if total > MAX_DIAGNOSTIC_TOTAL_ARG_BYTES {
        return Err(DiagnosticError::ArgumentsTooLarge { found: total });
    }
    if timeout_ms == 0 || timeout_ms > MAX_DIAGNOSTIC_TIMEOUT_MS {
        return Err(DiagnosticError::InvalidTimeout { found: timeout_ms });
    }
    let stdin_len = if version == DIAGNOSTIC_PROTOCOL_VERSION_V1 {
        0
    } else {
        2 + stdin.len()
    };
    let encoded_len = 2 + 32 + 2 + 2 * argv.len() + total + stdin_len + 4;
    if encoded_len > crate::PAYLOAD_MAX {
        return Err(DiagnosticError::RequestTooLarge { found: encoded_len });
    }
    if version == OBSERVATION_PROTOCOL_VERSION {
        let profile: ObservationProfile =
            serde_json::from_slice(stdin).map_err(|_| DiagnosticError::InvalidObservation)?;
        profile
            .validate()
            .map_err(|_| DiagnosticError::InvalidObservation)?;
        if profile.executable_blake3 != *executable
            || profile.argv != argv
            || timeout_ms != MAX_DIAGNOSTIC_TIMEOUT_MS
            || serde_json::to_vec(&profile).map_err(|_| DiagnosticError::InvalidObservation)?
                != stdin
        {
            return Err(DiagnosticError::InvalidObservation);
        }
    }
    Ok(())
}

/// Fully declared guest command. Identity is BLAKE3 of canonical JSON.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationProfile {
    pub executable: String,
    pub executable_blake3: [u8; 32],
    pub argv: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
    pub working_directory: String,
    pub deadline_tick: u64,
    pub stdout_limit: u32,
    pub stderr_limit: u32,
    pub mutates_guest: bool,
}

pub const MAX_OBSERVATION_ENV: usize = 16;
pub const MAX_OBSERVATION_ENV_BYTES: usize = 2048;
pub const MAX_OBSERVATION_PATH_BYTES: usize = 256;
pub const MAX_OBSERVATION_TICKS: u64 = 1_000_000;

/// Only normalized, nonempty relative paths; `.` is permitted only for cwd.
pub fn admit_observation_path(path: &str, directory: bool) -> Result<(), String> {
    if directory && path == "." {
        return Ok(());
    }
    if path.is_empty()
        || path.len() > MAX_OBSERVATION_PATH_BYTES
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("observation path must be normalized and relative".into());
    }
    Ok(())
}

impl ObservationProfile {
    pub fn validate(&self) -> Result<(), String> {
        admit_observation_path(&self.executable, false)?;
        admit_observation_path(&self.working_directory, true)?;
        if self.executable_blake3 == [0; 32]
            || self.deadline_tick == 0
            || self.argv.len() > MAX_DIAGNOSTIC_ARGS
            || self.environment.len() > MAX_OBSERVATION_ENV
            || self.stdout_limit == 0
            || self.stderr_limit == 0
            || self.stdout_limit as usize > MAX_DIAGNOSTIC_CAPTURE_BYTES
            || self.stderr_limit as usize > MAX_DIAGNOSTIC_CAPTURE_BYTES
        {
            return Err("observation identity or limits are invalid".into());
        }
        let mut total = 0usize;
        for argument in &self.argv {
            if argument.is_empty()
                || argument.len() > MAX_DIAGNOSTIC_ARG_BYTES
                || argument.contains('\0')
                || argument.starts_with('/')
                || argument.contains('\\')
                || argument.split('/').any(|part| part == "..")
            {
                return Err("observation argument escapes profile bounds".into());
            }
            total = total
                .checked_add(argument.len())
                .ok_or("argument overflow")?;
        }
        if total > MAX_DIAGNOSTIC_TOTAL_ARG_BYTES {
            return Err("argument byte bound".into());
        }
        let mut env_bytes = 0usize;
        for (key, value) in &self.environment {
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                || key.starts_with("LD_")
                || key.starts_with("DYLD_")
                || matches!(key.as_str(), "PATH" | "ENV" | "BASH_ENV" | "SHELLOPTS")
                || value.contains('\0')
                || value.len() > MAX_DIAGNOSTIC_ARG_BYTES
            {
                return Err("observation environment is not admitted".into());
            }
            env_bytes = env_bytes
                .checked_add(key.len() + value.len())
                .ok_or("environment overflow")?;
        }
        if env_bytes > MAX_OBSERVATION_ENV_BYTES {
            return Err("environment byte bound".into());
        }
        Ok(())
    }

    pub fn identity(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub fn request(&self) -> Result<DiagnosticRequest, String> {
        self.validate()?;
        let request = DiagnosticRequest {
            version: OBSERVATION_PROTOCOL_VERSION,
            executable: self.executable_blake3,
            argv: self.argv.clone(),
            stdin: serde_json::to_vec(self).map_err(|e| e.to_string())?,
            timeout_ms: MAX_DIAGNOSTIC_TIMEOUT_MS,
        };
        encode_request(&request).map_err(|e| format!("{e:?}"))?;
        Ok(request)
    }
}

/// Decode a v3 profile and reject drift between envelope and command.
pub fn observation_profile(
    request: &DiagnosticRequest,
) -> Result<Option<ObservationProfile>, String> {
    if request.version != OBSERVATION_PROTOCOL_VERSION {
        return Ok(None);
    }
    let profile: ObservationProfile =
        serde_json::from_slice(&request.stdin).map_err(|e| e.to_string())?;
    profile.validate()?;
    let expected = profile.request()?;
    if expected != *request {
        return Err("observation profile/request drift".into());
    }
    Ok(Some(profile))
}

/// Encode one admitted request in canonical little-endian form.
pub fn encode_request(request: &DiagnosticRequest) -> Result<Vec<u8>, DiagnosticError> {
    admit(
        request.version,
        &request.executable,
        &request.argv,
        &request.stdin,
        request.timeout_ms,
    )?;
    let mut encoded = Vec::with_capacity(
        2 + 32
            + 2
            + request.argv.iter().map(String::len).sum::<usize>()
            + 2 * request.argv.len()
            + if request.version == DIAGNOSTIC_PROTOCOL_VERSION_V1 {
                0
            } else {
                2 + request.stdin.len()
            }
            + 4,
    );
    encoded.extend_from_slice(&request.version.to_le_bytes());
    encoded.extend_from_slice(&request.executable);
    encoded.extend_from_slice(&(request.argv.len() as u16).to_le_bytes());
    for argument in &request.argv {
        encoded.extend_from_slice(&(argument.len() as u16).to_le_bytes());
        encoded.extend_from_slice(argument.as_bytes());
    }
    if request.version != DIAGNOSTIC_PROTOCOL_VERSION_V1 {
        encoded.extend_from_slice(&(request.stdin.len() as u16).to_le_bytes());
        encoded.extend_from_slice(&request.stdin);
    }
    encoded.extend_from_slice(&request.timeout_ms.to_le_bytes());
    Ok(encoded)
}

/// Decode and admit one request from canonical bytes.
pub fn decode_request(bytes: &[u8]) -> Result<DiagnosticRequest, DiagnosticError> {
    if bytes.len() > MAX_DIAGNOSTIC_REQUEST_BYTES {
        return Err(DiagnosticError::RequestTooLarge { found: bytes.len() });
    }
    let mut cursor = 0;
    let read = |cursor: &mut usize, count: usize| -> Result<&[u8], DiagnosticError> {
        let end = cursor
            .checked_add(count)
            .ok_or(DiagnosticError::MalformedEncoding)?;
        let slice = bytes
            .get(*cursor..end)
            .ok_or(DiagnosticError::MalformedEncoding)?;
        *cursor = end;
        Ok(slice)
    };
    let version = u16::from_le_bytes(
        read(&mut cursor, 2)?
            .try_into()
            .map_err(|_| DiagnosticError::MalformedEncoding)?,
    );
    let mut executable = [0u8; 32];
    executable.copy_from_slice(read(&mut cursor, 32)?);
    let argc = u16::from_le_bytes(
        read(&mut cursor, 2)?
            .try_into()
            .map_err(|_| DiagnosticError::MalformedEncoding)?,
    ) as usize;
    if argc > MAX_DIAGNOSTIC_ARGS {
        return Err(DiagnosticError::TooManyArguments { found: argc });
    }
    let mut argv = Vec::with_capacity(argc);
    for index in 0..argc {
        let len = u16::from_le_bytes(
            read(&mut cursor, 2)?
                .try_into()
                .map_err(|_| DiagnosticError::MalformedEncoding)?,
        ) as usize;
        if len == 0 || len > MAX_DIAGNOSTIC_ARG_BYTES {
            return Err(DiagnosticError::InvalidArgument { index, len });
        }
        let argument = std::str::from_utf8(read(&mut cursor, len)?)
            .map_err(|_| DiagnosticError::NonUtf8Argument { index })?;
        argv.push(argument.to_string());
    }
    let mut stdin = Vec::new();
    if version != DIAGNOSTIC_PROTOCOL_VERSION_V1 {
        let len = u16::from_le_bytes(
            read(&mut cursor, 2)?
                .try_into()
                .map_err(|_| DiagnosticError::MalformedEncoding)?,
        ) as usize;
        if len > MAX_DIAGNOSTIC_STDIN_BYTES {
            return Err(DiagnosticError::InputTooLarge { found: len });
        }
        stdin.extend_from_slice(read(&mut cursor, len)?);
    }
    let timeout_ms = u32::from_le_bytes(
        read(&mut cursor, 4)?
            .try_into()
            .map_err(|_| DiagnosticError::MalformedEncoding)?,
    );
    if cursor != bytes.len() {
        return Err(DiagnosticError::MalformedEncoding);
    }
    admit(version, &executable, &argv, &stdin, timeout_ms)?;
    Ok(DiagnosticRequest {
        version,
        executable,
        argv,
        stdin,
        timeout_ms,
    })
}

/// Version of the diagnostic receipt encoding.
pub const DIAGNOSTIC_RECEIPT_VERSION: u16 = 2;

/// Termination class recorded in a diagnostic receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticReceiptStatus {
    /// The process exited before the timeout.
    Completed = 0,
    /// The timeout expired and the process was killed.
    TimedOut = 1,
}

// r[impl chaoscontrol.guest_diagnostic.receipt_binding]
/// Bounded receipt for one diagnostic execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticReceipt {
    /// Receipt encoding version.
    pub version: u16,
    /// BLAKE3 identity of the canonical request encoding.
    pub request_digest: [u8; 32],
    /// BLAKE3 identity of the executed file.
    pub executable: [u8; 32],
    /// Termination class.
    pub status: DiagnosticReceiptStatus,
    /// Exit code when the process reported one.
    pub exit_code: Option<i32>,
    /// Whether a stream exceeded its bound or complete capture could not be established.
    pub truncated: bool,
    // r[impl chaoscontrol.guest_diagnostic.receipt_counterfactual]
    /// Whether the execution is a counterfactual guest-branch perturbation.
    pub counterfactual: bool,
    /// Captured stdout length.
    pub stdout_bytes: u32,
    /// BLAKE3 identity of captured stdout bytes.
    pub stdout_digest: [u8; 32],
    /// Captured stderr length.
    pub stderr_bytes: u32,
    /// BLAKE3 identity of captured stderr bytes.
    pub stderr_digest: [u8; 32],
}

/// Reason a diagnostic receipt was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticReceiptError {
    /// The receipt version is not supported.
    UnsupportedVersion {
        /// Version found in the receipt.
        found: u16,
    },
    /// The status byte is not a known class.
    InvalidStatus {
        /// Status byte found in the receipt.
        found: u8,
    },
    /// The presence byte for the exit code is not 0 or 1.
    InvalidExitPresence {
        /// Byte found in the receipt.
        found: u8,
    },
    /// The truncation byte is not 0 or 1.
    InvalidTruncated {
        /// Byte found in the receipt.
        found: u8,
    },
    /// The counterfactual byte is not 0 or 1.
    InvalidCounterfactual {
        /// Byte found in the receipt.
        found: u8,
    },
    /// A captured output length exceeds the capture bound.
    OutputTooLarge {
        /// Length found in the receipt.
        found: u32,
    },
    /// The encoding is truncated, malformed, or has trailing bytes.
    MalformedEncoding,
}

fn validate_receipt(receipt: &DiagnosticReceipt) -> Result<(), DiagnosticReceiptError> {
    if receipt.version != DIAGNOSTIC_RECEIPT_VERSION {
        return Err(DiagnosticReceiptError::UnsupportedVersion {
            found: receipt.version,
        });
    }
    for found in [receipt.stdout_bytes, receipt.stderr_bytes] {
        if found as usize > MAX_DIAGNOSTIC_CAPTURE_BYTES {
            return Err(DiagnosticReceiptError::OutputTooLarge { found });
        }
    }
    Ok(())
}

/// Encode one diagnostic receipt in canonical little-endian form.
pub fn encode_receipt(receipt: &DiagnosticReceipt) -> Result<Vec<u8>, DiagnosticReceiptError> {
    validate_receipt(receipt)?;
    let mut encoded = Vec::with_capacity(2 + 32 + 32 + 1 + 1 + 4 + 1 + 1 + 4 + 32 + 4 + 32);
    encoded.extend_from_slice(&receipt.version.to_le_bytes());
    encoded.extend_from_slice(&receipt.request_digest);
    encoded.extend_from_slice(&receipt.executable);
    encoded.push(receipt.status as u8);
    match receipt.exit_code {
        Some(code) => {
            encoded.push(1);
            encoded.extend_from_slice(&code.to_le_bytes());
        }
        None => encoded.push(0),
    }
    encoded.push(u8::from(receipt.truncated));
    encoded.push(u8::from(receipt.counterfactual));
    encoded.extend_from_slice(&receipt.stdout_bytes.to_le_bytes());
    encoded.extend_from_slice(&receipt.stdout_digest);
    encoded.extend_from_slice(&receipt.stderr_bytes.to_le_bytes());
    encoded.extend_from_slice(&receipt.stderr_digest);
    Ok(encoded)
}

/// Decode and validate one diagnostic receipt from canonical bytes.
pub fn decode_receipt(bytes: &[u8]) -> Result<DiagnosticReceipt, DiagnosticReceiptError> {
    if bytes.len() > 2 + 32 + 32 + 1 + (1 + 4) + 1 + 1 + 4 + 32 + 4 + 32 {
        return Err(DiagnosticReceiptError::MalformedEncoding);
    }
    let mut cursor = 0;
    let read = |cursor: &mut usize, count: usize| -> Result<&[u8], DiagnosticReceiptError> {
        let end = cursor
            .checked_add(count)
            .ok_or(DiagnosticReceiptError::MalformedEncoding)?;
        let slice = bytes
            .get(*cursor..end)
            .ok_or(DiagnosticReceiptError::MalformedEncoding)?;
        *cursor = end;
        Ok(slice)
    };
    let version = u16::from_le_bytes(
        read(&mut cursor, 2)?
            .try_into()
            .map_err(|_| DiagnosticReceiptError::MalformedEncoding)?,
    );
    let mut request_digest = [0u8; 32];
    request_digest.copy_from_slice(read(&mut cursor, 32)?);
    let mut executable = [0u8; 32];
    executable.copy_from_slice(read(&mut cursor, 32)?);
    let status = match read(&mut cursor, 1)?[0] {
        0 => DiagnosticReceiptStatus::Completed,
        1 => DiagnosticReceiptStatus::TimedOut,
        found => return Err(DiagnosticReceiptError::InvalidStatus { found }),
    };
    let exit_code = match read(&mut cursor, 1)?[0] {
        0 => None,
        1 => Some(i32::from_le_bytes(
            read(&mut cursor, 4)?
                .try_into()
                .map_err(|_| DiagnosticReceiptError::MalformedEncoding)?,
        )),
        found => return Err(DiagnosticReceiptError::InvalidExitPresence { found }),
    };
    let truncated = match read(&mut cursor, 1)?[0] {
        0 => false,
        1 => true,
        found => return Err(DiagnosticReceiptError::InvalidTruncated { found }),
    };
    let counterfactual = match read(&mut cursor, 1)?[0] {
        0 => false,
        1 => true,
        found => return Err(DiagnosticReceiptError::InvalidCounterfactual { found }),
    };
    let stdout_bytes = u32::from_le_bytes(
        read(&mut cursor, 4)?
            .try_into()
            .map_err(|_| DiagnosticReceiptError::MalformedEncoding)?,
    );
    let mut stdout_digest = [0u8; 32];
    stdout_digest.copy_from_slice(read(&mut cursor, 32)?);
    let stderr_bytes = u32::from_le_bytes(
        read(&mut cursor, 4)?
            .try_into()
            .map_err(|_| DiagnosticReceiptError::MalformedEncoding)?,
    );
    let mut stderr_digest = [0u8; 32];
    stderr_digest.copy_from_slice(read(&mut cursor, 32)?);
    if cursor != bytes.len() {
        return Err(DiagnosticReceiptError::MalformedEncoding);
    }
    let receipt = DiagnosticReceipt {
        version,
        request_digest,
        executable,
        status,
        exit_code,
        truncated,
        counterfactual,
        stdout_bytes,
        stderr_bytes,
        stdout_digest,
        stderr_digest,
    };
    validate_receipt(&receipt)?;
    Ok(receipt)
}

/// BLAKE3 identity of the canonical receipt encoding.
pub fn receipt_digest(receipt: &DiagnosticReceipt) -> Result<[u8; 32], DiagnosticReceiptError> {
    Ok(*blake3::hash(&encode_receipt(receipt)?).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DiagnosticRequest {
        DiagnosticRequest {
            version: DIAGNOSTIC_PROTOCOL_VERSION,
            executable: [0x42; 32],
            argv: vec!["--json".to_string(), "/tmp/probe".to_string()],
            stdin: b"payload".to_vec(),
            timeout_ms: 250,
        }
    }

    #[test]
    fn request_roundtrips_canonically() {
        let encoded = encode_request(&request()).unwrap();
        assert_eq!(decode_request(&encoded).unwrap(), request());
    }

    #[test]
    fn version_and_identity_are_admitted() {
        let mut value = request();
        value.version = 4;
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::UnsupportedVersion { found: 4 })
        );
        value.version = DIAGNOSTIC_PROTOCOL_VERSION;
        value.executable = [0; 32];
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::MissingExecutable)
        );
    }

    #[test]
    fn argument_bounds_are_admitted() {
        let mut value = request();
        value.argv = Vec::new();
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::MissingArguments)
        );
        value.argv = vec!["x".to_string(); MAX_DIAGNOSTIC_ARGS + 1];
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::TooManyArguments {
                found: MAX_DIAGNOSTIC_ARGS + 1
            })
        );
        value.argv = vec!["x".repeat(MAX_DIAGNOSTIC_ARG_BYTES + 1)];
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::InvalidArgument {
                index: 0,
                len: MAX_DIAGNOSTIC_ARG_BYTES + 1
            })
        );
        value.argv = vec!["x".repeat(MAX_DIAGNOSTIC_ARG_BYTES); 5];
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::ArgumentsTooLarge {
                found: 5 * MAX_DIAGNOSTIC_ARG_BYTES
            })
        );
    }

    #[test]
    fn requests_must_fit_the_hypercall_payload() {
        let mut value = request();
        value.stdin.clear();
        value.argv = vec!["x".repeat(MAX_DIAGNOSTIC_ARG_BYTES); 4];
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::RequestTooLarge { found: 4146 })
        );
    }

    #[test]
    fn timeout_bounds_are_admitted() {
        let mut value = request();
        value.timeout_ms = 0;
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::InvalidTimeout { found: 0 })
        );
        value.timeout_ms = MAX_DIAGNOSTIC_TIMEOUT_MS + 1;
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::InvalidTimeout {
                found: MAX_DIAGNOSTIC_TIMEOUT_MS + 1
            })
        );
    }

    fn receipt() -> DiagnosticReceipt {
        DiagnosticReceipt {
            version: DIAGNOSTIC_RECEIPT_VERSION,
            request_digest: [0x11; 32],
            executable: [0x22; 32],
            status: DiagnosticReceiptStatus::Completed,
            exit_code: Some(0),
            truncated: false,
            counterfactual: true,
            stdout_bytes: 6,
            stdout_digest: [0x33; 32],
            stderr_bytes: 0,
            stderr_digest: [0x44; 32],
        }
    }

    #[test]
    fn receipt_roundtrips_canonically() {
        let encoded = encode_receipt(&receipt()).unwrap();
        assert_eq!(decode_receipt(&encoded).unwrap(), receipt());
        assert_eq!(
            receipt_digest(&receipt()).unwrap(),
            *blake3::hash(&encoded).as_bytes()
        );
    }

    #[test]
    fn receipt_rejects_out_of_range_fields() {
        let mut value = receipt();
        value.version = 3;
        assert_eq!(
            encode_receipt(&value),
            Err(DiagnosticReceiptError::UnsupportedVersion { found: 3 })
        );
        value.version = DIAGNOSTIC_RECEIPT_VERSION;
        value.stdout_bytes = MAX_DIAGNOSTIC_CAPTURE_BYTES as u32 + 1;
        assert!(matches!(
            encode_receipt(&value),
            Err(DiagnosticReceiptError::OutputTooLarge { .. })
        ));
        let mut encoded = encode_receipt(&receipt()).unwrap();
        encoded[2 + 32 + 32] = 2;
        assert_eq!(
            decode_receipt(&encoded),
            Err(DiagnosticReceiptError::InvalidStatus { found: 2 })
        );
        let mut encoded = encode_receipt(&receipt()).unwrap();
        encoded[2 + 32 + 32 + 1 + 1 + 4 + 1] = 2;
        assert_eq!(
            decode_receipt(&encoded),
            Err(DiagnosticReceiptError::InvalidCounterfactual { found: 2 })
        );
    }

    #[test]
    fn receipt_rejects_malformed_encodings() {
        let encoded = encode_receipt(&receipt()).unwrap();
        assert_eq!(
            decode_receipt(&encoded[..encoded.len() - 1]),
            Err(DiagnosticReceiptError::MalformedEncoding)
        );
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            decode_receipt(&trailing),
            Err(DiagnosticReceiptError::MalformedEncoding)
        );
    }

    #[test]
    fn stdin_is_bounded_and_versioned() {
        let mut value = request();
        value.version = DIAGNOSTIC_PROTOCOL_VERSION_V1;
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::InputTooLarge { found: 7 })
        );
        value.stdin.clear();
        let encoded = encode_request(&value).unwrap();
        assert_eq!(decode_request(&encoded).unwrap().stdin, b"");
        value.version = DIAGNOSTIC_PROTOCOL_VERSION;
        value.stdin = vec![0u8; MAX_DIAGNOSTIC_STDIN_BYTES + 1];
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::InputTooLarge {
                found: MAX_DIAGNOSTIC_STDIN_BYTES + 1
            })
        );
    }

    #[test]
    fn malformed_encodings_are_rejected() {
        let encoded = encode_request(&request()).unwrap();
        assert_eq!(
            decode_request(&encoded[..encoded.len() - 1]),
            Err(DiagnosticError::MalformedEncoding)
        );
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            decode_request(&trailing),
            Err(DiagnosticError::MalformedEncoding)
        );
        let mut invalid = encoded;
        invalid[2 + 32 + 2] = 0xff;
        assert_eq!(
            decode_request(&invalid),
            Err(DiagnosticError::MalformedEncoding)
        );
    }
}
