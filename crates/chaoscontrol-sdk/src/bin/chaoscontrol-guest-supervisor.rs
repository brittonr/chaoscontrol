use chaoscontrol_protocol::guest_process::{
    admit_manifest, ProcessLifecycleEvent, ProcessManifest, MAX_PATH_BYTES,
};
use chaoscontrol_sdk::prelude::*;
use chaoscontrol_sdk::supervisor::{StdProcessRuntime, Supervisor};
use std::path::Path;
use std::time::Duration;

const DEFAULT_MANIFEST: &str = "/etc/chaoscontrol/process-manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const SUPERVISOR_POLL_MILLIS: u64 = 1;
const FIXTURE_WORKER_MODE: &str = "--fixture-worker";
const FIXTURE_SHARED_FILE_NAME: &str = "cooperating-state";
const DEFAULT_FIXTURE_DATA_DIRECTORY: &str = "/data";

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
    match role.as_str() {
        "writer" => {
            std::fs::write(&shared_file, b"committed\n").map_err(|error| error.to_string())?;
            cc_assert_reachable_stable!(
                "org.onixresearch.chaoscontrol.multiprocess",
                "writer-shared-state",
                "writer",
                "multiprocess",
                "writer published shared state",
            );
        }
        "checkpoint" => {
            while !shared_file.is_file() {
                std::thread::yield_now();
            }
            let state = std::fs::read(&shared_file).map_err(|error| error.to_string())?;
            cc_assert_always_stable!(
                "org.onixresearch.chaoscontrol.multiprocess",
                "checkpoint-observes-state",
                "checkpoint",
                "multiprocess",
                state == b"committed\n",
                "checkpoint observed the writer state",
            );
        }
        _ => return Err(format!("unsupported fixture worker role: {role}")),
    }
    send_event(
        "multiprocess_fixture_ready",
        &serde_json::json!({"role": role, "claim_scope": "declared-processes-only"}),
    );
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
    let metadata = std::fs::symlink_metadata(&manifest_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err("manifest must be a bounded regular file".to_string());
    }
    let bytes = std::fs::read(&manifest_path).map_err(|error| error.to_string())?;
    let manifest: ProcessManifest =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let admitted = admit_manifest(&manifest).map_err(|error| format!("{error:?}"))?;
    let process_count = admitted.processes.len();
    let manifest_identity = admitted.manifest_identity.clone();

    guest_init();
    let mut supervisor = Supervisor::new(admitted, StdProcessRuntime::default());
    supervisor.start().map_err(|error| format!("{error:?}"))?;
    emit_events(supervisor.drain_events());
    setup_complete(&serde_json::json!({
        "manifest_identity": manifest_identity,
        "declared_process_count": process_count,
        "claim_scope": "declared-processes-only",
    }));

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
