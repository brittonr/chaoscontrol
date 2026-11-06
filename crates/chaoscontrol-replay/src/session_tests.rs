use super::*;
use chaoscontrol_explore::snapshot_store::{decode_snapshot_artifact, encode_snapshot_artifact};
use chaoscontrol_fault::schedule::FaultSchedule;

const FIXTURE_MEMORY_BYTES: usize = 4096;
const FIXTURE_SEED: u64 = 42;
const FIXTURE_END_TICK: u64 = 20;

fn configuration() -> SimulationConfig {
    SimulationConfig {
        num_vms: 1,
        kernel_path: "kernel".into(),
        seed: FIXTURE_SEED,
        quantum: 10,
        random_faults: false,
        vm_config: VmConfig {
            memory_size: FIXTURE_MEMORY_BYTES,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn recording() -> Recording {
    let config = configuration();
    let mut recording = Recorder::new(
        crate::recording::RecordingConfig {
            num_vms: 1,
            vm_memory_size: FIXTURE_MEMORY_BYTES,
            tsc_khz: config.vm_config.cpu.tsc_khz,
            kernel_path: config.kernel_path,
            initrd_path: None,
            quantum: config.quantum,
            checkpoint_interval: 5,
            disk_image_path: None,
            random_faults: false,
        },
        FaultSchedule::new(),
        FIXTURE_SEED,
    )
    .recording()
    .clone();
    recording.total_ticks = FIXTURE_END_TICK;
    recording.session_id = "fixture-session".into();
    recording.timestamp = 0;
    recording
}
fn artifact(path: &str, bytes: &[u8]) -> SessionArtifact {
    SessionArtifact {
        path: path.into(),
        digest: digest_bytes(bytes),
        bytes: bytes.len() as u64,
    }
}
fn fixture_manifest() -> SessionManifest {
    let config = SessionConfiguration::capture(&configuration()).unwrap();
    let snapshot_path = format!("snapshots/{}.snapshot.bin", "0".repeat(64));
    let mut manifest = SessionManifest {
        schema_version: SESSION_SCHEMA_VERSION,
        session_id: String::new(),
        config_identity: digest_json(&config).unwrap(),
        configuration: config,
        recording: artifact("recording.json", &serde_json::to_vec(&recording()).unwrap()),
        guest_artifacts: vec![GuestArtifact {
            role: GuestArtifactRole::Kernel,
            artifact: artifact("artifacts/kernel.bin", b"kernel"),
        }],
        checkpoints: vec![SessionCheckpoint {
            id: 0,
            tick: 10,
            artifact: artifact(&snapshot_path, b"not a snapshot"),
            snapshot_ref: ReplayParentSnapshotRef {
                store: chaoscontrol_explore::snapshot_store::FILE_STORE_KIND.into(),
                digest: format!("sha256:{}", "0".repeat(64)),
                codec: chaoscontrol_explore::snapshot_store::SNAPSHOT_CODEC.into(),
                schema_version: chaoscontrol_explore::snapshot_store::SNAPSHOT_SCHEMA_VERSION,
                path: snapshot_path,
            },
            serial_output: Vec::new(),
            events_since_last: Vec::new(),
        }],
        branches: vec![SessionBranch {
            branch_id: digest_bytes(b"root"),
            parent: None,
            checkpoint_id: 0,
            tick: 10,
        }],
        retained_start_tick: 0,
        retained_end_tick: FIXTURE_END_TICK,
        limits: SessionLimits::default(),
    };
    resign(&mut manifest);
    manifest
}
fn resign(manifest: &mut SessionManifest) {
    manifest.session_id = manifest_identity(manifest).unwrap();
}
fn retained_fixture(root: &File, manifest: &SessionManifest, include_snapshot: bool) {
    mkdir_child(root, "artifacts").unwrap();
    mkdir_child(root, "snapshots").unwrap();
    write_artifact(
        root,
        "recording.json",
        &serde_json::to_vec(&recording()).unwrap(),
    )
    .unwrap();
    write_artifact(root, "artifacts/kernel.bin", b"kernel").unwrap();
    if include_snapshot {
        write_artifact(
            root,
            &manifest.checkpoints[0].artifact.path,
            b"not a snapshot",
        )
        .unwrap();
    }
    write_artifact(root, MANIFEST_NAME, &serde_json::to_vec(manifest).unwrap()).unwrap();
}

// r[verify chaoscontrol.debug_query.moment]
// r[verify chaoscontrol.debug_query.fixtures]
#[test]
fn manifest_admission_is_deterministic_but_does_not_admit_exact_execution() {
    let manifest = fixture_manifest();
    validate_manifest(&manifest, SessionLimits::default()).unwrap();
    assert_eq!(
        manifest_identity(&manifest).unwrap(),
        manifest_identity(&manifest).unwrap()
    );
    let directory = tempfile::tempdir().unwrap();
    retained_fixture(&open_directory(directory.path()).unwrap(), &manifest, true);
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::Corrupt(_))
    ));
}
#[test]
fn absent_session_and_metadata_only_recording_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    assert!(matches!(
        load_session(&directory.path().join("absent"), SessionLimits::default()),
        Err(SessionError::MissingSession)
    ));
    std::fs::write(
        directory.path().join("recording.json"),
        serde_json::to_vec(&recording()).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::MissingSession)
    ));
    let metadata = Checkpoint {
        id: 0,
        tick: 10,
        snapshot: None,
        serial_output: Vec::new(),
        events_since_last: Vec::new(),
    };
    let config = SessionConfiguration::capture(&configuration()).unwrap();
    assert!(matches!(
        validate_checkpoint(&recording(), &metadata, &config),
        Err(SessionError::Corrupt(_))
    ));
    assert!(!directory.path().join("session").exists());
}
#[test]
fn missing_checkpoint_artifact_is_not_inferred_from_recording() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = fixture_manifest();
    retained_fixture(&open_directory(directory.path()).unwrap(), &manifest, false);
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::MissingArtifact(_))
    ));
}
#[test]
fn recording_and_manifest_source_drift_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = fixture_manifest();
    retained_fixture(&open_directory(directory.path()).unwrap(), &manifest, false);
    std::fs::write(directory.path().join("recording.json"), b"{}").unwrap();
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::Drift(_))
    ));
    let mut changed = manifest.clone();
    changed.configuration.quantum += 1;
    assert!(matches!(
        validate_manifest(&changed, SessionLimits::default()),
        Err(SessionError::Corrupt(_))
    ));
}
#[test]
fn incompatible_schema_config_checkpoint_and_parentage_are_rejected() {
    let original = fixture_manifest();
    let mut changed = original.clone();
    changed.schema_version += 1;
    resign(&mut changed);
    assert!(matches!(
        validate_manifest(&changed, SessionLimits::default()),
        Err(SessionError::Incompatible(_))
    ));
    let mut changed = original.clone();
    changed.checkpoints[0].tick = FIXTURE_END_TICK + 1;
    resign(&mut changed);
    assert!(matches!(
        validate_manifest(&changed, SessionLimits::default()),
        Err(SessionError::Corrupt(_))
    ));
    for parent in [
        digest_bytes(b"missing"),
        original.branches[0].branch_id.clone(),
    ] {
        let mut changed = original.clone();
        changed.branches[0].parent = Some(parent);
        resign(&mut changed);
        assert!(matches!(
            validate_manifest(&changed, SessionLimits::default()),
            Err(SessionError::Corrupt(_))
        ));
    }
    let mut wrong = original.configuration.clone();
    wrong.quantum += 1;
    assert!(matches!(
        validate_configuration(&recording(), &wrong),
        Err(SessionError::Incompatible(_))
    ));
}
#[test]
fn explicit_limits_reject_memory_count_and_bytes_before_loading() {
    let mut manifest = fixture_manifest();
    manifest.configuration.vm_config.memory_size = usize::MAX;
    manifest.config_identity = digest_json(&manifest.configuration).unwrap();
    resign(&mut manifest);
    assert!(matches!(
        validate_manifest(&manifest, SessionLimits::default()),
        Err(SessionError::Bound(_))
    ));
    let limits = SessionLimits {
        max_checkpoints: 0,
        ..SessionLimits::default()
    };
    assert!(matches!(limits.validate(), Err(SessionError::Bound(_))));
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("large");
    std::fs::write(&input, b"12345").unwrap();
    assert!(matches!(
        read_bounded(File::open(input).unwrap(), 4, "large"),
        Err(SessionError::Bound(_))
    ));
}
#[test]
fn nofollow_rejects_leaf_directory_and_ancestor_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("input"), b"value").unwrap();
    std::os::unix::fs::symlink(&target, directory.path().join("link")).unwrap();
    assert!(open_directory(&directory.path().join("link")).is_err());
    assert!(open_file_path(&directory.path().join("link/input")).is_err());
    std::os::unix::fs::symlink(target.join("input"), target.join("leaf")).unwrap();
    assert!(open_file_path(&target.join("leaf")).is_err());
    let root = open_directory(&target).unwrap();
    for path in [
        "../input",
        "/input",
        "input/../input",
        "./input",
        "input//leaf",
    ] {
        assert!(open_relative(&root, path).is_err(), "{path}");
    }
}
#[test]
fn sealed_guest_inputs_cannot_drift_during_execution() {
    let file = sealed_artifact(b"exact bytes").unwrap();
    let mut writer = &file;
    assert!(writer.write_all(b"changed").is_err());
    let path = format!("/proc/self/fd/{}", file.as_raw_fd());
    assert_eq!(std::fs::read(path).unwrap(), b"exact bytes");
}
#[test]
fn complete_configuration_roundtrips_and_rejects_schedule_drift() {
    let config = configuration();
    let captured = SessionConfiguration::capture(&config).unwrap();
    let encoded = serde_json::to_vec(&captured).unwrap();
    let decoded: SessionConfiguration = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        digest_json(&captured).unwrap(),
        digest_json(&decoded).unwrap()
    );
    assert_eq!(
        decoded
            .simulation_config(FaultSchedule::new())
            .unwrap()
            .vm_config
            .cpu
            .hide_tsc,
        config.vm_config.cpu.hide_tsc
    );
    let mut forged = decoded;
    forged.schedule_identity = digest_bytes(b"wrong schedule");
    assert!(forged.simulation_config(FaultSchedule::new()).is_err());
}
#[test]
fn post_bootstrap_checkpoint_clock_is_bounded_and_not_catching_up() {
    let mut recorder = Recorder::new(recording().config, FaultSchedule::new(), FIXTURE_SEED);
    recorder.start_at_tick(1000).unwrap();
    recorder.on_tick(
        1001,
        || panic!("checkpoint before declared offset"),
        Vec::new(),
    );
    assert!(recorder.recording().checkpoints.is_empty());
    assert!(recorder.start_at_tick(u64::MAX).is_err());
}

#[test]
fn streamed_guest_verification_and_sealing_reject_identity_and_bound_drift() {
    let directory = tempfile::tempdir().unwrap();
    let root = open_directory(directory.path()).unwrap();
    let source = write_artifact(&root, "guest.bin", &[7; 128 * 1024]).unwrap();
    verify_streamed_artifact(&root, &source, source.bytes, &mut std::io::sink()).unwrap();
    let sealed = sealed_verified_artifact(&root, &source, source.bytes).unwrap();
    assert_eq!(sealed.metadata().unwrap().len(), source.bytes);
    let mut changed = source.clone();
    changed.digest = digest_bytes(b"different");
    assert!(matches!(
        sealed_verified_artifact(&root, &changed, source.bytes),
        Err(SessionError::Drift(_))
    ));
    assert!(matches!(
        sealed_verified_artifact(&root, &source, source.bytes - 1),
        Err(SessionError::Bound(_))
    ));
    let mut writer = &sealed;
    assert!(writer.write_all(b"mutate").is_err());
}

#[test]
fn shared_snapshot_codec_is_bounded_and_does_not_admit_wrong_vm_shape() {
    use chaoscontrol_vmm::controller::{NetworkFabric, SimulationSnapshot};
    let snapshot = SimulationSnapshot {
        tick: 10,
        vm_snapshots: Vec::new(),
        network_state: NetworkFabric::new(1, FIXTURE_SEED),
        fault_engine_snapshot: chaoscontrol_fault::engine::FaultEngine::new(Default::default())
            .snapshot(),
        vcpu_stall_until: Vec::new(),
        cpu_throttle: Vec::new(),
        clock_freeze: Vec::new(),
        clock_jitter_bound: Vec::new(),
        memory_pressure: Vec::new(),
        process_fault_attempt: Vec::new(),
        pending_process_observations: Default::default(),
        fault_operation_sequence: 0,
    };
    assert!(encode_snapshot_artifact(&snapshot, 0, 1).is_err());
    assert!(
        encode_snapshot_artifact_with_limits(&snapshot, 0, MAX_SESSION_SNAPSHOT_BYTES, 1).is_err()
    );
    let (reference, bytes) =
        encode_snapshot_artifact(&snapshot, 0, MAX_SESSION_SNAPSHOT_BYTES).unwrap();
    assert_eq!(
        decode_snapshot_artifact(&reference, &bytes).unwrap().tick,
        10
    );
    assert!(decode_snapshot_artifact_with_limit(&reference, &bytes, 1).is_err());
    let directory = tempfile::tempdir().unwrap();
    let root = open_directory(directory.path()).unwrap();
    let mut manifest = fixture_manifest();
    manifest.checkpoints[0].artifact = artifact(&reference.path, &bytes);
    manifest.checkpoints[0].snapshot_ref = reference;
    resign(&mut manifest);
    retained_fixture(&root, &manifest, false);
    write_artifact(&root, &manifest.checkpoints[0].artifact.path, &bytes).unwrap();
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::Corrupt(_))
    ));
}

#[test]
fn retained_graph_uses_real_capture_order_not_timestamp_inferred_forks() {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = fixture_manifest();
    let mut second = manifest.checkpoints[0].clone();
    second.id = 1;
    second.tick = 15;
    manifest.checkpoints.push(second);
    resign(&mut manifest);
    validate_manifest(&manifest, SessionLimits::default()).unwrap();
    // Exercise only the pure graph projection, never restored execution.
    let session = ExactSession {
        root: open_directory(directory.path()).unwrap(),
        manifest,
        recording: recording(),
        limits: SessionLimits::default(),
    };
    let facts = session.branch_graph_facts();
    assert_eq!(facts.len(), 3);
    assert_eq!(facts[0].kind, "branch");
    assert_eq!(facts[1].parent, Some(facts[0].id));
    assert_eq!(facts[2].parent, Some(facts[1].id));
    assert_eq!((facts[1].tick, facts[2].tick), (10, 15));
}
