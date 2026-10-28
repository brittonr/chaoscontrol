//! Product-type leakage fixtures at the Campaign boundary.
//!
//! The Campaign-owned records a journal persists, `MOMENT` (Choregraph moment
//! event) and `CONTROL` (Campaign control event), may carry only opaque
//! identities, digests of at most [`DIGEST_LENGTH`] bytes, integer ranks,
//! counters, and ticks, and closed-vocabulary field and variant names. They never
//! carry ChaosControl product values: snapshot bytes, coverage bitmaps, assertion
//! data, fault schedules, or finding keys.
//!
//! The positive fixture drives the Campaign backend with distinctive real product
//! values and checks every persisted Campaign record against that rule. The
//! negative fixtures inject each product value into a persisted Campaign record;
//! the journal decoder and epoch recovery refuse every injection with a typed
//! error, so a leaked product value never becomes Campaign or Choregraph history.

use super::digests::{coverage_digest, oracle_digest, schedule_digest, variant_digest};
use super::records::{CONTROL, MOMENT};
use super::*;
use crate::campaign_adapter::{decode_control_event, decode_moment, WireError};
use crate::explorer::journal::{
    decode_segment, epoch_directory_name, CampaignJournal, JournalRecord, SystemPublicationIo,
    JOURNAL_DIRECTORY_NAME,
};
use crate::explorer::{ExplorationMode, ExplorerConfig, FrontierPolicy};
use campaign_core::DIGEST_LENGTH;
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::{AssertionKind, AssertionRecord, OracleReport};
use chaoscontrol_fault::schedule::ScheduledFault;
use ciborium::value::Value;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::path::Path;

const SEED: u64 = 0x1eaf_5eed;
const ROUNDS: u64 = 8;
const MAX_FRONTIER: usize = 4;
const BRANCHES: usize = 2;
/// Rounds the positive fixture executes.
const EXECUTED_ROUNDS: u64 = 2;
const VM_COUNT: usize = 2;
const NETWORK_SEED: u64 = 42;
const ROOT_SCORES: [f64; 2] = [5.0, 3.0];
const CHILD_SCORES: [f64; 2] = [4.5, 2.5];
const BASE_SET: [u8; 32] = [4; 32];
/// Distinctive product values; none may reach a Campaign record.
const SNAPSHOT_TICK: u64 = 0x5a5a_0000_7e57_0001;
const BRANCH_TICKS: u64 = 0x0000_0000_0007_4e57;
const FAULT_TIME_NS: u64 = 0x0fa1_7000_0000_0003;
const FAULT_LATENCY_NS: u64 = 0x1a7e_0000_0000_0004;
const FINDING_DEDUP_KEY: u64 = 0x0dec_0de0_0000_0005;
const ASSERTION_MESSAGE: &str = "leak sentinel: two leaders elected in term 7";
const ASSERTION_DETAILS: &[u8] = b"{\"leak-sentinel\":\"term 7 leaders [0, 1]\"}";
/// Coverage counters the bitmap records at [`COVERAGE_OFFSET`], in order.
const COVERAGE_PATTERN: [u8; 16] = [
    0x11, 0x2c, 0x03, 0x7f, 0x05, 0x61, 0x17, 0x08, 0x39, 0x0a, 0x4b, 0x0c, 0x5d, 0x0e, 0x6f, 0x10,
];
const COVERAGE_OFFSET: usize = 4_096;
/// Longest closed-vocabulary name a Campaign record may carry.
const MAX_RECORD_NAME_BYTES: usize = 64;

/// Field names a leaking writer would use for each product value.
const SNAPSHOT_FIELD: &str = "snapshot";
const COVERAGE_FIELD: &str = "coverage_bitmap";
const ASSERTION_FIELD: &str = "assertions";
const SCHEDULE_FIELD: &str = "fault_schedule";

/// One run's distinctive product values and the digests the shell derives.
struct ProductValues {
    snapshot: SimulationSnapshot,
    coverage: CoverageBitmap,
    schedule: FaultSchedule,
    oracle: OracleReport,
    finding: FindingFacts,
    digests: BranchDigests,
}

impl ProductValues {
    fn new() -> Self {
        let engine = chaoscontrol_fault::engine::FaultEngine::new(
            chaoscontrol_fault::engine::EngineConfig::default(),
        );
        let snapshot = SimulationSnapshot {
            tick: SNAPSHOT_TICK,
            vm_snapshots: Vec::new(),
            network_state: chaoscontrol_vmm::controller::NetworkFabric::new(VM_COUNT, NETWORK_SEED),
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: Vec::new(),
            cpu_throttle: Vec::new(),
            clock_freeze: Vec::new(),
            clock_jitter_bound: Vec::new(),
            memory_pressure: Vec::new(),
            process_fault_attempt: Vec::new(),
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
        };
        let mut coverage = CoverageBitmap::new();
        for (offset, count) in COVERAGE_PATTERN.iter().enumerate() {
            for _ in 0..*count {
                coverage.record_hit(COVERAGE_OFFSET + offset);
            }
        }
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(
            FAULT_TIME_NS,
            Fault::NetworkLatency {
                target: 1,
                latency_ns: FAULT_LATENCY_NS,
            },
        ));
        let mut oracle = OracleReport {
            total_runs: 1,
            ..OracleReport::empty()
        };
        oracle.assertions.insert(
            7,
            AssertionRecord {
                message: ASSERTION_MESSAGE.to_owned(),
                kind: AssertionKind::Always,
                hit_count: 3,
                true_count: 2,
                false_count: 1,
                runs_hit: 1,
                runs_satisfied: 0,
                first_failure_run: Some(0),
                last_failure_details: Some(ASSERTION_DETAILS.to_vec()),
                guest: "raft".to_owned(),
                category: "safety".to_owned(),
                identity: None,
                compatibility_id: Some(7),
                must_hit: None,
                catalog_tokens: std::collections::BTreeSet::new(),
                vm_instances: std::collections::BTreeSet::new(),
                process_instances: std::collections::BTreeSet::new(),
                fallback_scope: None,
            },
        );
        let digests = BranchDigests {
            schedule: schedule_digest(&schedule).unwrap(),
            variant: variant_digest(None).unwrap(),
            coverage: coverage_digest(&coverage),
            oracle: oracle_digest(&oracle).unwrap(),
            total_ticks: BRANCH_TICKS,
            snapshot_tick: BRANCH_TICKS,
        };
        let finding = FindingFacts {
            dedup_key: FINDING_DEDUP_KEY,
            assertion_fingerprint: digests.oracle,
        };
        Self {
            snapshot,
            coverage,
            schedule,
            oracle,
            finding,
            digests,
        }
    }

    /// Each product value as a leaking writer would embed it in a record.
    fn leaked(&self) -> [(&'static str, Value); 4] {
        [
            (
                SNAPSHOT_FIELD,
                Value::Bytes(serde_json::to_vec(&self.snapshot).unwrap()),
            ),
            (
                COVERAGE_FIELD,
                Value::Bytes(self.coverage.as_slice().to_vec()),
            ),
            (ASSERTION_FIELD, Value::serialized(&self.oracle).unwrap()),
            (
                SCHEDULE_FIELD,
                Value::serialized(&self.schedule.snapshot()).unwrap(),
            ),
        ]
    }

    /// Byte strings whose presence in a record proves a product value leaked.
    fn sentinels(&self) -> Vec<(&'static str, Vec<u8>)> {
        let mut sentinels = vec![
            ("coverage counters", COVERAGE_PATTERN.to_vec()),
            ("assertion message", ASSERTION_MESSAGE.as_bytes().to_vec()),
            ("assertion details", ASSERTION_DETAILS.to_vec()),
            ("snapshot tick", cbor_bytes(&Value::from(SNAPSHOT_TICK))),
            ("fault time", cbor_bytes(&Value::from(FAULT_TIME_NS))),
            ("fault latency", cbor_bytes(&Value::from(FAULT_LATENCY_NS))),
            ("branch ticks", cbor_bytes(&Value::from(BRANCH_TICKS))),
            ("finding key", cbor_bytes(&Value::from(FINDING_DEDUP_KEY))),
        ];
        for (name, value) in self.leaked() {
            sentinels.push((name, cbor_bytes(&value)));
        }
        sentinels
    }

    fn root(&self) -> RootAdmission {
        RootAdmission {
            snapshot: Some(self.snapshot.clone()),
            coverage: self.coverage.clone(),
            schedule: self.schedule.clone(),
            proposals: ROOT_SCORES.iter().copied().map(proposal).collect(),
            digests: Some(self.digests),
            base_set_id: BASE_SET,
        }
    }

    fn observation(&self) -> RoundObservation {
        RoundObservation {
            branches: (0..BRANCHES)
                .map(|index| BranchOutcome {
                    slot: BranchSlot::Variant(u32::try_from(index).unwrap()),
                    new_edges: COVERAGE_PATTERN.len(),
                    snapshot_present: true,
                    findings: if index == 0 {
                        vec![self.finding]
                    } else {
                        Vec::new()
                    },
                    retained: vec![RetainedBranch {
                        snapshot: self.snapshot.clone(),
                        coverage: self.coverage.clone(),
                        schedule: self.schedule.clone(),
                        depth: 1,
                        proposals: vec![proposal(CHILD_SCORES[index])],
                        digests: Some(self.digests),
                    }],
                })
                .collect(),
        }
    }
}

fn proposal(score: f64) -> EntryProposal {
    EntryProposal {
        score,
        marker: None,
        assertion: None,
    }
}

fn config(output: &Path) -> ExplorerConfig {
    ExplorerConfig {
        seed: SEED,
        max_rounds: ROUNDS,
        max_frontier: MAX_FRONTIER,
        branch_factor: BRANCHES,
        exploration_mode: ExplorationMode::FaultSchedule,
        output_dir: Some(output.display().to_string()),
        frontier_policy: FrontierPolicy::Campaign,
        ..ExplorerConfig::default()
    }
}

fn open(output: &Path) -> Result<CampaignFrontier<SystemPublicationIo>, FrontierError> {
    CampaignFrontier::open_with_io(&config(output), output, None, SystemPublicationIo)
}

fn cbor_bytes(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).unwrap();
    bytes
}

fn cbor_value(bytes: &[u8]) -> Value {
    ciborium::de::from_reader(bytes).unwrap()
}

/// Runs [`EXECUTED_ROUNDS`] rounds over `product` into a journal under `output`.
fn explore(output: &Path, product: &ProductValues) {
    let mut frontier = open(output).unwrap();
    frontier.admit_root(product.root()).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    for round in 0..EXECUTED_ROUNDS {
        let context = RoundContext {
            round,
            mode: RoundMode::FaultSchedule,
        };
        let planned = match frontier.begin_round(&mut rng, context).unwrap() {
            RoundBegin::Planned(planned) => planned,
            RoundBegin::Stop(stop) => panic!("unexpected stop {stop:?}"),
        };
        let Authorization::Execute(authorized) = frontier.authorize(planned).unwrap() else {
            panic!("the retained parent snapshot is eligible");
        };
        frontier
            .finish_round(*authorized, &product.observation())
            .unwrap();
    }
}

/// Every Campaign-owned record (`MOMENT`, `CONTROL`) of epoch 0, in order.
fn campaign_records(output: &Path) -> Vec<JournalRecord> {
    let epoch = output
        .join(JOURNAL_DIRECTORY_NAME)
        .join(epoch_directory_name(0));
    let mut segments: Vec<_> = std::fs::read_dir(epoch)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "cbor")
        })
        .collect();
    segments.sort();
    segments
        .iter()
        .flat_map(|path| {
            let bytes = std::fs::read(path).unwrap();
            decode_segment(&bytes).unwrap().records().to_vec()
        })
        .filter(|record| record.kind() == MOMENT || record.kind() == CONTROL)
        .collect()
}

fn is_record_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_RECORD_NAME_BYTES
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Where a Campaign record carries something other than an opaque identity, a
/// digest, an integer, a flag, or a closed-vocabulary name.
fn product_shaped(value: &Value, at: &str, found: &mut Vec<String>) {
    match value {
        Value::Integer(_) | Value::Bool(_) | Value::Null => {}
        Value::Bytes(bytes) if bytes.len() <= DIGEST_LENGTH => {}
        Value::Bytes(bytes) => found.push(format!("{at}: {} bytes", bytes.len())),
        Value::Float(_) => found.push(format!("{at}: float")),
        Value::Text(text) if is_record_name(text) => {}
        Value::Text(text) => found.push(format!("{at}: text of {} bytes", text.len())),
        Value::Array(items)
            if items.len() > DIGEST_LENGTH
                && items.iter().all(|item| matches!(item, Value::Integer(_))) =>
        {
            found.push(format!("{at}: integer array of {}", items.len()));
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                product_shaped(item, &format!("{at}[{index}]"), found);
            }
        }
        Value::Map(entries) => {
            for (key, item) in entries {
                let name = key.as_text().unwrap_or("<non-text key>");
                if !key.as_text().is_some_and(is_record_name) {
                    found.push(format!("{at}: key {name:?}"));
                }
                product_shaped(item, &format!("{at}.{name}"), found);
            }
        }
        Value::Tag(_, inner) => product_shaped(inner, at, found),
        _ => found.push(format!("{at}: unsupported CBOR value")),
    }
}

fn product_shapes(bytes: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    product_shaped(&cbor_value(bytes), "", &mut found);
    found
}

fn decodes(record: &JournalRecord) -> Result<(), WireError> {
    if record.kind() == MOMENT {
        decode_moment(record.payload()).map(|_| ())
    } else {
        decode_control_event(record.payload()).map(|_| ())
    }
}

fn map_field<'a>(value: &'a mut Value, key: &str) -> &'a mut Value {
    let Value::Map(entries) = value else {
        panic!("expected a CBOR map at {key}");
    };
    entries
        .iter_mut()
        .find(|(name, _)| name.as_text() == Some(key))
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing field {key}"))
}

fn with_extra_field(bytes: &[u8], key: &str, leaked: Value) -> Vec<u8> {
    let mut value = cbor_value(bytes);
    let Value::Map(entries) = &mut value else {
        panic!("a Campaign record is a CBOR map");
    };
    entries.push((Value::Text(key.to_owned()), leaked));
    cbor_bytes(&value)
}

fn with_payload(bytes: &[u8], leaked: Value) -> Vec<u8> {
    let mut value = cbor_value(bytes);
    *map_field(&mut value, "payload") = leaked;
    cbor_bytes(&value)
}

/// The honest payload digest as a byte string with a product value appended.
fn appended(bytes: &[u8], leaked: &Value) -> Value {
    let mut value = cbor_value(bytes);
    let Value::Array(digest) = map_field(&mut value, "payload") else {
        panic!("the payload digest is an integer array");
    };
    let mut smuggled: Vec<u8> = digest
        .iter()
        .map(|byte| u8::try_from(byte.as_integer().unwrap()).unwrap())
        .collect();
    smuggled.extend(cbor_bytes(leaked));
    Value::Bytes(smuggled)
}

/// First [`DIGEST_LENGTH`] bytes of a leaked value: a product value shaped
/// like a digest.
fn digest_shaped(leaked: &Value) -> Value {
    let bytes = cbor_bytes(leaked);
    let mut shaped = [0u8; DIGEST_LENGTH];
    let taken = bytes.len().min(DIGEST_LENGTH);
    shaped[..taken].copy_from_slice(&bytes[..taken]);
    Value::Array(shaped.iter().map(|byte| Value::from(*byte)).collect())
}

/// Appends `record` to a fresh honest journal and reopens it.
fn recover_with(record: JournalRecord) -> Result<(), FrontierError> {
    let dir = tempfile::tempdir().unwrap();
    explore(dir.path(), &ProductValues::new());
    {
        let (mut journal, _) = CampaignJournal::open(dir.path()).unwrap();
        journal.append(&[record]).unwrap();
    }
    open(dir.path()).map(|_| ())
}

fn replay_failure(result: Result<(), FrontierError>) -> ReplayFailure {
    match result {
        Err(FrontierError::Replay(error)) => error.failure,
        Err(other) => panic!("refused outside epoch replay: {other}"),
        Ok(()) => panic!("a leaked product value replayed as Campaign history"),
    }
}

// r[verify chaoscontrol.campaign_policy.evidence.valid]
// r[verify chaoscontrol.campaign_policy.adapter.valid]
#[test]
fn campaign_records_carry_only_opaque_identities_digests_ranks_and_ticks() {
    let dir = tempfile::tempdir().unwrap();
    let product = ProductValues::new();
    explore(dir.path(), &product);
    let records = campaign_records(dir.path());
    let moments = records
        .iter()
        .filter(|record| record.kind() == MOMENT)
        .count();
    // Root plus one child per branch per round.
    let expected_moments = 1 + usize::try_from(EXECUTED_ROUNDS).unwrap() * BRANCHES;
    assert_eq!(moments, expected_moments);
    assert!(records.len() > moments, "control events are journaled");
    let sentinels = product.sentinels();
    for (index, record) in records.iter().enumerate() {
        assert_eq!(decodes(record), Ok(()), "record {index}");
        assert_eq!(
            product_shapes(record.payload()),
            Vec::<String>::new(),
            "record {index}"
        );
        for (name, sentinel) in &sentinels {
            assert!(
                !record
                    .payload()
                    .windows(sentinel.len())
                    .any(|window| window == sentinel.as_slice()),
                "record {index} carries the {name}"
            );
        }
    }
    // The same journal recovers: honest records are accepted at the boundary
    // that refuses the leaks below.
    assert!(open(dir.path()).is_ok());
}

// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn product_values_injected_into_campaign_records_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let product = ProductValues::new();
    explore(dir.path(), &product);
    let records = campaign_records(dir.path());
    let control = records
        .iter()
        .find(|record| record.kind() == CONTROL)
        .unwrap();
    let moment = records
        .iter()
        .find(|record| record.kind() == MOMENT)
        .unwrap();
    for (field, leaked) in product.leaked() {
        // A product value beside the control event.
        let bytes = with_extra_field(control.payload(), field, leaked.clone());
        assert_eq!(
            decode_control_event(&bytes),
            Err(WireError::Decode),
            "{field}"
        );
        // A product value beside the moment event.
        let bytes = with_extra_field(moment.payload(), field, leaked.clone());
        assert_eq!(decode_moment(&bytes), Err(WireError::Decode), "{field}");
        // A product value in place of the moment payload digest.
        let bytes = with_payload(moment.payload(), leaked.clone());
        assert_eq!(decode_moment(&bytes), Err(WireError::Decode), "{field}");
        // A product value appended to the honest payload digest.
        let bytes = with_payload(moment.payload(), appended(moment.payload(), &leaked));
        assert_eq!(decode_moment(&bytes), Err(WireError::Decode), "{field}");
        // A product value cut to the shape of a digest: identities re-derive.
        let bytes = with_payload(moment.payload(), digest_shaped(&leaked));
        assert_eq!(product_shapes(&bytes), Vec::<String>::new(), "{field}");
        assert_eq!(
            decode_moment(&bytes),
            Err(WireError::IdentityMismatch),
            "{field}"
        );
    }
    // Recovery refuses a journal that persisted a leaked record.
    let [(field, snapshot), _, (_, assertions), _] = product.leaked();
    assert_eq!(field, SNAPSHOT_FIELD);
    let leaked_control = JournalRecord::new(
        CONTROL,
        with_extra_field(control.payload(), ASSERTION_FIELD, assertions),
    );
    assert_eq!(
        replay_failure(recover_with(leaked_control)),
        ReplayFailure::Wire(WireError::Decode)
    );
    let leaked_moment = JournalRecord::new(
        MOMENT,
        with_payload(moment.payload(), appended(moment.payload(), &snapshot)),
    );
    assert_eq!(
        replay_failure(recover_with(leaked_moment)),
        ReplayFailure::Wire(WireError::Decode)
    );
    let shaped_moment = JournalRecord::new(
        MOMENT,
        with_payload(moment.payload(), digest_shaped(&snapshot)),
    );
    assert_eq!(
        replay_failure(recover_with(shaped_moment)),
        ReplayFailure::Wire(WireError::IdentityMismatch)
    );
}
