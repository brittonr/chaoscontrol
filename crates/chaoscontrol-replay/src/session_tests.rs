use super::*;
use crate::snapshot_store::{decode_snapshot_artifact, encode_snapshot_artifact};
use chaoscontrol_fault::schedule::FaultSchedule;

const FIXTURE_MEMORY_BYTES: usize = 4096;
const FIXTURE_SEED: u64 = 42;
const FIXTURE_END_TICK: u64 = 20;
const FIXTURE_START_TICK: u64 = 10;

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
            smp_progress_mode: config.vm_config.smp_progress_mode,
            smp_progress_budget: config.vm_config.smp_progress_budget,
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
    recording.trace_start_tick = FIXTURE_START_TICK;
    recording.session_id = "fixture-session".into();
    recording.timestamp = 0;
    for tick in FIXTURE_START_TICK + 1..=FIXTURE_END_TICK {
        recording
            .fault_round_deltas
            .push(crate::recording::FaultRoundTraceDelta {
                tick,
                event_start: 0,
                event_end: 0,
            });
        recording
            .schedule_rounds
            .push(crate::recording::RecordedScheduleRound {
                injection_traces: Vec::new(),
                tick,
                traces: vec![],
            });
    }
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
    let config = SessionConfiguration::capture(
        &configuration(),
        fixture_execution_descriptors(&configuration()),
    )
    .unwrap();
    let snapshot_path = format!("snapshots/{}.snapshot.bin", "0".repeat(64));
    let mut manifest = SessionManifest {
        schema_version: SESSION_SCHEMA_VERSION,
        limits_profile: SESSION_LIMITS_PROFILE.into(),
        session_id: String::new(),
        config_identity: configuration_identity(&config).unwrap(),
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
                store: crate::snapshot_store::FILE_STORE_KIND.into(),
                digest: format!("sha256:{}", "0".repeat(64)),
                codec: crate::snapshot_store::SNAPSHOT_CODEC.into(),
                schema_version: crate::snapshot_store::SNAPSHOT_SCHEMA_VERSION,
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
        retained_start_tick: FIXTURE_START_TICK,
        retained_end_tick: FIXTURE_END_TICK,
        limits: SessionLimits::default(),
        finding: None,
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
    let config = SessionConfiguration::capture(
        &configuration(),
        fixture_execution_descriptors(&configuration()),
    )
    .unwrap();
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
    changed.limits_profile = "unknown-profile".into();
    resign(&mut changed);
    assert!(matches!(
        validate_manifest(&changed, SessionLimits::default()),
        Err(SessionError::Incompatible(_))
    ));
    let mut unsigned = original.clone();
    unsigned.limits_profile = "tampered-profile".into();
    assert_ne!(manifest_identity(&unsigned).unwrap(), original.session_id);
    let mut missing = serde_json::to_value(&original).unwrap();
    missing.as_object_mut().unwrap().remove("limits_profile");
    assert!(serde_json::from_value::<SessionManifest>(missing).is_err());
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
    manifest.config_identity = configuration_identity(&manifest.configuration).unwrap();
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
fn retained_manifest_cap_counts_raw_whitespace_and_equivalent_escapes() {
    const LOWER_MANIFEST_BYTES: u64 = 16 * 1024;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(MANIFEST_NAME);
    let mut manifest = fixture_manifest();
    manifest.limits.max_manifest_bytes = LOWER_MANIFEST_BYTES;
    resign(&mut manifest);
    validate_manifest(&manifest, SessionLimits::default()).unwrap();
    let compact = serde_json::to_vec(&manifest).unwrap();
    assert!(compact.len() as u64 <= LOWER_MANIFEST_BYTES);
    std::fs::write(&path, &compact).unwrap();
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::MissingArtifact(_))
    ));
    let escaped =
        String::from_utf8(compact)
            .unwrap()
            .replacen("schema_version", r"schema_\u0076ersion", 1);
    let mut padded = escaped.into_bytes();
    padded.resize(LOWER_MANIFEST_BYTES as usize + 1, b' ');
    std::fs::write(&path, padded).unwrap();
    assert!(matches!(
        load_session(directory.path(), SessionLimits::default()),
        Err(SessionError::Bound("manifest bytes"))
    ));
}

#[test]
fn retained_total_budget_charges_raw_manifest_bytes_before_artifact_io() {
    const LOWER_TOTAL_BYTES: u64 = 16 * 1024;
    for retained_limit in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut manifest = fixture_manifest();
        let mut limits = SessionLimits::default();
        if retained_limit {
            manifest.limits.max_total_bytes = LOWER_TOTAL_BYTES;
        } else {
            limits.max_total_bytes = LOWER_TOTAL_BYTES;
        }
        resign(&mut manifest);
        validate_manifest(&manifest, limits).unwrap();
        let inventory_bytes = manifest.recording.bytes
            + manifest.guest_artifacts[0].artifact.bytes
            + manifest.checkpoints[0].artifact.bytes;
        let mut raw = serde_json::to_vec(&manifest).unwrap();
        let raw_length = LOWER_TOTAL_BYTES - inventory_bytes + 1;
        assert!((raw.len() as u64) < raw_length);
        raw.resize(raw_length as usize, b' ');
        std::fs::write(directory.path().join(MANIFEST_NAME), &raw).unwrap();
        assert!(matches!(
            load_session(directory.path(), limits),
            Err(SessionError::Bound("total bytes"))
        ));
    }
}

#[test]
fn artifact_declared_lengths_are_checked_before_reading_or_copying() {
    const SOURCE_BYTES: usize = 128 * 1024;
    let directory = tempfile::tempdir().unwrap();
    let root = open_directory(directory.path()).unwrap();
    let bytes = vec![7; SOURCE_BYTES];
    let source = write_artifact(&root, "bounded.bin", &bytes).unwrap();
    assert_eq!(read_verified(&root, &source, source.bytes).unwrap(), bytes);
    for declared in [1, source.bytes - 1, source.bytes + 1] {
        let mut mismatched = source.clone();
        mismatched.bytes = declared;
        assert!(matches!(
            read_verified(&root, &mismatched, declared),
            Err(SessionError::Drift(_))
        ));
        let mut copied = Vec::new();
        assert!(matches!(
            verify_streamed_artifact(&root, &mismatched, declared, &mut copied),
            Err(SessionError::Drift(_))
        ));
        assert!(copied.is_empty());
        assert!(matches!(
            sealed_verified_artifact(&root, &mismatched, declared),
            Err(SessionError::Drift(_))
        ));
    }
}

#[test]
fn streamed_artifact_growth_and_shrinkage_never_copy_beyond_declared_bytes() {
    const SOURCE_BYTES: usize = 128 * 1024;
    struct ResizeOnWrite {
        file: File,
        length: u64,
        copied: u64,
    }
    impl Write for ResizeOnWrite {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.copied == 0 {
                self.file.set_len(self.length)?;
            }
            self.copied += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let root = open_directory(directory.path()).unwrap();
    let path = directory.path().join("changing.bin");
    let bytes = vec![7; SOURCE_BYTES];
    let source = artifact("changing.bin", &bytes);
    for length in [1, source.bytes + 1] {
        std::fs::write(&path, &bytes).unwrap();
        let mut sink = ResizeOnWrite {
            file: File::options().write(true).open(&path).unwrap(),
            length,
            copied: 0,
        };
        assert!(matches!(
            verify_streamed_artifact(&root, &source, source.bytes, &mut sink),
            Err(SessionError::Drift(_))
        ));
        assert!(sink.copied <= source.bytes);
        if length > source.bytes {
            assert_eq!(sink.copied, source.bytes);
        }
    }
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
    let captured =
        SessionConfiguration::capture(&config, fixture_execution_descriptors(&config)).unwrap();
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
fn exact_smp_sessions_bind_effective_clock_policy_and_reject_previous_schema() {
    let (config, recording) = exact_smp_fixture();
    assert!(config.vm_config.cpu.hide_tsc);
    validate_configuration(&recording, &config).unwrap();
    let mut incompatible = config;
    incompatible.vm_config.cpu.hide_tsc = false;
    assert!(matches!(
        validate_configuration(&recording, &incompatible),
        Err(SessionError::Incompatible(_))
    ));

    let mut previous = fixture_manifest();
    previous.schema_version = 1;
    resign(&mut previous);
    assert!(matches!(
        validate_manifest(&previous, SessionLimits::default()),
        Err(SessionError::Incompatible(_))
    ));
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
        execution_selection: None,
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

#[test]
fn reviewed_limits_projection_rejects_zero_and_raised_fields() {
    let limits = SessionLimits::default();
    limits.validate().unwrap();
    assert_eq!(limits.max_snapshot_bytes, MAX_SESSION_SNAPSHOT_BYTES);
    assert_eq!(
        limits.max_decoded_snapshot_bytes,
        crate::snapshot_store::MAX_DECOMPRESSED_SNAPSHOT_BYTES
    );
    let projection: serde_json::Value = serde_json::from_str(REVIEWED_LIMITS_JSON).unwrap();
    assert_eq!(serde_json::to_value(limits).unwrap(), projection);
    for (field, value) in projection.as_object().unwrap() {
        for replacement in [0, value.as_u64().unwrap() + 1] {
            let mut changed = projection.clone();
            changed[field] = replacement.into();
            let limits: SessionLimits = serde_json::from_value(changed).unwrap();
            assert!(
                matches!(limits.validate(), Err(SessionError::Bound(_))),
                "{field}"
            );
        }
    }
    let mut missing = projection.clone();
    missing.as_object_mut().unwrap().remove("host_deadline_ms");
    assert!(serde_json::from_value::<SessionLimits>(missing).is_err());
    let mut unknown = projection;
    unknown["infer_vm_defaults"] = true.into();
    assert!(serde_json::from_value::<SessionLimits>(unknown).is_err());
}

#[test]
fn explicit_empty_rounds_are_distinct_from_missing_or_reordered_evidence() {
    let complete = recording();
    validate_session_recording(&complete, FIXTURE_START_TICK, SessionLimits::default()).unwrap();
    let mut missing_fault = complete.clone();
    missing_fault.fault_round_deltas.pop();
    assert!(matches!(
        validate_session_recording(&missing_fault, FIXTURE_START_TICK, SessionLimits::default()),
        Err(SessionError::Corrupt(_))
    ));
    let mut missing_schedule = complete.clone();
    missing_schedule.schedule_rounds.pop();
    assert!(matches!(
        validate_session_recording(
            &missing_schedule,
            FIXTURE_START_TICK,
            SessionLimits::default()
        ),
        Err(SessionError::Corrupt(_))
    ));
    let mut reordered = complete;
    reordered.schedule_rounds.swap(0, 1);
    assert!(matches!(
        validate_session_recording(&reordered, FIXTURE_START_TICK, SessionLimits::default()),
        Err(SessionError::Corrupt(_))
    ));
}

#[test]
fn retained_output_ticks_and_artifact_count_obey_lowered_profile() {
    let mut recording = recording();
    recording
        .events
        .push(crate::recording::RecordedEvent::SerialOutput {
            tick: FIXTURE_START_TICK + 1,
            vm_index: 0,
            data: "12345".into(),
        });
    let limits = SessionLimits {
        max_output_bytes: 4,
        ..SessionLimits::default()
    };
    assert!(matches!(
        validate_session_recording(&recording, FIXTURE_START_TICK, limits),
        Err(SessionError::Bound("retained output bytes"))
    ));
    let limits = SessionLimits {
        max_replay_ticks: 1,
        ..SessionLimits::default()
    };
    assert!(matches!(
        validate_session_recording(&recording, FIXTURE_START_TICK, limits),
        Err(SessionError::Bound("replay ticks"))
    ));
    let manifest = fixture_manifest();
    let limits = SessionLimits {
        max_artifacts: 1,
        ..SessionLimits::default()
    };
    assert!(matches!(
        validate_manifest(&manifest, limits),
        Err(SessionError::Bound("artifact count"))
    ));
    let mut manifest = manifest;
    manifest.retained_end_tick = u64::MAX;
    resign(&mut manifest);
    assert!(matches!(
        validate_manifest(&manifest, SessionLimits::default()),
        Err(SessionError::Bound("replay ticks"))
    ));
}

#[test]
fn effective_profile_never_raises_a_retained_runtime_limit() {
    let caller = SessionLimits::default();
    let retained = SessionLimits {
        max_output_bytes: 1,
        host_deadline_ms: 1,
        ..caller
    };
    let effective = caller.intersect(retained);
    effective.validate().unwrap();
    assert_eq!(effective.max_output_bytes, 1);
    assert_eq!(effective.host_deadline_ms, 1);
}

fn finding_fixture() -> (SessionFinding, Vec<u8>) {
    use chaoscontrol_protocol::identity::{
        AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
    };
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.onixresearch.chaoscontrol.persisted-debugger-fixture".into(),
        logical_key: AssertionLogicalKey::Stable {
            key: "retained-finding".into(),
        },
        compatibility_id: Some(7),
        kind: AssertionKind::Always,
        message: "retained finding identity".into(),
        source_file: "fixture.rs".into(),
        source_line: 1,
        source_column: 1,
        guest: "fixture".into(),
        category: "invariant".into(),
    };
    let identity = chaoscontrol_protocol::admission::AssertionEvidenceIdentity {
        fingerprint: descriptor.fingerprint().unwrap(),
        canonical_descriptor: descriptor.canonical_bytes().unwrap(),
        catalog_token: chaoscontrol_protocol::admission::token_for_descriptors(
            std::slice::from_ref(&descriptor),
        )
        .unwrap(),
        descriptor,
    };
    let parent = fixture_manifest().checkpoints[0].snapshot_ref.clone();
    let carrier = serde_json::to_vec(&serde_json::json!({
        "bug_id": 4, "assertion_id": 7, "assertion_identity": identity,
        "replay_parent_snapshot_ref": parent, "replay_parent_depth": 0,
    }))
    .unwrap();
    (
        SessionFinding {
            bug_artifact: artifact("finding.json", &carrier),
            bug_id: 4,
            assertion_id: 7,
            assertion_identity: identity,
            original_parent: parent,
            original_parent_depth: 0,
            original_parent_tick: FIXTURE_START_TICK,
            command: "explore reproduce --debug-recording-output session".into(),
            requested_ticks: FIXTURE_END_TICK - FIXTURE_START_TICK,
            target_failed: true,
            reproduction_exit_status: 0,
        },
        carrier,
    )
}

// r[verify chaoscontrol.replay_debugger.producer]
// r[verify chaoscontrol.replay_debugger.claim_boundary]
#[test]
fn finding_outcome_and_original_parent_are_bound_without_promoting_reproduction() {
    let (finding, _) = finding_fixture();
    let mut manifest = fixture_manifest();
    manifest.finding = Some(finding.clone());
    resign(&mut manifest);
    validate_manifest(&manifest, SessionLimits::default()).unwrap();
    let mut not_reproduced = finding.clone();
    not_reproduced.target_failed = false;
    assert!(validate_finding(&not_reproduced, FIXTURE_START_TICK, FIXTURE_END_TICK).is_err());
    not_reproduced.reproduction_exit_status = 1;
    validate_finding(&not_reproduced, FIXTURE_START_TICK, FIXTURE_END_TICK).unwrap();
    for change in 0..5 {
        let mut invalid = finding.clone();
        match change {
            0 => invalid.original_parent_tick -= 1,
            1 => invalid.requested_ticks -= 1,
            2 => invalid.assertion_id += 1,
            3 => invalid.command.clear(),
            _ => invalid.bug_artifact.path = "other-finding.json".into(),
        }
        assert!(validate_finding(&invalid, FIXTURE_START_TICK, FIXTURE_END_TICK).is_err());
    }
    assert!(validate_finding(&finding, FIXTURE_END_TICK, FIXTURE_START_TICK).is_err());
}

// r[verify chaoscontrol.replay_debugger.producer]
// r[verify chaoscontrol.replay_debugger.validation]
#[test]
fn finding_carrier_cannot_substitute_another_bug_assertion_catalog_or_parent() {
    let (finding, carrier) = finding_fixture();
    validate_finding_carrier(&finding, &carrier).unwrap();
    for field in [
        "bug_id",
        "assertion_id",
        "assertion_identity",
        "catalog_token",
        "replay_parent_snapshot_ref",
        "replay_parent_depth",
    ] {
        let mut forged: serde_json::Value = serde_json::from_slice(&carrier).unwrap();
        match field {
            "bug_id" => forged[field] = 5.into(),
            "assertion_id" => forged[field] = 8.into(),
            "assertion_identity" => {
                forged[field]["descriptor"]["message"] = "another assertion".into()
            }
            "catalog_token" => {
                let token = forged["assertion_identity"]["catalog_token"]
                    .as_str()
                    .unwrap();
                let replacement = format!(
                    "{}{}",
                    if token.starts_with('0') { "1" } else { "0" },
                    &token[1..]
                );
                forged["assertion_identity"]["catalog_token"] = replacement.into();
            }
            "replay_parent_snapshot_ref" => {
                forged[field]["digest"] = format!("sha256:{}", "1".repeat(64)).into()
            }
            _ => forged[field] = 1.into(),
        }
        assert!(validate_finding_carrier(&finding, &serde_json::to_vec(&forged).unwrap()).is_err());
    }
    let root = tempfile::tempdir().unwrap();
    let directory = open_directory(root.path()).unwrap();
    write_artifact(&directory, MANIFEST_NAME, b"previous complete session").unwrap();
    assert!(write_artifact(&directory, MANIFEST_NAME, b"replacement").is_err());
    assert_eq!(
        std::fs::read(root.path().join(MANIFEST_NAME)).unwrap(),
        b"previous complete session"
    );
}

fn exact_snapshot_fixture(
    tick: u64,
    scheduler: chaoscontrol_vmm::scheduler::SchedulerSnapshot,
    recording: &Recording,
) -> chaoscontrol_vmm::controller::SimulationSnapshot {
    use chaoscontrol_vmm::devices::{entropy::DeterministicEntropy, pit::DeterministicPit};
    use chaoscontrol_vmm::snapshot::{
        build_snapshot_inventory, MsrSnapshot, SnapshotMemory, SnapshotMetadata, SnapshotTopology,
        VcpuSnapshot, VmSnapshot, SNAPSHOT_PROFILE_EXACT_X86_KVM_V2, SNAPSHOT_STATE_SCHEMA_VERSION,
    };
    // Standard KVM_GET_XSAVE ABI image, including its zero reserved header.
    const KVM_STANDARD_XSAVE_BYTES: usize = 4096;
    const PIT_HPET_LEGACY_INHIBIT: u32 = 1;
    const TSC_MSR_INDEX: u32 = 0x10;
    let num_vcpus = scheduler.state.num_vcpus;
    let mut fixture_config = configuration();
    fixture_config.vm_config.num_vcpus = num_vcpus;
    fixture_config.vm_config.smp_progress_mode = recording.config.smp_progress_mode;
    fixture_config.vm_config.smp_progress_budget = recording.config.smp_progress_budget;
    let mut engine =
        chaoscontrol_fault::engine::FaultEngine::new(chaoscontrol_fault::engine::EngineConfig {
            seed: recording.seed,
            num_vms: recording.config.num_vms,
            schedule: Some(recording.schedule.clone()),
            random_faults: recording.config.random_faults,
            ..Default::default()
        });
    engine.start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
    let controller_engine_snapshot = engine.snapshot();
    // Controller orchestration keeps its pending oracle; a VM's active run
    // instead requires a genuinely admitted strict catalog, even before hits.
    let descriptor = finding_fixture().0.assertion_identity.descriptor;
    let token =
        chaoscontrol_protocol::admission::token_for_descriptors(std::slice::from_ref(&descriptor))
            .unwrap();
    let mut catalog = chaoscontrol_protocol::admission::CatalogBuilder::begin(1).unwrap();
    catalog.insert(descriptor).unwrap();
    engine
        .oracle_mut()
        .activate_catalog(catalog.complete(token).unwrap())
        .unwrap();
    let topology = SnapshotTopology {
        vcpu_count: num_vcpus as u32,
        msr_indices: vec![TSC_MSR_INDEX],
        virtio_devices: Vec::new(),
    };
    let mut vcpu = VcpuSnapshot {
        regs: Default::default(),
        sregs: Default::default(),
        fpu: Default::default(),
        debug_regs: Default::default(),
        lapic: Default::default(),
        xcrs: Default::default(),
        xsave: vec![0; KVM_STANDARD_XSAVE_BYTES],
        events: Default::default(),
        msrs: vec![MsrSnapshot {
            index: TSC_MSR_INDEX,
            data: 0,
        }],
        cpuid: vec![chaoscontrol_vmm::snapshot::SnapshotCpuidEntry {
            function: 0,
            index: 0,
            flags: 0,
            eax: 0,
            ebx: 0,
            ecx: 0,
            edx: 0,
        }],
        mp_state: Default::default(),
    };
    vcpu.regs.rflags = 2;
    let mut vm = VmSnapshot {
        metadata: Some(SnapshotMetadata {
            state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
            completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V2.into(),
            inventory: build_snapshot_inventory(&topology),
            topology,
            execution_descriptor: fixture_execution_descriptors(&fixture_config).remove(0),
        }),
        boundary_state: chaoscontrol_vmm::snapshot::SnapshotBoundaryState {
            pending_io: chaoscontrol_vmm::snapshot::SnapshotPendingIoState::Settled,
            injection_ledger: chaoscontrol_vmm::snapshot::InjectionLedger::new(),
            delivery_state:
                chaoscontrol_vmm::snapshot::SnapshotDeliveryState::CapturedKernelAndUserspace,
        },
        vcpu_snapshots: vec![vcpu; num_vcpus],
        pic_master: Default::default(),
        pic_slave: Default::default(),
        ioapic: Default::default(),
        pit: if recording.config.smp_progress_mode
            == chaoscontrol_vmm::scheduler::core::ProgressMode::KvmDebugBoundary
        {
            chaoscontrol_vmm::snapshot::SnapshotKernelPitState::Absent
        } else {
            chaoscontrol_vmm::snapshot::SnapshotKernelPitState::Present(Default::default())
        },
        clock: Default::default(),
        memory: SnapshotMemory::Full(vec![0; FIXTURE_MEMORY_BYTES]),
        serial_state: Default::default(),
        entropy: DeterministicEntropy::new(FIXTURE_SEED).snapshot(),
        virtual_tsc: 0,
        exit_count: 0,
        io_exit_count: 0,
        exits_since_last_sdk: 0,
        panic_detected: false,
        panic_match_state: 0,
        pit_snapshot: DeterministicPit::new(configuration().vm_config.cpu.tsc_khz).snapshot(),
        last_kvm_pit_mode: 0,
        fault_engine_snapshot: engine.snapshot(),
        virtio_snapshots: Vec::new(),
        coverage_active: false,
        scheduler_snapshot: scheduler,
        hlt_latched_vcpus: vec![false; num_vcpus],
    };
    if let chaoscontrol_vmm::snapshot::SnapshotKernelPitState::Present(pit) = &mut vm.pit {
        pit.flags = PIT_HPET_LEGACY_INHIBIT;
    }
    // Stable KVM_IRQCHIP identities: master=0, slave=1, IOAPIC=2.
    vm.pic_slave.chip_id = 1;
    vm.ioapic.chip_id = 2;
    chaoscontrol_vmm::controller::SimulationSnapshot {
        tick,
        vm_snapshots: vec![(vm, chaoscontrol_vmm::controller::VmStatus::Running)],
        network_state: chaoscontrol_vmm::controller::NetworkFabric::new(1, FIXTURE_SEED),
        fault_engine_snapshot: controller_engine_snapshot,
        vcpu_stall_until: vec![BTreeMap::new()],
        cpu_throttle: vec![None],
        clock_freeze: vec![None],
        clock_jitter_bound: vec![0],
        memory_pressure: vec![None],
        process_fault_attempt: vec![None],
        pending_process_observations: Default::default(),
        fault_operation_sequence: 0,
    }
}

fn exact_smp_fixture() -> (SessionConfiguration, Recording) {
    smp_fixture(chaoscontrol_vmm::scheduler::core::ProgressMode::ExactSingleStep)
}

pub(crate) fn coarse_smp_fixture() -> (SessionConfiguration, Recording) {
    smp_fixture(chaoscontrol_vmm::scheduler::core::ProgressMode::KvmDebugBoundary)
}

fn smp_fixture(
    mode: chaoscontrol_vmm::scheduler::core::ProgressMode,
) -> (SessionConfiguration, Recording) {
    use chaoscontrol_vmm::scheduler::core::{
        plan_execution_observation, ExecutionProgressObservation, ProgressMode, ScheduleTrace,
    };
    use chaoscontrol_vmm::scheduler::{
        SchedulerConfig, SchedulerSnapshot, VcpuScheduler, SCHEDULER_SNAPSHOT_SCHEMA_VERSION,
    };
    const SMP_VCPUS: usize = 2;
    const SMP_QUANTUM: u64 = 32;
    let mut config = configuration();
    config.vm_config.num_vcpus = SMP_VCPUS;
    let mut recording = recording();
    let budget = match mode {
        ProgressMode::KvmDebugBoundary => {
            chaoscontrol_vmm::scheduler::core::ProgressBudget::KvmBoundaries {
                profile_id:
                    chaoscontrol_vmm::scheduler::core::BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: SMP_QUANTUM,
            }
        }
        _ => chaoscontrol_vmm::scheduler::core::ProgressBudget::Instructions(SMP_QUANTUM),
    };
    config.vm_config.smp_progress_mode = mode;
    config.vm_config.smp_progress_budget = budget;
    recording.config.smp_progress_mode = mode;
    recording.config.smp_progress_budget = config.vm_config.smp_progress_budget;
    let mut state = VcpuScheduler::try_new(
        &SchedulerConfig {
            num_vcpus: SMP_VCPUS,
            progress_budget: budget,
            seed: FIXTURE_SEED,
            ..Default::default()
        },
        mode,
        vec![true; SMP_VCPUS],
    )
    .unwrap()
    .snapshot()
    .state;
    let mut checkpoints = CheckpointStore::new();
    for tick in FIXTURE_START_TICK..=FIXTURE_END_TICK {
        if tick > FIXTURE_START_TICK {
            let planned = plan_execution_observation(
                &state,
                match mode {
                    ProgressMode::KvmDebugBoundary => ExecutionProgressObservation::KvmBoundary {
                        vcpu: 0,
                        profile_id: chaoscontrol_vmm::scheduler::core::BoundaryProfileId::KvmDebugExitBoundaryV1,
                        boundary: chaoscontrol_vmm::scheduler::core::KvmBoundaryKind::DebugSingleStep,
                    },
                    _ => ExecutionProgressObservation::ExactInstruction { vcpu: 0 },
                },
            )
            .unwrap()
            .unwrap();
            recording.schedule_rounds[(tick - FIXTURE_START_TICK - 1) as usize].traces =
                vec![chaoscontrol_vmm::controller::VmScheduleTrace {
                    vm_index: 0,
                    trace: ScheduleTrace {
                        schema_version:
                            chaoscontrol_vmm::scheduler::core::SCHEDULE_TRACE_SCHEMA_VERSION,
                        initial_state_id: state.identity(),
                        initial_state: state,
                        records: vec![planned.record],
                    },
                }];
            if mode == ProgressMode::KvmDebugBoundary {
                recording.schedule_rounds[(tick - FIXTURE_START_TICK - 1) as usize]
                    .injection_traces = vec![chaoscontrol_vmm::controller::VmInjectionTrace {
                    vm_id: 0,
                    records: Vec::new(),
                }];
            }
            state = planned.next_state;
        }
        if tick == FIXTURE_START_TICK || tick == FIXTURE_START_TICK + 5 || tick == FIXTURE_END_TICK
        {
            checkpoints.push(Checkpoint {
                id: checkpoints.len() as u64,
                tick,
                snapshot: Some(exact_snapshot_fixture(
                    tick,
                    SchedulerSnapshot {
                        schema_version: SCHEDULER_SNAPSHOT_SCHEMA_VERSION,
                        state_id: state.identity(),
                        state: state.clone(),
                    },
                    &recording,
                )),
                serial_output: Vec::new(),
                events_since_last: Vec::new(),
            });
        }
    }
    recording.checkpoints = checkpoints;
    (
        SessionConfiguration::capture(&config, fixture_execution_descriptors(&config)).unwrap(),
        recording,
    )
}

fn retain_exact_fixture(
    directory: &Path,
    configuration: &SessionConfiguration,
    recording: &Recording,
    disk: Option<&[u8]>,
) {
    let root = open_directory(directory).unwrap();
    mkdir_child(&root, "artifacts").unwrap();
    mkdir_child(&root, "snapshots").unwrap();
    let mut manifest = fixture_manifest();
    manifest.configuration = configuration.clone();
    manifest.config_identity = configuration_identity(configuration).unwrap();
    manifest.recording = write_artifact(
        &root,
        "recording.json",
        &serde_json::to_vec(recording).unwrap(),
    )
    .unwrap();
    manifest.guest_artifacts[0].artifact =
        write_artifact(&root, "artifacts/kernel.bin", b"kernel").unwrap();
    if let Some(disk) = disk {
        manifest.guest_artifacts.push(GuestArtifact {
            role: GuestArtifactRole::Disk,
            artifact: write_artifact(&root, "artifacts/disk.bin", disk).unwrap(),
        });
    }
    manifest.checkpoints.clear();
    for checkpoint in recording.checkpoints.all() {
        let (reference, bytes) = encode_snapshot_artifact(
            checkpoint.snapshot.as_ref().unwrap(),
            0,
            MAX_SESSION_SNAPSHOT_BYTES,
        )
        .unwrap();
        manifest.checkpoints.push(SessionCheckpoint {
            id: checkpoint.id,
            tick: checkpoint.tick,
            artifact: write_artifact(&root, &reference.path, &bytes).unwrap(),
            snapshot_ref: reference,
            serial_output: checkpoint.serial_output.clone(),
            events_since_last: checkpoint.events_since_last.clone(),
        });
    }
    resign(&mut manifest);
    write_artifact(
        &root,
        MANIFEST_NAME,
        &serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn checkpoint_scheduler_prefix_and_suffix_bind_save_and_load_admission() {
    let (config, recording) = exact_smp_fixture();
    validate_recording(&recording).unwrap();
    for checkpoint in recording.checkpoints.all() {
        validate_checkpoint(&recording, checkpoint, &config).unwrap();
    }
    let valid = tempfile::tempdir().unwrap();
    retain_exact_fixture(valid.path(), &config, &recording, None);
    assert!(load_session(valid.path(), SessionLimits::default()).is_ok());

    // Initial checkpoint binds the suffix start; interior and final checkpoints
    // bind the completed prefix even when no continuation is requested.
    for corrupt_id in 0..recording.checkpoints.len() {
        let mut changed = recording.clone();
        let mut store = CheckpointStore::new();
        for checkpoint in recording.checkpoints.all() {
            let mut checkpoint = checkpoint.clone();
            if checkpoint.id == corrupt_id as u64 {
                let scheduler = &mut checkpoint.snapshot.as_mut().unwrap().vm_snapshots[0]
                    .0
                    .scheduler_snapshot;
                scheduler.state.rng_seed[0] ^= 1;
                scheduler.state_id = scheduler.state.identity();
                scheduler.validate().unwrap(); // The foreign state is internally valid.
                assert!(matches!(
                    validate_checkpoint(&recording, &checkpoint, &config),
                    Err(SessionError::Corrupt(_))
                ));
            }
            store.push(checkpoint);
        }
        changed.checkpoints = store;
        let invalid = tempfile::tempdir().unwrap();
        retain_exact_fixture(invalid.path(), &config, &changed, None);
        assert!(
            matches!(load_session(invalid.path(), SessionLimits::default()), Err(SessionError::Corrupt(message))
            if message.contains("checkpoint scheduler differs"))
        );
    }
}

#[test]
fn retained_disk_bytes_bind_every_decoded_checkpoint_base() {
    use chaoscontrol_vmm::devices::{
        block::DeterministicBlock, virtio_block::VirtioBlock, virtio_mmio::VirtioMmioDevice,
    };
    use chaoscontrol_vmm::snapshot::{
        build_snapshot_inventory, VirtioBackendSnapshot, VirtioDeviceSnapshot,
    };
    let (mut config, mut recording) = exact_smp_fixture();
    config.disk_image_path = Some("disk".into());
    recording.config.disk_image_path = config.disk_image_path.clone();
    let base = vec![0; FIXTURE_MEMORY_BYTES];
    let mut checkpoints = CheckpointStore::new();
    for checkpoint in recording.checkpoints.all() {
        let mut checkpoint = checkpoint.clone();
        let vm = &mut checkpoint.snapshot.as_mut().unwrap().vm_snapshots[0].0;
        let block = DeterministicBlock::from_image(base.clone());
        let backend = block.snapshot();
        let transport = VirtioMmioDevice::new(0x1000, 5, Box::new(VirtioBlock::new(block)))
            .snapshot()
            .unwrap();
        let device = VirtioDeviceSnapshot {
            transport,
            backend: VirtioBackendSnapshot::Block(Box::new(backend)),
        };
        let metadata = vm.metadata.as_mut().unwrap();
        metadata.topology.virtio_devices =
            vec![(device.identity(), device.transport.queues.len() as u32)];
        metadata.inventory = build_snapshot_inventory(&metadata.topology);
        vm.virtio_snapshots.push(device);
        checkpoints.push(checkpoint);
    }
    recording.checkpoints = checkpoints;
    let exact = sealed_artifact(&base).unwrap();
    validate_disk_base(&recording, &exact).unwrap();
    let valid = tempfile::tempdir().unwrap();
    retain_exact_fixture(valid.path(), &config, &recording, Some(&base));
    assert!(load_session(valid.path(), SessionLimits::default()).is_ok());

    let different = vec![1; base.len()];
    assert!(matches!(
        validate_disk_base(&recording, &sealed_artifact(&different).unwrap()),
        Err(SessionError::Drift(_))
    ));
    let invalid = tempfile::tempdir().unwrap();
    // All artifact and manifest digests are recomputed: only the cross-binding
    // can reject this different, same-sized retained image.
    retain_exact_fixture(invalid.path(), &config, &recording, Some(&different));
    assert!(
        matches!(load_session(invalid.path(), SessionLimits::default()), Err(SessionError::Drift(message))
        if message == "disk snapshot base")
    );
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
// r[verify chaoscontrol.kvm_debug_boundary.admission]
#[test]
fn coarse_static_admission_is_not_execution_authorization() {
    use chaoscontrol_vmm::scheduler::core::ProgressMode;
    let (config, recording) = coarse_smp_fixture();
    validate_recording(&recording).unwrap();
    for checkpoint in recording.checkpoints.all() {
        validate_checkpoint(&recording, checkpoint, &config).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    retain_exact_fixture(directory.path(), &config, &recording, None);
    let mut session = load_session(directory.path(), SessionLimits::default()).unwrap();
    assert!(session.require_execution_authorization().is_err());
    assert!(session.restore_controller(0).is_err());
    assert!(session
        .authorize_execution(ExecutionSelection {
            progress_mode: ProgressMode::ExactSingleStep,
            progress_budget: config.vm_config.smp_progress_budget,
        })
        .is_err());
    assert!(session
        .authorize_execution(ExecutionSelection {
            progress_mode: ProgressMode::KvmDebugBoundary,
            progress_budget: config.vm_config.smp_progress_budget.with_count(33),
        })
        .is_err());
    session
        .authorize_execution(ExecutionSelection {
            progress_mode: ProgressMode::KvmDebugBoundary,
            progress_budget: config.vm_config.smp_progress_budget,
        })
        .unwrap();
    session.require_execution_authorization().unwrap();
    // Mechanism fixtures are explicitly non-measured, never executable host proof.
    assert!(session.restore_controller(0).is_err());
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn coarse_schema_units_profiles_and_missing_fields_never_default() {
    let (config, recording) = coarse_smp_fixture();
    let mut old = recording.clone();
    old.schema_version = crate::recording::RECORDING_SCHEMA_VERSION - 1;
    assert_eq!(
        validate_recording(&old),
        Err(crate::recording::RecordingValidationError::UnsupportedSchema)
    );
    let value = serde_json::to_value(&recording).unwrap();
    for field in ["schema_version", "schedule_rounds", "fault_outcome_ledger"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Recording>(missing).is_err());
    }
    for field in ["smp_progress_mode", "smp_progress_budget"] {
        let mut missing = value.clone();
        missing["config"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Recording>(missing).is_err());
    }
    let mut unknown = value;
    unknown["config"]["smp_progress_budget"]["KvmBoundaries"]["profile_id"] =
        serde_json::json!("unknown-boundary-profile");
    assert!(serde_json::from_value::<Recording>(unknown).is_err());
    let mut wrong = recording.clone();
    wrong.config.smp_progress_budget =
        chaoscontrol_vmm::scheduler::core::ProgressBudget::Instructions(32);
    assert!(validate_recording(&wrong).is_err());
    assert!(validate_configuration(&wrong, &config).is_err());
    let mut wrong_config = config.clone();
    wrong_config.execution_descriptors[0]
        .boundary_profile
        .as_mut()
        .unwrap()
        .table_identity[0] ^= 1;
    assert!(validate_execution_descriptors(
        &wrong_config.execution_descriptors,
        1,
        &wrong_config.vm_config
    )
    .is_err());
    let mut missing = serde_json::to_value(&config).unwrap();
    missing
        .as_object_mut()
        .unwrap()
        .remove("execution_descriptors");
    assert!(serde_json::from_value::<SessionConfiguration>(missing).is_err());
}

fn coarse_injection_fixture() -> (SessionConfiguration, Recording) {
    use chaoscontrol_vmm::snapshot::{
        InjectionKind, InjectionLedger, InjectionRecord, InjectionSource,
    };
    let (config, mut recording) = coarse_smp_fixture();
    let round = &mut recording.schedule_rounds[0];
    let requested = InjectionRecord {
        target_vcpu: None,
        kind: InjectionKind::Irq {
            irq: 1,
            level: true,
        },
        source: InjectionSource::Shell,
        requested_state_id: round.traces[0].trace.initial_state_id.0,
        accepted: false,
    };
    let mut accepted = requested.clone();
    accepted.accepted = true;
    round.injection_traces[0].records = vec![requested, accepted];
    let mut ledger = InjectionLedger::new();
    for fact in &round.injection_traces[0].records {
        ledger.record(fact).unwrap();
    }
    let mut checkpoints = CheckpointStore::new();
    for checkpoint in recording.checkpoints.all() {
        let mut checkpoint = checkpoint.clone();
        if checkpoint.tick > FIXTURE_START_TICK {
            checkpoint.snapshot.as_mut().unwrap().vm_snapshots[0]
                .0
                .boundary_state
                .injection_ledger = ledger.clone();
        }
        checkpoints.push(checkpoint);
    }
    recording.checkpoints = checkpoints;
    (config, recording)
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn coarse_injection_prefix_rejects_dropped_reordered_and_forged_facts() {
    let (config, recording) = coarse_injection_fixture();
    validate_recording(&recording).unwrap();
    for checkpoint in recording.checkpoints.all() {
        validate_checkpoint(&recording, checkpoint, &config).unwrap();
    }
    let valid = tempfile::tempdir().unwrap();
    retain_exact_fixture(valid.path(), &config, &recording, None);
    assert!(load_session(valid.path(), SessionLimits::default()).is_ok());
    for mutation in 0..4 {
        let mut changed = recording.clone();
        match mutation {
            0 => {
                changed.schedule_rounds[0].injection_traces[0]
                    .records
                    .remove(0);
            }
            1 => changed.schedule_rounds[0].injection_traces[0]
                .records
                .reverse(),
            2 => {
                changed.schedule_rounds[0].injection_traces[0].records[0].requested_state_id =
                    [7; 32]
            }
            _ => changed.schedule_rounds[0].injection_traces.clear(),
        }
        let invalid = tempfile::tempdir().unwrap();
        retain_exact_fixture(invalid.path(), &config, &changed, None);
        assert!(load_session(invalid.path(), SessionLimits::default()).is_err());
    }
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn actual_coarse_continuation_compares_memory_registers_pending_state_and_ledger() {
    let (_, recording) = coarse_injection_fixture();
    let expected = recording
        .checkpoints
        .all()
        .last()
        .unwrap()
        .snapshot
        .as_ref()
        .unwrap();
    crate::replay::validate_actual_snapshot_continuation(&recording, expected, false).unwrap();
    for mutation in 0..5 {
        let mut actual = expected.clone();
        let vm = &mut actual.vm_snapshots[0].0;
        match mutation {
            0 => vm.vcpu_snapshots[0].regs.rip ^= 1,
            1 => {
                if let chaoscontrol_vmm::snapshot::SnapshotMemory::Full(bytes) = &mut vm.memory {
                    bytes[0] ^= 1;
                } else {
                    unreachable!();
                }
            }
            2 => vm.boundary_state.injection_ledger.prefix_digest[0] ^= 1,
            3 => vm.vcpu_snapshots[0].events.interrupt.injected ^= 1,
            _ => vm.virtual_tsc += 1,
        }
        assert!(
            crate::replay::validate_actual_snapshot_continuation(&recording, &actual, false)
                .is_err()
        );
    }
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn coarse_unsettled_device_delivery_and_descriptor_mismatch_reject_before_restore() {
    use chaoscontrol_vmm::snapshot::{SnapshotDeliveryState, SnapshotPendingIoState};
    let (config, recording) = coarse_smp_fixture();
    for mutation in 0..4 {
        let mut checkpoint = recording.checkpoints.all()[0].clone();
        let vm = &mut checkpoint.snapshot.as_mut().unwrap().vm_snapshots[0].0;
        match mutation {
            0 => vm.boundary_state.pending_io = SnapshotPendingIoState::Pending { vcpu: 0 },
            1 => vm.boundary_state.delivery_state = SnapshotDeliveryState::Incomplete,
            2 => vm.boundary_state.injection_ledger.prefix_digest = [0; 32],
            _ => {
                vm.metadata
                    .as_mut()
                    .unwrap()
                    .execution_descriptor
                    .host
                    .kernel_release = "wrong-host".into()
            }
        }
        assert!(validate_checkpoint(&recording, &checkpoint, &config).is_err());
        if mutation < 3 {
            assert!(encode_snapshot_artifact(
                checkpoint.snapshot.as_ref().unwrap(),
                0,
                MAX_SESSION_SNAPSHOT_BYTES
            )
            .is_err());
        }
    }
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn coarse_boundary_rounds_reject_dropped_reordered_forged_and_old_schema_evidence() {
    let (config, recording) = coarse_smp_fixture();
    validate_recording(&recording).unwrap();
    for mutation in 0..6 {
        let mut changed = recording.clone();
        match mutation {
            0 => changed.schedule_rounds[0].traces[0].trace.records.clear(),
            1 => changed.schedule_rounds.swap(0, 1),
            2 => {
                if let chaoscontrol_vmm::scheduler::core::ScheduleEvent::KvmBoundaryObserved {
                    observed_boundaries,
                    ..
                } = &mut changed.schedule_rounds[0].traces[0].trace.records[0].event
                {
                    *observed_boundaries += 1;
                } else {
                    unreachable!();
                }
            }
            3 => {
                changed.schedule_rounds[0].traces[0].trace.records[0]
                    .post_state_id
                    .0[0] ^= 1
            }
            4 => changed.schedule_rounds[0].traces[0].trace.schema_version -= 1,
            _ => {
                changed.schedule_rounds.remove(0);
            }
        }
        assert!(validate_recording(&changed).is_err());
        let directory = tempfile::tempdir().unwrap();
        retain_exact_fixture(directory.path(), &config, &changed, None);
        assert!(load_session(directory.path(), SessionLimits::default()).is_err());
    }
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn coarse_missing_snapshot_and_changed_guest_artifact_never_boot_a_replacement() {
    let (config, recording) = coarse_smp_fixture();
    for missing_snapshot in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        retain_exact_fixture(directory.path(), &config, &recording, None);
        let session = load_session(directory.path(), SessionLimits::default()).unwrap();
        if missing_snapshot {
            std::fs::remove_file(
                directory
                    .path()
                    .join(&session.manifest().checkpoints[0].artifact.path),
            )
            .unwrap();
        } else {
            std::fs::write(
                directory
                    .path()
                    .join(&session.manifest().guest_artifacts[0].artifact.path),
                b"changed kernel",
            )
            .unwrap();
        }
        assert!(load_session(directory.path(), SessionLimits::default()).is_err());
        assert!(session.restore_controller(0).is_err());
    }
}

// r[verify chaoscontrol.kvm_debug_boundary.persistence]
#[test]
fn coarse_actual_output_is_the_recorded_suffix_not_checkpoint_text() {
    let (_, mut recording) = coarse_smp_fixture();
    use crate::recording::RecordedEvent;
    recording.events = vec![
        RecordedEvent::SerialOutput {
            tick: 10,
            vm_index: 0,
            data: "retained prefix".into(),
        },
        RecordedEvent::SerialOutput {
            tick: 11,
            vm_index: 0,
            data: "hello ".into(),
        },
        RecordedEvent::SerialOutput {
            tick: 12,
            vm_index: 0,
            data: "world\n".into(),
        },
        RecordedEvent::SerialOutput {
            tick: 16,
            vm_index: 0,
            data: "later".into(),
        },
    ];
    crate::replay::validate_replayed_serial_suffix(&recording, 10, 15, 0, "hello world\n").unwrap();
    for actual in [
        "",
        "hello world",
        "hello worLd\n",
        "retained prefixhello world\n",
        "hello world\nlater",
    ] {
        assert!(
            crate::replay::validate_replayed_serial_suffix(&recording, 10, 15, 0, actual).is_err()
        );
    }
}
