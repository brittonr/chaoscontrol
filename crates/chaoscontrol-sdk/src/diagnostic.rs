//! Bounded execution of admitted guest diagnostic requests.
//!
//! The supervisor supplies an admitted executable or an exact retained v3
//! command profile. Observation execution is guest-only, opens each path without
//! following symlinks, hashes and executes the same pinned ELF, clears ambient
//! environment, captures bounded streams, and enforces the controller's virtual
//! tick deadline. Legacy diagnostic requests retain their wall-time watchdog.
//! Neither path proves output correctness or certifies the original history.

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
    /// Whether a stream exceeded its bound or complete capture could not be established.
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
    /// A declared observation failed its guest-only boundary.
    Observation(String),
}

fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> std::io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Own the read end, never a detached reader waiting for descendant-held EOF.
struct CapturePipe<R> {
    reader: Option<R>,
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl<R: Read + std::os::fd::AsRawFd> CapturePipe<R> {
    fn new(reader: R, limit: usize) -> std::io::Result<Self> {
        nonblocking(&reader)?;
        Ok(Self {
            reader: Some(reader),
            bytes: Vec::with_capacity(limit.min(4096)),
            limit,
            truncated: false,
        })
    }

    /// A fixed number of nonblocking reads prevents a streaming child from
    /// starving the process/deadline checks. The extra byte detects overflow.
    fn drain(&mut self) {
        let mut chunk = [0u8; 4096];
        for _ in 0..self.limit.div_ceil(chunk.len()).saturating_add(1) {
            let Some(reader) = self.reader.as_mut() else {
                break;
            };
            match reader.read(&mut chunk) {
                Ok(0) => {
                    self.reader = None;
                    break;
                }
                Ok(read) => {
                    let take = read.min(self.limit.saturating_sub(self.bytes.len()));
                    self.bytes.extend_from_slice(&chunk[..take]);
                    if take < read {
                        self.truncated = true;
                        self.reader = None;
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.truncated = true;
                    self.reader = None;
                    break;
                }
            }
        }
    }

    fn finish(mut self) -> (Vec<u8>, bool) {
        self.drain();
        // A setsid descendant can retain this descriptor indefinitely. Close
        // our end immediately and disclose that complete capture is unproven.
        self.truncated |= self.reader.is_some();
        (self.bytes, self.truncated)
    }
}

/// Terminate the owned child and its original command group on every return.
struct DiagnosticChild {
    child: std::process::Child,
    group: i32,
}
impl DiagnosticChild {
    fn new(child: std::process::Child) -> Self {
        let group = child.id() as i32;
        Self { child, group }
    }
    fn terminate_group(&mut self) {
        if self.group != 0 {
            unsafe {
                libc::kill(-self.group, libc::SIGKILL);
            }
            self.group = 0;
        }
        // The child may itself have left the original process group.
        let _ = self.child.kill();
    }
}
impl std::ops::Deref for DiagnosticChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl std::ops::DerefMut for DiagnosticChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
impl Drop for DiagnosticChild {
    fn drop(&mut self) {
        self.terminate_group();
        let _ = self.child.wait();
    }
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
/// is created component-by-component without following symlinks. Existing
/// artifacts are never overwritten and must be regular files with exact bytes.
/// The receipt still reports only bounded digests and lengths.
pub fn run_diagnostic_with_artifacts(
    request: &DiagnosticRequest,
    executable_path: &std::path::Path,
    artifact_dir: &std::path::Path,
) -> Result<(DiagnosticOutcome, Vec<std::path::PathBuf>), DiagnosticRunError> {
    run_diagnostic_inner(request, executable_path, Some(artifact_dir))
}

#[allow(unknown_lints)]
#[allow(
    ambient_clock,
    reason = "diagnostic shell enforces a child-process timeout on the host clock"
)]
fn host_now() -> Instant {
    Instant::now()
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

    let profile = chaoscontrol_protocol::diagnostic::observation_profile(request)
        .map_err(DiagnosticRunError::Observation)?;
    if profile.is_some() && !crate::is_in_vm() {
        return Err(DiagnosticRunError::Observation(
            "observation requires guest transport".into(),
        ));
    }
    let (executable_file, directory_file) = if let Some(profile) = &profile {
        if executable_path
            != std::path::Path::new("/")
                .join(&profile.executable)
                .as_path()
        {
            return Err(DiagnosticRunError::Observation(
                "supervisor path/profile mismatch".into(),
            ));
        }
        if observation_tick()? >= profile.deadline_tick {
            return Err(DiagnosticRunError::Observation(
                "observation deadline expired before execution".into(),
            ));
        }
        (
            open_observation_path(&profile.executable, false)?,
            Some(open_observation_path(&profile.working_directory, true)?),
        )
    } else {
        (
            std::fs::File::open(executable_path)
                .map_err(DiagnosticRunError::ExecutableUnreadable)?,
            None,
        )
    };
    let metadata = executable_file
        .metadata()
        .map_err(DiagnosticRunError::ExecutableUnreadable)?;
    if !metadata.is_file() || metadata.len() > MAX_EXECUTABLE_BYTES {
        return Err(DiagnosticRunError::ExecutableTooLarge {
            found: metadata.len(),
        });
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&executable_file)
        .take(MAX_EXECUTABLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(DiagnosticRunError::ExecutableUnreadable)?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > MAX_EXECUTABLE_BYTES {
        return Err(DiagnosticRunError::Observation(
            "executable size drift".into(),
        ));
    }
    let actual = *blake3::hash(&bytes).as_bytes();
    if actual != request.executable {
        return Err(DiagnosticRunError::ExecutableIdentityMismatch {
            expected: request.executable,
            actual,
        });
    }
    if profile.is_some() && !bytes.starts_with(b"\x7fELF") {
        return Err(DiagnosticRunError::Observation(
            "observation executable must be ELF, not a shell script".into(),
        ));
    }
    // A pinned inode alone remains writable by another guest process. Execute
    // an immutable copy of the exact hash-admitted bytes, never a later inode.
    let sealed_executable = if profile.is_some() {
        Some(seal_observation_executable(&bytes)?)
    } else {
        None
    };
    drop(bytes);

    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    let pinned_executable = format!(
        "/proc/self/fd/{}",
        sealed_executable
            .as_ref()
            .unwrap_or(&executable_file)
            .as_raw_fd()
    );
    let mut command = Command::new(if profile.is_some() {
        std::path::Path::new(&pinned_executable)
    } else {
        executable_path
    });
    command.args(&request.argv);
    if let Some(profile) = &profile {
        command.env_clear();
        command.envs(&profile.environment);
        command.current_dir(format!(
            "/proc/self/fd/{}",
            directory_file.as_ref().expect("opened cwd").as_raw_fd()
        ));
    }
    // Best-effort descendant cleanup; pipe collection never depends on group EOF.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let has_stdin = profile.is_none() && !request.stdin.is_empty();
    let mut child = DiagnosticChild::new(
        command
            .stdin(if has_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(DiagnosticRunError::Spawn)?,
    );
    let mut stdin = child.stdin.take();
    if let Some(pipe) = &stdin {
        nonblocking(pipe).map_err(DiagnosticRunError::Spawn)?;
    }
    let mut stdin_written = 0;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_limit = profile
        .as_ref()
        .map_or(MAX_CAPTURE_BYTES, |p| p.stdout_limit as usize);
    let stderr_limit = profile
        .as_ref()
        .map_or(MAX_CAPTURE_BYTES, |p| p.stderr_limit as usize);
    let mut stdout_reader =
        CapturePipe::new(stdout, stdout_limit).map_err(DiagnosticRunError::Spawn)?;
    let mut stderr_reader =
        CapturePipe::new(stderr, stderr_limit).map_err(DiagnosticRunError::Spawn)?;
    let deadline = profile
        .is_none()
        .then(|| host_now() + Duration::from_millis(u64::from(request.timeout_ms)));
    let mut timed_out = false;
    let status: ExitStatus = loop {
        stdout_reader.drain();
        stderr_reader.drain();
        if let Some(pipe) = stdin.as_mut() {
            match pipe.write(&request.stdin[stdin_written..]) {
                Ok(written) => {
                    stdin_written += written;
                    if stdin_written == request.stdin.len() {
                        stdin = None;
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(DiagnosticRunError::Spawn(error)),
            }
        }
        match child.try_wait().map_err(DiagnosticRunError::Wait)? {
            Some(status) => {
                if let Some(profile) = &profile {
                    timed_out = observation_tick()? >= profile.deadline_tick;
                }
                break status;
            }
            None if match &profile {
                Some(profile) => observation_tick()? >= profile.deadline_tick,
                None => deadline.is_some_and(|deadline| host_now() >= deadline),
            } =>
            {
                timed_out = true;
                child.terminate_group();
                break child.wait().map_err(DiagnosticRunError::Wait)?;
            }
            None => std::thread::yield_now(),
        }
    };

    child.terminate_group();
    drop(stdin);
    let (stdout_bytes, stdout_truncated) = stdout_reader.finish();
    let (stderr_bytes, stderr_truncated) = stderr_reader.finish();
    let mut artifacts = Vec::new();
    if let Some(directory) = artifact_dir {
        let pinned = open_artifact_directory(directory).map_err(DiagnosticRunError::Artifact)?;
        for (suffix, bytes) in [("stdout", &stdout_bytes), ("stderr", &stderr_bytes)] {
            if bytes.is_empty() {
                continue;
            }
            let name = format!("blake3-{}.{}", blake3::hash(bytes).to_hex(), suffix);
            publish_artifact(&pinned, &name, bytes).map_err(DiagnosticRunError::Artifact)?;
            artifacts.push(directory.join(name));
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

/// Pin every directory component before creating anything beneath it. Neither
/// an ancestor substitution nor a final symlink can redirect publication.
fn open_artifact_directory(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Component;

    let flags = libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(if path.is_absolute() { "/" } else { "." })?;
    for part in path.components() {
        let name = match part {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => std::ffi::CString::new(name.as_bytes())?,
            _ => {
                return Err(std::io::Error::other(
                    "artifact parent traversal is forbidden",
                ))
            }
        };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        let fd = if fd >= 0 {
            fd
        } else {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error);
            }
            if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(error);
                }
            }
            let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            fd
        };
        directory = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    Ok(directory)
}

/// Never open an existing destination for writing: symlinks, FIFOs, devices,
/// and hardlinks cannot turn a content-addressed artifact into an overwrite.
fn publish_artifact(directory: &std::fs::File, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::fd::{AsRawFd, FromRawFd};
    if bytes.len() > MAX_CAPTURE_BYTES {
        return Err(std::io::Error::other("artifact exceeds capture bound"));
    }
    let name = std::ffi::CString::new(name)?;
    let flags = libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
    };
    if fd >= 0 {
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        return file.write_all(bytes);
    }
    let error = std::io::Error::last_os_error();
    if error.kind() != std::io::ErrorKind::AlreadyExists {
        return Err(error);
    }
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let pinned = unsafe { std::fs::File::from_raw_fd(fd) };
    let metadata = pinned.metadata()?;
    if !metadata.is_file() || metadata.len() != bytes.len() as u64 {
        return Err(std::io::Error::other(
            "existing artifact is not the admitted regular file",
        ));
    }
    // Only reopen the pinned regular inode, never a special file or a raced
    // directory entry. The procfs descriptor link is owned by this process.
    let mut file = std::fs::File::open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))?;
    let mut chunk = [0u8; 4096];
    for expected in bytes.chunks(chunk.len()) {
        file.read_exact(&mut chunk[..expected.len()])?;
        if expected != &chunk[..expected.len()] {
            return Err(std::io::Error::other("existing artifact content differs"));
        }
    }
    if file.read(&mut chunk[..1])? != 0 {
        return Err(std::io::Error::other(
            "existing artifact grew beyond its bound",
        ));
    }
    Ok(())
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
            counterfactual: true,
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

fn observation_tick() -> Result<u64, DiagnosticRunError> {
    let (tick, status) =
        crate::transport::hypercall_simple(chaoscontrol_protocol::CMD_DIAGNOSTIC_TICK, 0);
    if status != chaoscontrol_protocol::STATUS_OK {
        return Err(DiagnosticRunError::Observation(
            "controller tick unavailable".into(),
        ));
    }
    Ok(tick)
}

fn seal_observation_executable(bytes: &[u8]) -> Result<std::fs::File, DiagnosticRunError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let fd = unsafe {
        libc::memfd_create(
            c"chaoscontrol-observation".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd < 0 {
        return Err(DiagnosticRunError::ExecutableUnreadable(
            std::io::Error::last_os_error(),
        ));
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(bytes)
        .map_err(DiagnosticRunError::ExecutableUnreadable)?;
    let seals = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } < 0 {
        return Err(DiagnosticRunError::ExecutableUnreadable(
            std::io::Error::last_os_error(),
        ));
    }
    Ok(file)
}

/// Open every component relative to a pinned guest root, without symlinks.
fn open_observation_path(path: &str, directory: bool) -> Result<std::fs::File, DiagnosticRunError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;
    chaoscontrol_protocol::diagnostic::admit_observation_path(path, directory)
        .map_err(DiagnosticRunError::Observation)?;
    let mut current = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(DiagnosticRunError::ExecutableUnreadable)?;
    if path == "." {
        return Ok(current);
    }
    let mut parts = path.split('/').peekable();
    let mut name = [0u8; chaoscontrol_protocol::diagnostic::MAX_OBSERVATION_PATH_BYTES + 1];
    while let Some(part) = parts.next() {
        name[..part.len()].copy_from_slice(part.as_bytes());
        name[part.len()] = 0;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if directory || parts.peek().is_some() {
                libc::O_DIRECTORY
            } else {
                0
            };
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr().cast(), flags) };
        if fd < 0 {
            return Err(DiagnosticRunError::ExecutableUnreadable(
                std::io::Error::last_os_error(),
            ));
        }
        current = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    Ok(current)
}

/// Poll once and require an exact guest-retained profile before any command effect.
pub fn serve_one_declared_diagnostic(
    legacy_executable: Option<&std::path::Path>,
    profiles: &[chaoscontrol_protocol::diagnostic::ObservationProfile],
) -> Result<Option<DiagnosticReceipt>, String> {
    let Some(request) = poll_diagnostic_request()? else {
        return Ok(None);
    };
    let profile = chaoscontrol_protocol::diagnostic::observation_profile(&request)?;
    let executable = if let Some(profile) = &profile {
        if !profiles.contains(profile) {
            return Err("unadmitted observation profile".into());
        }
        std::path::Path::new("/").join(&profile.executable)
    } else {
        legacy_executable
            .ok_or("missing admitted diagnostic executable")?
            .to_path_buf()
    };
    let outcome = run_diagnostic(&request, &executable).map_err(|e| format!("{e:?}"))?;
    let receipt = outcome.receipt();
    report_diagnostic_receipt(&receipt)?;
    Ok(Some(receipt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_executable_snapshot_cannot_drift_after_hashing() {
        use std::os::unix::fs::FileExt;
        let mut source = b"admitted ELF bytes".to_vec();
        let mut sealed = seal_observation_executable(&source).unwrap();
        source.fill(b'x');
        assert!(sealed.write_all(b"changed").is_err());
        assert!(sealed.write_at(b"changed", 0).is_err());
        let mut retained = [0; 18];
        assert_eq!(sealed.read_at(&mut retained, 0).unwrap(), 18);
        assert_eq!(&retained, b"admitted ELF bytes");
    }

    fn observation() -> chaoscontrol_protocol::diagnostic::ObservationProfile {
        chaoscontrol_protocol::diagnostic::ObservationProfile {
            executable: "init".into(),
            executable_blake3: [1; 32],
            argv: vec!["--observe-shared-state".into()],
            environment: Default::default(),
            working_directory: "data".into(),
            deadline_tick: 5000,
            stdout_limit: 1024,
            stderr_limit: 1024,
            mutates_guest: false,
        }
    }

    // r[verify chaoscontrol.debug_query.guest_observation]
    // r[verify chaoscontrol.debug_query.fixtures]
    #[test]
    fn declared_observation_never_executes_on_host() {
        let request = observation().request().unwrap();
        assert!(
            matches!(run_diagnostic(&request, std::path::Path::new("/init")),
            Err(DiagnosticRunError::Observation(reason)) if reason.contains("guest transport"))
        );
    }

    #[test]
    fn observation_capture_marks_overflow_without_unbounded_output() {
        use std::os::unix::net::UnixStream;
        for (data, expected_overflow) in [(b"12345".as_slice(), true), (b"1234".as_slice(), false)]
        {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            writer.write_all(data).unwrap();
            drop(writer);
            let (bytes, overflow) = CapturePipe::new(reader, 4).unwrap().finish();
            assert_eq!(bytes, b"1234");
            assert_eq!(overflow, expected_overflow);
        }
    }

    #[test]
    fn capture_closes_owned_pipe_without_waiting_for_retained_writer() {
        use std::os::unix::net::UnixStream;
        let (reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(b"bounded").unwrap();
        let started = Instant::now();
        let (bytes, truncated) = CapturePipe::new(reader, MAX_CAPTURE_BYTES)
            .unwrap()
            .finish();
        assert_eq!(bytes, b"bounded");
        assert!(truncated);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(writer.write_all(b"closed").is_err());
    }

    #[test]
    fn observation_paths_reject_symlink_components_and_missing_files() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("cc-observation-paths-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("real")).unwrap();
        std::fs::write(root.join("real/file"), b"retained").unwrap();
        symlink(root.join("real"), root.join("linked")).unwrap();
        symlink(root.join("real/file"), root.join("file-link")).unwrap();
        let relative = root.strip_prefix("/").unwrap().to_str().unwrap();
        assert!(open_observation_path(&format!("{relative}/real/file"), false).is_ok());
        assert!(open_observation_path(&format!("{relative}/linked/file"), false).is_err());
        assert!(open_observation_path(&format!("{relative}/file-link"), false).is_err());
        assert!(open_observation_path(&format!("{relative}/missing"), false).is_err());
        assert!(open_observation_path(&format!("{relative}/linked"), true).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

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
        assert!(receipt.counterfactual);
    }

    #[test]
    fn identity_mismatch_is_rejected_before_spawn() {
        let path = std::path::Path::new("/bin/echo");
        if !path.exists() {
            return;
        }
        let mut request = request_for(path, &["hello"], 100);
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
            counterfactual: true,
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
    fn artifact_publication_rejects_links_fifos_and_content_drift() {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let directory = open_artifact_directory(&root.path().join("artifacts")).unwrap();
        let artifacts = root.path().join("artifacts");
        let victim = root.path().join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        symlink(&victim, artifacts.join("symlink")).unwrap();
        let fifo = std::ffi::CString::new(artifacts.join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        std::fs::hard_link(&victim, artifacts.join("hardlink")).unwrap();
        std::fs::create_dir(artifacts.join("directory")).unwrap();
        let started = Instant::now();
        for name in ["symlink", "fifo", "hardlink", "directory"] {
            assert!(publish_artifact(&directory, name, b"replacement").is_err());
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
        publish_artifact(&directory, "regular", b"retained").unwrap();
        publish_artifact(&directory, "regular", b"retained").unwrap();
        assert!(publish_artifact(&directory, "regular", b"modified").is_err());
        assert_eq!(
            std::fs::read(artifacts.join("regular")).unwrap(),
            b"retained"
        );
        assert!(
            publish_artifact(&directory, "oversized", &vec![0; MAX_CAPTURE_BYTES + 1]).is_err()
        );
    }

    #[test]
    fn artifact_publication_stays_in_pinned_no_follow_directory() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let admitted = root.path().join("admitted");
        let moved = root.path().join("moved");
        let substitute = root.path().join("substitute");
        std::fs::create_dir(&substitute).unwrap();
        let directory = open_artifact_directory(&admitted).unwrap();
        std::fs::rename(&admitted, &moved).unwrap();
        symlink(&substitute, &admitted).unwrap();
        assert!(open_artifact_directory(&admitted).is_err());
        assert!(open_artifact_directory(&admitted.join("child")).is_err());
        assert!(open_artifact_directory(&root.path().join("..")).is_err());
        publish_artifact(&directory, "capture", b"pinned").unwrap();
        assert_eq!(std::fs::read(moved.join("capture")).unwrap(), b"pinned");
        assert!(!substitute.join("capture").exists());
    }

    #[test]
    fn public_artifact_runner_never_overwrites_predictable_symlink() {
        use std::os::unix::fs::symlink;
        let path = std::path::Path::new("/bin/echo");
        if !path.exists() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let victim = root.path().join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        let artifact = format!("blake3-{}.stdout", blake3::hash(b"artifact\n").to_hex());
        symlink(&victim, root.path().join(artifact)).unwrap();
        let request = request_for(path, &["artifact"], 1_000);
        assert!(matches!(
            run_diagnostic_with_artifacts(&request, path, root.path()),
            Err(DiagnosticRunError::Artifact(_))
        ));
        assert_eq!(std::fs::read(victim).unwrap(), b"untouched");
    }

    #[test]
    fn setsid_descendant_retaining_pipes_does_not_delay_runner() {
        let shell = std::path::Path::new("/bin/sh");
        let setsid = ["/usr/bin/setsid", "/bin/setsid"]
            .into_iter()
            .find(|path| std::path::Path::new(path).exists());
        let Some(setsid) = setsid.filter(|_| shell.exists()) else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let pid_file = root.path().join("escaped-pid");
        let script = format!(
            "{setsid} /bin/sh -c 'echo $$ > \"$1\"; sleep 3' retained '{}' & while [ ! -s '{}' ]; do sleep 0.01; done",
            pid_file.display(), pid_file.display()
        );
        let request = request_for(shell, &["-c", &script], 1_000);
        let started = Instant::now();
        let result = run_diagnostic(&request, shell);
        // The escaped group is fixture-owned; cleanup is independent of the
        // runner result, and the fixture also has a finite sleep fallback.
        if let Ok(pid) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid.trim().parse::<i32>() {
                if pid > 1 {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
            }
        }
        let outcome = result.unwrap();
        assert_eq!(outcome.status, DiagnosticStatus::Completed);
        assert!(outcome.truncated);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn stdin_is_delivered_to_the_process() {
        let path = std::path::Path::new("/bin/cat");
        if !path.exists() {
            return;
        }
        let mut request = request_for(path, &["-"], 1_000);
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
