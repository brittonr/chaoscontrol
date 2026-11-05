//! Guest diagnostic request contract.
//!
//! Defines the versioned host-to-supervisor request used to run one admitted
//! diagnostic operation inside the guest. Admission is lexical and bounded: an
//! exact executable identity, a bounded argument vector, and a bounded timeout.
//! The contract does not execute anything, authorize an executable, or claim
//! that a request produced evidence. The supervisor supplies the executable path
//! from its admitted manifest and verifies it against ``executable`` before running.

// r[impl chaoscontrol.guest_diagnostic.request_admission]
/// Version of the diagnostic request encoding.
pub const DIAGNOSTIC_PROTOCOL_VERSION: u16 = 1;

/// Maximum number of arguments, excluding the executable.
pub const MAX_DIAGNOSTIC_ARGS: usize = 16;

/// Maximum encoded length of one argument.
pub const MAX_DIAGNOSTIC_ARG_BYTES: usize = 1024;

/// Maximum encoded length of all arguments together.
pub const MAX_DIAGNOSTIC_TOTAL_ARG_BYTES: usize = 4096;

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
    timeout_ms: u32,
) -> Result<(), DiagnosticError> {
    if version != DIAGNOSTIC_PROTOCOL_VERSION {
        return Err(DiagnosticError::UnsupportedVersion { found: version });
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
    Ok(())
}

/// Encode one admitted request in canonical little-endian form.
pub fn encode_request(request: &DiagnosticRequest) -> Result<Vec<u8>, DiagnosticError> {
    admit(
        request.version,
        &request.executable,
        &request.argv,
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
    let timeout_ms = u32::from_le_bytes(
        read(&mut cursor, 4)?
            .try_into()
            .map_err(|_| DiagnosticError::MalformedEncoding)?,
    );
    if cursor != bytes.len() {
        return Err(DiagnosticError::MalformedEncoding);
    }
    admit(version, &executable, &argv, timeout_ms)?;
    Ok(DiagnosticRequest {
        version,
        executable,
        argv,
        timeout_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DiagnosticRequest {
        DiagnosticRequest {
            version: DIAGNOSTIC_PROTOCOL_VERSION,
            executable: [0x42; 32],
            argv: vec!["--json".to_string(), "/tmp/probe".to_string()],
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
        value.version = 2;
        assert_eq!(
            encode_request(&value),
            Err(DiagnosticError::UnsupportedVersion { found: 2 })
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
