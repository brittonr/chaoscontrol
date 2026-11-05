//! Bounded execution of admitted guest diagnostic requests.
//!
//! The supervisor supplies the executable path from its admitted manifest; the
//! request carries only the exact BLAKE3 identity and arguments. Execution reads
//! and hashes the executable before spawning, captures bounded output, enforces
//! the request timeout, and returns an outcome the caller can bind into a receipt.
//! It does not prove the executable is trustworthy or that the output is correct.

use chaoscontrol_protocol::diagnostic::{
    admit, encode_request, DiagnosticError, DiagnosticReceipt, DiagnosticReceiptStatus,
    DiagnosticRequest,
};
use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Largest executable this runner will read and hash.
pub const MAX_EXECUTABLE_BYTES: u64 = 16 * 1024 * 1024;

/// Largest captured byte count for each output stream.
pub const MAX_CAPTURE_BYTES: usize = 64 * 1024;

/// Termination class for one diagnostic execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticStatus {
    /// The process exited before the timeout.
    Completed,
    /// The timeout expired and the process was killed.
    TimedOut,
}

/// Bounded observation of one diagnostic execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticOutcome {
    /// BLAKE3 identity of the canonical request encoding.
    pub request_digest: [u8; 32],
    /// BLAKE3 identity of the executed file.
    pub executable: [u8; 32],
    /// Termination class.
    pub status: DiagnosticStatus,
    /// Exit code when the process reported one.
    pub exit_code: Option<i32>,
    /// Number of captured stdout bytes.
    pub stdout_bytes: usize,
    /// BLAKE3 identity of captured stdout bytes.
    pub stdout_digest: [u8; 32],
    /// Number of captured stderr bytes.
    pub stderr_bytes: usize,
    /// BLAKE3 identity of captured stderr bytes.
    pub stderr_digest: [u8; 32],
    /// Whether either stream exceeded the capture bound.
    pub truncated: bool,
}

/// Reason a diagnostic execution did not produce an outcome.
#[derive(Debug)]
pub enum DiagnosticRunError {
    /// The request failed admission.
    Admission(DiagnosticError),
    /// The executable exceeds the read bound.
    ExecutableTooLarge {
        /// Observed size.
        found: u64,
    },
    /// The executable could not be read.
    ExecutableUnreadable(std::io::Error),
    /// The file bytes do not match the requested identity.
    ExecutableIdentityMismatch {
        /// Identity from the request.
        expected: [u8; 32],
        /// Identity observed on disk.
        actual: [u8; 32],
    },
    /// The process could not be spawned.
    Spawn(std::io::Error),
    /// The process could not be waited for.
    Wait(std::io::Error),
    /// The encoded request could not be identified.
    Encoding(DiagnosticError),
}

fn capture<R: Read>(mut reader: R) -> (Vec<u8>, bool) {
    let mut captured = Vec::with_capacity(MAX_CAPTURE_BYTES.min(4096));
    let mut truncated = false;
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
                if remaining == 0 {
                    truncated = true;
                    break;
                }
                let take = read.min(remaining);
                captured.extend_from_slice(&chunk[..take]);
                if take < read {
                    truncated = true;
                }
            }
            Err(_) => {
                truncated = true;
                break;
            }
        }
    }
    (captured, truncated)
}

// r[impl chaoscontrol.guest_diagnostic.supervisor_execution]
/// Run one admitted request against the supervisor-chosen executable path.
pub fn run_diagnostic(
    request: &DiagnosticRequest,
    executable_path: &std::path::Path,
) -> Result<DiagnosticOutcome, DiagnosticRunError> {
    admit(
        request.version,
        &request.executable,
        &request.argv,
        request.timeout_ms,
    )
    .map_err(DiagnosticRunError::Admission)?;
    let encoded = encode_request(request).map_err(DiagnosticRunError::Encoding)?;
    let request_digest = *blake3::hash(&encoded).as_bytes();

    let metadata =
        std::fs::metadata(executable_path).map_err(DiagnosticRunError::ExecutableUnreadable)?;
    if metadata.len() > MAX_EXECUTABLE_BYTES {
        return Err(DiagnosticRunError::ExecutableTooLarge {
            found: metadata.len(),
        });
    }
    let bytes = std::fs::read(executable_path).map_err(DiagnosticRunError::ExecutableUnreadable)?;
    let actual = *blake3::hash(&bytes).as_bytes();
    if actual != request.executable {
        return Err(DiagnosticRunError::ExecutableIdentityMismatch {
            expected: request.executable,
            actual,
        });
    }

    let mut child = Command::new(executable_path)
        .args(&request.argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(DiagnosticRunError::Spawn)?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_reader = std::thread::spawn(move || capture(stdout));
    let stderr_reader = std::thread::spawn(move || capture(stderr));

    let deadline = Instant::now() + Duration::from_millis(u64::from(request.timeout_ms));
    let mut timed_out = false;
    let status: ExitStatus = loop {
        match child.try_wait().map_err(DiagnosticRunError::Wait)? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().map_err(DiagnosticRunError::Wait)?;
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };

    let (stdout_bytes, stdout_truncated) = stdout_reader.join().unwrap_or_default();
    let (stderr_bytes, stderr_truncated) = stderr_reader.join().unwrap_or_default();
    Ok(DiagnosticOutcome {
        request_digest,
        executable: request.executable,
        status: if timed_out {
            DiagnosticStatus::TimedOut
        } else {
            DiagnosticStatus::Completed
        },
        exit_code: status.code(),
        stdout_bytes: stdout_bytes.len(),
        stdout_digest: *blake3::hash(&stdout_bytes).as_bytes(),
        stderr_bytes: stderr_bytes.len(),
        stderr_digest: *blake3::hash(&stderr_bytes).as_bytes(),
        truncated: stdout_truncated || stderr_truncated,
    })
}

impl DiagnosticOutcome {
    /// Build the canonical receipt binding for this outcome.
    pub fn receipt(&self) -> DiagnosticReceipt {
        DiagnosticReceipt {
            version: chaoscontrol_protocol::diagnostic::DIAGNOSTIC_RECEIPT_VERSION,
            request_digest: self.request_digest,
            executable: self.executable,
            status: match self.status {
                DiagnosticStatus::Completed => DiagnosticReceiptStatus::Completed,
                DiagnosticStatus::TimedOut => DiagnosticReceiptStatus::TimedOut,
            },
            exit_code: self.exit_code,
            truncated: self.truncated,
            stdout_bytes: self.stdout_bytes as u32,
            stdout_digest: self.stdout_digest,
            stderr_bytes: self.stderr_bytes as u32,
            stderr_digest: self.stderr_digest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_protocol::diagnostic::encode_receipt;

    fn request_for(path: &std::path::Path, argv: &[&str], timeout_ms: u32) -> DiagnosticRequest {
        let bytes = std::fs::read(path).unwrap();
        DiagnosticRequest {
            version: chaoscontrol_protocol::diagnostic::DIAGNOSTIC_PROTOCOL_VERSION,
            executable: *blake3::hash(&bytes).as_bytes(),
            argv: argv.iter().map(|value| value.to_string()).collect(),
            timeout_ms,
        }
    }

    #[test]
    fn admitted_executable_runs_and_reports_bounded_output() {
        let path = std::path::Path::new("/bin/echo");
        if !path.exists() {
            return;
        }
        let request = request_for(path, &["hello"], 1_000);
        let outcome = run_diagnostic(&request, path).unwrap();
        assert_eq!(outcome.status, DiagnosticStatus::Completed);
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout_bytes, 6);
        assert_eq!(outcome.stdout_digest, *blake3::hash(b"hello\n").as_bytes());
        assert!(!outcome.truncated);
        let receipt = outcome.receipt();
        let encoded = encode_receipt(&receipt).unwrap();
        assert_eq!(
            chaoscontrol_protocol::diagnostic::decode_receipt(&encoded).unwrap(),
            receipt
        );
        assert_eq!(receipt.request_digest, outcome.request_digest);
        assert_eq!(receipt.executable, request.executable);
    }

    #[test]
    fn identity_mismatch_is_rejected_before_spawn() {
        let path = std::path::Path::new("/bin/echo");
        if !path.exists() {
            return;
        }
        let mut request = request_for(path, &[], 100);
        request.executable = [0x11; 32];
        assert!(matches!(
            run_diagnostic(&request, path),
            Err(DiagnosticRunError::ExecutableIdentityMismatch { .. })
        ));
    }

    #[test]
    fn timeout_kills_the_process() {
        let path = std::path::Path::new("/bin/sleep");
        if !path.exists() {
            return;
        }
        let request = request_for(path, &["5"], 50);
        let started = Instant::now();
        let outcome = run_diagnostic(&request, path).unwrap();
        assert_eq!(outcome.status, DiagnosticStatus::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
