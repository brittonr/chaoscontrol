use chaoscontrol_protocol::guest_process::{
    admit_manifest, ProcessManifest, ProcessSpec, RestartMode, RestartPolicy, SharedDeviceKind,
    SharedDirectorySpec, PROCESS_MANIFEST_SCHEMA,
};
use chaoscontrol_protocol::process::{ProcessFaultAction, ProcessFaultCommand};
use chaoscontrol_sdk::supervisor::{StdProcessRuntime, Supervisor};
use std::collections::BTreeMap;
use std::time::Duration;

const RESTART_LIMIT: u32 = 2;
const WAIT_ATTEMPTS: usize = 100;
const WAIT_MILLIS: u64 = 5;
const SHARED_FILE_NAME: &str = "cooperating-state";

#[test]
fn real_children_share_state_and_survive_one_process_restart() {
    let root = std::env::temp_dir().join(format!(
        "chaoscontrol-multiprocess-shell-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let executable = env!("CARGO_BIN_EXE_chaoscontrol-guest-supervisor").to_string();
    let process = |role: &str, slot: u16| ProcessSpec {
        role: role.to_string(),
        executable: executable.clone(),
        arguments: vec!["--fixture-worker".to_string()],
        environment: BTreeMap::from([("DATA_DIR".to_string(), root.display().to_string())]),
        shared_directories: vec!["data".to_string()],
        restart: RestartPolicy {
            mode: RestartMode::Never,
            max_restarts: RESTART_LIMIT,
        },
        instrumented: true,
        transport_slot: Some(slot),
    };
    let manifest = admit_manifest(&ProcessManifest {
        schema: PROCESS_MANIFEST_SCHEMA.to_string(),
        guest: "shell-fixture".to_string(),
        shared_directories: vec![SharedDirectorySpec {
            id: "data".to_string(),
            path: root.display().to_string(),
            device: SharedDeviceKind::Memory,
        }],
        processes: vec![process("writer", 0), process("checkpoint", 1)],
    })
    .unwrap();
    let mut supervisor = Supervisor::new(manifest, StdProcessRuntime::default());
    supervisor.start().unwrap();

    let shared_file = root.join(SHARED_FILE_NAME);
    for _ in 0..WAIT_ATTEMPTS {
        if shared_file.is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(WAIT_MILLIS));
    }
    assert_eq!(std::fs::read(&shared_file).unwrap(), b"committed\n");

    let restart = ProcessFaultCommand::new(
        "restart-writer",
        "writer",
        ProcessFaultAction::Restart,
        None,
    )
    .unwrap();
    supervisor.apply_fault(&restart).unwrap();
    assert_eq!(std::fs::read(&shared_file).unwrap(), b"committed\n");

    for role in ["writer", "checkpoint"] {
        let kill =
            ProcessFaultCommand::new(format!("kill-{role}"), role, ProcessFaultAction::Kill, None)
                .unwrap();
        supervisor.apply_fault(&kill).unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}
