use chaoscontrol_protocol::guest_process::{
    admit_manifest, ProcessLifecycleEvent, ProcessManifest, MAX_PATH_BYTES,
};
use chaoscontrol_sdk::prelude::*;
use chaoscontrol_sdk::supervisor::{StdProcessRuntime, Supervisor};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

const DEFAULT_MANIFEST: &str = "/etc/chaoscontrol/process-manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_FIXTURE_STATE_BYTES: u64 = 1024 * 1024;
const SUPERVISOR_POLL_MILLIS: u64 = 1;
const FIXTURE_WORKER_MODE: &str = "--fixture-worker";
const FIXTURE_SHARED_FILE_NAME: &str = "cooperating-state";
const DEFAULT_FIXTURE_DATA_DIRECTORY: &str = "/data";
const FIXTURE_COMMIT_MARKER: &str = ".writer-committed-once";
const FIXTURE_EXIT_MARKER: &str = ".writer-exit-once";
const FIXTURE_EXIT_STATUS: i32 = 23;

fn main() {
    let result = if std::env::args().nth(1).as_deref() == Some(FIXTURE_WORKER_MODE) {
        run_fixture_worker()
    } else {
        run()
    };
    if let Err(error) = result {
        eprintln!("chaoscontrol guest supervisor failed: {error}");
        std::process::exit(1);
    }
}

fn run_fixture_worker() -> Result<(), String> {
    let role = std::env::var("CHAOSCONTROL_PROCESS_ROLE")
        .map_err(|_| "fixture worker requires process role".to_string())?;
    guest_init();
    let data_directory =
        std::env::var("DATA_DIR").unwrap_or_else(|_| DEFAULT_FIXTURE_DATA_DIRECTORY.to_string());
    let shared_file = Path::new(&data_directory).join(FIXTURE_SHARED_FILE_NAME);
    let commit_marker = Path::new(&data_directory).join(FIXTURE_COMMIT_MARKER);
    let mut exit_after_ready = false;
    match role.as_str() {
        "writer" => {
            write_data_file(&shared_file, b"committed\n").map_err(|error| error.to_string())?;
            if std::env::var("CHAOSCONTROL_FIXTURE_LOSE_SHARED_STATE").as_deref() == Ok("1") {
                std::fs::remove_file(&shared_file).map_err(|error| error.to_string())?;
            }
            write_data_file(&commit_marker, b"writer committed\n")
                .map_err(|error| error.to_string())?;
            if std::env::var("CHAOSCONTROL_FIXTURE_WRITER_EXIT_ONCE").as_deref() == Ok("1") {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(Path::new(&data_directory).join(FIXTURE_EXIT_MARKER))
                {
                    Ok(marker) => {
                        marker.sync_all().map_err(|error| error.to_string())?;
                        exit_after_ready = true;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            cc_assert_reachable_stable!(
                "org.onixresearch.chaoscontrol.multiprocess",
                "writer-shared-state",
                "writer",
                "multiprocess",
                "writer published shared state",
            );
        }
        "checkpoint" => {
            while !commit_marker.is_file() {
                std::thread::yield_now();
            }
            let state =
                read_bounded_regular(&shared_file, MAX_FIXTURE_STATE_BYTES, "fixture state");
            cc_assert_always_stable!(
                "org.onixresearch.chaoscontrol.multiprocess",
                "checkpoint-observes-state",
                "checkpoint",
                "multiprocess",
                state.as_deref().is_ok_and(|bytes| bytes == b"committed\n"),
                "checkpoint observed the writer state",
            );
        }
        _ => return Err(format!("unsupported fixture worker role: {role}")),
    }
    send_event(
        "multiprocess_fixture_ready",
        &serde_json::json!({"role": role, "claim_scope": "declared-processes-only"}),
    );
    if exit_after_ready {
        std::process::exit(FIXTURE_EXIT_STATUS);
    }
    loop {
        unsafe { libc::pause() };
    }
}

fn run() -> Result<(), String> {
    let manifest_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_MANIFEST.to_string());
    if manifest_path.len() > MAX_PATH_BYTES || !Path::new(&manifest_path).is_absolute() {
        return Err("manifest path must be a bounded absolute path".to_string());
    }
    let bytes = read_manifest(Path::new(&manifest_path))?;
    let manifest: ProcessManifest =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let admitted = admit_manifest(&manifest).map_err(|error| format!("{error:?}"))?;
    let process_count = admitted.processes.len();
    let manifest_identity = admitted.manifest_identity.clone();
    let shared_storage = chaoscontrol_sdk::supervisor::prepare_guest_shared_storage(&admitted)
        .map_err(|error| format!("shared storage admission: {error:?}"))?;

    guest_init();
    let mut supervisor = Supervisor::new(admitted, StdProcessRuntime::default());
    supervisor.start().map_err(|error| format!("{error:?}"))?;
    emit_events(supervisor.drain_events());
    let setup_details = serde_json::json!({
        "manifest_identity": manifest_identity,
        "declared_process_count": process_count,
        "claim_scope": "declared-processes-only",
        "shared_storage": shared_storage,
    });
    send_event("guest_supervisor_setup", &setup_details);
    setup_complete(&setup_details);

    loop {
        if let Some(command) = chaoscontrol_sdk::process::poll_fault()? {
            supervisor
                .apply_fault(&command)
                .map_err(|error| format!("{error:?}"))?;
        }
        supervisor.monitor().map_err(|error| format!("{error:?}"))?;
        supervisor
            .advance_tick()
            .map_err(|error| format!("{error:?}"))?;
        emit_events(supervisor.drain_events());
        std::thread::sleep(Duration::from_millis(SUPERVISOR_POLL_MILLIS));
    }
}

fn emit_events(events: Vec<ProcessLifecycleEvent>) {
    for event in events {
        send_event(
            "guest_process_lifecycle",
            &serde_json::to_value(event).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
}

fn read_manifest(path: &Path) -> Result<Vec<u8>, String> {
    read_bounded_regular(path, MAX_MANIFEST_BYTES, "manifest")
}

fn read_bounded_regular(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(format!("{label} must be a bounded regular file"));
    }
    let capacity = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("{label} must be a bounded regular file"));
    }
    Ok(bytes)
}

fn write_data_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn manifest_reads_reject_symlinks_and_fifos() {
        let root = std::env::temp_dir().join(format!(
            "chaoscontrol-supervisor-manifest-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target.json");
        std::fs::write(&target, b"{}").unwrap();
        let link = root.join("manifest.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_manifest(&link).is_err());
        assert!(read_bounded_regular(&link, MAX_FIXTURE_STATE_BYTES, "fixture state").is_err());
        assert!(read_manifest(&root).is_err());

        let fifo = root.join("manifest.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_manifest(&fifo).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fixture_writes_reject_symlinks_and_fifos() {
        let root = std::env::temp_dir().join(format!(
            "chaoscontrol-guest-supervisor-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let link = root.join("state");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_data_file(&link, b"replacement").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");

        let fifo = root.join("state.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(write_data_file(&fifo, b"replacement").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
