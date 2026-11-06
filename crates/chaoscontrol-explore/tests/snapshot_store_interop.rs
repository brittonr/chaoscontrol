//! Explorer bug carriers interoperate with the replay-owned snapshot store.
// r[verify chaoscontrol.replay_debugger.shared_store]
use chaoscontrol_replay::snapshot_store::{FileSnapshotStore, SnapshotStore};
use chaoscontrol_vmm::controller::{NetworkFabric, SimulationSnapshot};

#[test]
fn saved_bug_fixture_loads_restorable_parent_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileSnapshotStore::new(dir.path());
    let snapshot = SimulationSnapshot {
        tick: 99,
        vm_snapshots: Vec::new(),
        network_state: NetworkFabric::new(2, 42),
        fault_engine_snapshot: chaoscontrol_fault::engine::FaultEngine::new(Default::default())
            .snapshot(),
        vcpu_stall_until: vec![],
        cpu_throttle: vec![],
        clock_freeze: vec![],
        clock_jitter_bound: vec![],
        memory_pressure: vec![],
        process_fault_attempt: vec![],
        pending_process_observations: Default::default(),
        fault_operation_sequence: 0,
    };
    let reference = store.put_snapshot(&snapshot, 1).unwrap();
    let bug = chaoscontrol_explore::checkpoint::SerializableBug {
        bug_id: 7,
        assertion_id: 42,
        assertion_identity: None,
        fallback_scope: None,
        process_instances: Default::default(),
        marker_context: None,
        assertion_location: "fixture assertion".into(),
        schedule: (&chaoscontrol_fault::schedule::FaultSchedule::new()).into(),
        tick: 101,
        replay_parent_depth: 1,
        replay_parent_snapshot_ref: Some(reference.clone()),
        dedup_key: None,
        schedule_variant: None,
        scenario_config: None,
        scenario_summary: None,
        random_faults: false,
    };
    let bug_path = dir.path().join("bug_0.json");
    std::fs::write(&bug_path, serde_json::to_vec_pretty(&bug).unwrap()).unwrap();
    let loaded_bug: chaoscontrol_explore::checkpoint::SerializableBug =
        serde_json::from_slice(&std::fs::read(&bug_path).unwrap()).unwrap();
    let loaded = store
        .get_snapshot(loaded_bug.replay_parent_snapshot_ref.as_ref().unwrap())
        .unwrap();
    assert_eq!(loaded.tick, 99);
    assert_eq!(loaded.vm_snapshots.len(), 0);
    assert_eq!(loaded_bug.replay_parent_snapshot_ref.unwrap(), reference);
}
