//! Guest diagnostic request contract.
//!
//! Defines the versioned host-to-supervisor request used to run one admitted
//! diagnostic operation inside the guest. Admission is lexical and bounded: an
//! exact executable identity, a bounded argument vector, and a bounded timeout.
//! The contract does not execute anything, authorize an executable, or claim
//! that a request produced evidence. The supervisor supplies the executable path
//! from its admitted manifest and verifies it against ``executable`` before running.

// r[impl chaoscontrol.guest_diagnostic.request_admission]
/// Current diagnostic request encoding version (bounded stdin supported).
pub const DIAGNOSTIC_PROTOCOL_VERSION: u16 = 2;

/// Legacy diagnostic request version without stdin.
pub const DIAGNOSTIC_PROTOCOL_VERSION_V1: u16 = 1;

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
pub const MAX_DIAGNOSTIC_REQUEST_BYTES: usize =
    2 + 32 + 2 + MAX_DIAGNOSTIC_TOTAL_ARG_BYTES + 2 * MAX_DIAGNOSTIC_ARGS + 4;

/// One admitted diagnostic request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRequest {
    /// Required protocol version.
    pub version: u16,
    /// BLAKE3 identity of the executable bytes.
    pub executable: [u8; 32],
    /// Arguments, excluding the executable.
    pub argv: Vec<String>,
    /// Bounded stdin bytes; must be empty for version 1 requests.
    pub stdin: Vec<u8>,
    /// Wall-clock bound for the operation.
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
}

/// Admit one request without encoding it.
pub fn admit(
    version: u16,
    executable: &[u8; 32],
    argv: &[String],
    stdin: &[u8],
    timeout_ms: u32,
) -> Result<(), DiagnosticError> {
    if version != DIAGNOSTIC_PROTOCOL_VERSION && version != DIAGNOSTIC_PROTOCOL_VERSION_V1 {
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
    if argv.is_empty() {
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
    Ok(())
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
    /// Whether either captured stream exceeded its bound.
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
        value.version = 3;
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::UnsupportedVersion { found: 3 })
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
