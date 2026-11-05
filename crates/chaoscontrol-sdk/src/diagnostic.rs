//! Bounded execution of admitted guest diagnostic requests.
//!
//! The supervisor supplies the executable path from its admitted manifest; the
//! request carries only the exact BLAKE3 identity and arguments. Execution reads
//! and hashes the executable before spawning, captures bounded output, enforces
//! the request timeout, and returns an outcome the caller can bind into a receipt.
//! It does not prove the executable is trustworthy or that the output is correct.

use chaoscontrol_protocol::diagnostic::{
    admit, decode_request, encode_receipt, encode_request, DiagnosticError, DiagnosticReceipt,
    DiagnosticReceiptStatus, DiagnosticRequest,
};
use std::io::{Read, Write};
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
    /// Captured output could not be recorded as an artifact.
    Artifact(std::io::Error),
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
    run_diagnostic_inner(request, executable_path, None).map(|(outcome, _)| outcome)
}

// r[impl chaoscontrol.guest_diagnostic.diagnostic_artifacts]
/// Run one admitted request and record non-empty captured streams as
/// content-addressed artifacts.
///
/// Files are named `blake3-<hex>.stdout` and `.stderr` under `artifact_dir`, which
/// is created if missing. The returned paths identify the written bytes; the
/// receipt still reports only bounded digests and lengths.
pub fn run_diagnostic_with_artifacts(
    request: &DiagnosticRequest,
    executable_path: &std::path::Path,
    artifact_dir: &std::path::Path,
) -> Result<(DiagnosticOutcome, Vec<std::path::PathBuf>), DiagnosticRunError> {
    run_diagnostic_inner(request, executable_path, Some(artifact_dir))
}

fn run_diagnostic_inner(
    request: &DiagnosticRequest,
    executable_path: &std::path::Path,
    artifact_dir: Option<&std::path::Path>,
) -> Result<(DiagnosticOutcome, Vec<std::path::PathBuf>), DiagnosticRunError> {
    admit(
        request.version,
        &request.executable,
        &request.argv,
        &request.stdin,
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
        .stdin(if request.stdin.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(DiagnosticRunError::Spawn)?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(&request.stdin)
            .map_err(DiagnosticRunError::Spawn)?;
    }
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
    let mut artifacts = Vec::new();
    if let Some(directory) = artifact_dir {
        std::fs::create_dir_all(directory).map_err(DiagnosticRunError::Artifact)?;
        for (suffix, bytes) in [("stdout", &stdout_bytes), ("stderr", &stderr_bytes)] {
            if bytes.is_empty() {
                continue;
            }
            let name = format!("blake3-{}.{}", blake3::hash(bytes).to_hex(), suffix);
            let path = directory.join(name);
            std::fs::write(&path, bytes).map_err(DiagnosticRunError::Artifact)?;
            artifacts.push(path);
        }
    }
    Ok((
        DiagnosticOutcome {
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
        },
        artifacts,
    ))
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

// r[impl chaoscontrol.guest_diagnostic.guest_client]
/// Poll one host-queued diagnostic request.
///
/// Returns `Ok(None)` when the host has nothing queued or when no VM transport is
/// active. Malformed payloads fail closed.
pub fn poll_diagnostic_request() -> Result<Option<DiagnosticRequest>, String> {
    let (present, status, payload) =
        crate::transport::hypercall_response(chaoscontrol_protocol::CMD_DIAGNOSTIC_REQUEST);
    if status != chaoscontrol_protocol::STATUS_OK {
        return Err(format!("diagnostic poll failed with status {status}"));
    }
    if present == 0 {
        return Ok(None);
    }
    decode_request(&payload)
        .map(Some)
        .map_err(|error| format!("{error:?}"))
}

/// Report one canonical receipt to the host.
pub fn report_diagnostic_receipt(receipt: &DiagnosticReceipt) -> Result<(), String> {
    let encoded = encode_receipt(receipt).map_err(|error| format!("{error:?}"))?;
    let (_, status) = crate::transport::hypercall_raw(
        chaoscontrol_protocol::CMD_DIAGNOSTIC_REPORT,
        0,
        0,
        &encoded,
    );
    if status != chaoscontrol_protocol::STATUS_OK {
        return Err(format!("diagnostic report failed with status {status}"));
    }
    Ok(())
}

/// Poll, execute, and report at most one diagnostic request.
///
/// The supervisor supplies the executable path from its admitted manifest.
pub fn serve_one_diagnostic(
    executable_path: &std::path::Path,
) -> Result<Option<DiagnosticReceipt>, String> {
    let Some(request) = poll_diagnostic_request()? else {
        return Ok(None);
    };
    let outcome =
        run_diagnostic(&request, executable_path).map_err(|error| format!("{error:?}"))?;
    let receipt = outcome.receipt();
    report_diagnostic_receipt(&receipt)?;
    Ok(Some(receipt))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_for(path: &std::path::Path, argv: &[&str], timeout_ms: u32) -> DiagnosticRequest {
        let bytes = std::fs::read(path).unwrap();
        DiagnosticRequest {
            version: chaoscontrol_protocol::diagnostic::DIAGNOSTIC_PROTOCOL_VERSION,
            executable: *blake3::hash(&bytes).as_bytes(),
            argv: argv.iter().map(|value| value.to_string()).collect(),
            stdin: Vec::new(),
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
    fn local_transport_has_no_requests_and_accepts_report() {
        assert_eq!(poll_diagnostic_request().unwrap(), None);
        let receipt = super::DiagnosticReceipt {
            version: chaoscontrol_protocol::diagnostic::DIAGNOSTIC_RECEIPT_VERSION,
            request_digest: [0x11; 32],
            executable: [0x22; 32],
            status: DiagnosticReceiptStatus::Completed,
            exit_code: Some(0),
            truncated: false,
            stdout_bytes: 0,
            stdout_digest: [0; 32],
            stderr_bytes: 0,
            stderr_digest: [0; 32],
        };
        report_diagnostic_receipt(&receipt).unwrap();
    }

    #[test]
    fn artifacts_are_content_addressed() {
        let path = std::path::Path::new("/bin/echo");
        if !path.exists() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let request = request_for(path, &["artifact"], 1_000);
        let (outcome, artifacts) =
            run_diagnostic_with_artifacts(&request, path, dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        let expected = format!("blake3-{}.stdout", blake3::hash(b"artifact\n").to_hex());
        assert_eq!(artifacts[0].file_name().unwrap(), expected.as_str());
        assert_eq!(std::fs::read(&artifacts[0]).unwrap(), b"artifact\n");
        assert_eq!(outcome.stdout_bytes, 9);
    }

    #[test]
    fn stdin_is_delivered_to_the_process() {
        let path = std::path::Path::new("/bin/cat");
        if !path.exists() {
            return;
        }
        let mut request = request_for(path, &[], 1_000);
        request.stdin = b"bounded input\n".to_vec();
        let outcome = run_diagnostic(&request, path).unwrap();
        assert_eq!(outcome.status, DiagnosticStatus::Completed);
        assert_eq!(outcome.stdout_bytes, 14);
        assert_eq!(
            outcome.stdout_digest,
            *blake3::hash(b"bounded input\n").as_bytes()
        );
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
