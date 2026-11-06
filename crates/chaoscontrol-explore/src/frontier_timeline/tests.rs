use super::*;
use crate::coverage::CoverageBitmap;
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_replay::event_query::render_timeline_dot;
use chaoscontrol_vmm::controller::{NetworkFabric, SimulationSnapshot};

fn entry(id: u64, parent: Option<u64>, tick: u64, depth: u32) -> FrontierEntry {
    FrontierEntry {
        id,
        parent,
        depth,
        score: 1.0,
        times_selected: 0,
        marker: None,
        assertion: None,
        coverage: CoverageBitmap::new(),
        schedule: FaultSchedule::new(),
        snapshot: SimulationSnapshot {
            tick,
            vm_snapshots: vec![],
            network_state: NetworkFabric::new(2, 42),
            fault_engine_snapshot: FaultEngine::new(EngineConfig::default()).snapshot(),
            vcpu_stall_until: vec![],
            cpu_throttle: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            memory_pressure: vec![],
            process_fault_attempt: vec![],
            fault_operation_sequence: 0,
            pending_process_observations: Default::default(),
        },
    }
}

#[test]
fn real_frontier_entries_keep_exact_ticks_parentage_and_checkpoint_facts() {
    let entries = [entry(10, None, 3, 0), entry(42, Some(10), 17, 1)];
    let projection =
        timeline_from_frontier_entries([4; 32], &entries, &TimelineLimits::default()).unwrap();
    assert_eq!(projection.source_id, [4; 32]);
    assert_eq!(projection.facts[0].id, 10);
    assert_eq!(projection.facts[0].tick, 3);
    assert_eq!(projection.facts[1].id, 42);
    assert_eq!(projection.facts[1].parent, Some(10));
    assert_eq!(projection.facts[1].tick, 17);
    assert_eq!(projection.facts[1].kind, "frontier_checkpoint");
    assert!(!projection.truncated);
    assert!(projection.missing_artifacts.is_empty());
    assert!(render_timeline_dot(&projection.facts)
        .unwrap()
        .contains("n10 -> n42"));
    assert_eq!(
        timeline_from_frontier_entries([4; 32], &entries[1..], &TimelineLimits::default())
            .unwrap_err(),
        EventQueryError::MissingTimelineParent
    );
    let mut wrong_depth = entries.clone();
    wrong_depth[1].depth = 5;
    assert_eq!(
        timeline_from_frontier_entries([4; 32], &wrong_depth, &TimelineLimits::default())
            .unwrap_err(),
        EventQueryError::InvalidEvent
    );
    let bounds = TimelineLimits {
        max_nodes: 1,
        ..TimelineLimits::default()
    };
    assert_eq!(
        timeline_from_frontier_entries([4; 32], &entries, &bounds).unwrap_err(),
        EventQueryError::TimelineBound
    );
    let bounds = TimelineLimits {
        max_bytes: 1,
        ..TimelineLimits::default()
    };
    assert_eq!(
        timeline_from_frontier_entries([4; 32], &entries, &bounds).unwrap_err(),
        EventQueryError::TimelineByteBound
    );
}

fn retained_trace(stop: bool) -> Vec<u8> {
    // Literal retained producer-format records, not a parallel adapter schema.
    let mut bytes = concat!(
        "{\"kind\":\"header\",\"schema\":\"chaoscontrol.frontier-trace.v1\",\"policy\":\"legacy-diagnostic\",\"seed\":42,\"max_rounds\":1,\"max_frontier\":16,\"branch_factor\":1,\"stale_round_limit\":0,\"mode\":\"fault-schedule\",\"journal\":null}\n",
        "{\"kind\":\"root\",\"proposals\":[],\"retained\":[]}\n"
    ).as_bytes().to_vec();
    if stop {
        bytes.extend_from_slice(
            b"{\"kind\":\"stop\",\"stop\":\"frontier_exhausted\",\"rounds_completed\":0}\n",
        );
    }
    bytes
}

#[test]
fn retained_trace_reports_missing_time_parentage_and_truncation_without_fake_nodes() {
    let bytes = retained_trace(true);
    let projection = timeline_from_frontier_trace(&bytes, &TimelineLimits::default()).unwrap();
    assert_eq!(projection.source_id, *blake3::hash(&bytes).as_bytes());
    assert_eq!(projection.records, 3);
    assert!(!projection.truncated);
    assert!(projection.facts.is_empty());
    assert!(projection
        .missing_artifacts
        .iter()
        .any(|note| note.contains("simulation ticks absent")));
    assert!(projection
        .missing_artifacts
        .iter()
        .any(|note| note.contains("branch parentage")));
    let prefix =
        timeline_from_frontier_trace(&retained_trace(false), &TimelineLimits::default()).unwrap();
    assert!(prefix.truncated);
    assert!(prefix.facts.is_empty());
    assert!(prefix
        .missing_artifacts
        .iter()
        .any(|note| note.contains("stop record missing")));
    let mut malformed = bytes;
    malformed.extend_from_slice(b"{\"kind\":");
    assert!(timeline_from_frontier_trace(&malformed, &TimelineLimits::default()).is_err());
    let bounds = TimelineLimits {
        max_bytes: 1,
        ..TimelineLimits::default()
    };
    assert_eq!(
        timeline_from_frontier_trace(&retained_trace(true), &bounds).unwrap_err(),
        EventQueryError::TimelineByteBound
    );
}
