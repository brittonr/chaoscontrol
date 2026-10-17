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
    supervisor.drain_events();

    for role in ["writer", "checkpoint"] {
        let kill =
            ProcessFaultCommand::new(format!("kill-{role}"), role, ProcessFaultAction::Kill, None)
                .unwrap();
        supervisor.apply_fault(&kill).unwrap();
        supervisor.monitor().unwrap();
        let events = supervisor.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].role, role);
        assert_eq!(
            events[0].kind,
            chaoscontrol_protocol::guest_process::LifecycleKind::Killed
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn actual_child_exit_restarts_once_with_identical_lifecycle_at_fixed_tick() {
    use chaoscontrol_protocol::guest_process::{LifecycleKind, ProcessStatus};
    let mut replays = Vec::new();
    for replay in 0..2 {
        let root = std::env::temp_dir().join(format!(
            "chaoscontrol-exit-restart-{}-{replay}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let manifest = admit_manifest(&ProcessManifest {
            schema: PROCESS_MANIFEST_SCHEMA.to_string(),
            guest: "exit-restart".to_string(),
            shared_directories: vec![SharedDirectorySpec {
                id: "data".to_string(),
                path: root.display().to_string(),
                device: SharedDeviceKind::Memory,
            }],
            processes: vec![ProcessSpec {
                role: "writer".to_string(),
                executable: env!("CARGO_BIN_EXE_chaoscontrol-guest-supervisor").to_string(),
                arguments: vec!["--fixture-worker".to_string()],
                environment: BTreeMap::from([
                    ("DATA_DIR".to_string(), root.display().to_string()),
                    (
                        "CHAOSCONTROL_FIXTURE_WRITER_EXIT_ONCE".to_string(),
                        "1".to_string(),
                    ),
                ]),
                shared_directories: vec!["data".to_string()],
                restart: RestartPolicy {
                    mode: RestartMode::OnFailure,
                    max_restarts: RESTART_LIMIT,
                },
                instrumented: true,
                transport_slot: Some(0),
            }],
        })
        .unwrap();
        let identity = manifest.processes[0].identity.clone();
        let mut supervisor = Supervisor::new(manifest, StdProcessRuntime::default());
        supervisor.start().unwrap();
        let initial_generation = supervisor.state().processes[&identity].generation;
        let fixed_tick = supervisor.state().tick;
        for _ in 0..WAIT_ATTEMPTS {
            supervisor.monitor().unwrap();
            if supervisor.state().processes[&identity].restart_count == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(WAIT_MILLIS));
        }
        let process = &supervisor.state().processes[&identity];
        assert_eq!(process.generation, initial_generation + 1);
        assert_eq!(process.restart_count, 1);
        assert_eq!(process.status, ProcessStatus::Running);
        assert_eq!(supervisor.state().tick, fixed_tick);
        assert_eq!(
            std::fs::read(root.join(SHARED_FILE_NAME)).unwrap(),
            b"committed\n"
        );
        let events = supervisor.drain_events();
        assert_eq!(
            events.iter().map(|event| event.kind).collect::<Vec<_>>(),
            [
                LifecycleKind::Spawned,
                LifecycleKind::Exited,
                LifecycleKind::Restarted
            ]
        );
        assert!(events.iter().all(|event| event.tick == fixed_tick));
        replays.push(events);
        supervisor
            .apply_fault(
                &ProcessFaultCommand::new("stop", "writer", ProcessFaultAction::Kill, None)
                    .unwrap(),
            )
            .unwrap();
        supervisor.monitor().unwrap();
        assert_eq!(
            supervisor.state().processes[&identity].status,
            ProcessStatus::Exited
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    assert_eq!(replays[0], replays[1]);
}

#[test]
fn shared_device_mounts_are_refused_outside_guest_pid_one_without_filesystem_effects() {
    let root = std::env::temp_dir().join(format!(
        "chaoscontrol-host-mount-refusal-{}",
        std::process::id()
    ));
    assert!(!root.exists());
    let manifest = admit_manifest(&ProcessManifest {
        schema: PROCESS_MANIFEST_SCHEMA.to_string(),
        guest: "host-refusal".to_string(),
        shared_directories: vec![SharedDirectorySpec {
            id: "data".to_string(),
            path: root.display().to_string(),
            device: SharedDeviceKind::Memory,
        }],
        processes: Vec::new(),
    })
    .unwrap();
    assert!(matches!(
        chaoscontrol_sdk::supervisor::prepare_guest_shared_storage(&manifest),
        Err(chaoscontrol_sdk::supervisor::RuntimeError::GuestInitRequired)
    ));
    assert!(!root.exists());
}
