//! In-process Raft consensus simulation for ChaosControl exploration.
//!
//! Runs 3 Raft nodes in a single process as PID 1 inside the VM.
//! All randomness flows through the ChaosControl SDK so the VMM can
//! systematically explore message orderings, election timing, and
//! fault injection schedules.
//!
//! Fault injection is done at the application level: per-node crashes,
//! per-link partitions, message reordering, and message duplication.
//! Every fault decision uses `random_choice()` so the input-tree explorer
//! can systematically enumerate fault combinations.
//!
//! Safety invariants checked every tick:
//! - **Election Safety**: at most one leader per term
//! - **Log Matching**: if two logs agree at index i, they agree on all j < i
//! - **Leader Completeness**: committed entries are never lost
//! - **Data Integrity**: committed log entries are never overwritten

use chaoscontrol_raft_guest::{
    check_election_safety, check_leader_completeness, check_log_matching, quorum_for, BugMode,
    LogEntry, Message, Node, RaftSmrAdapter, Role, ELECTION_TIMEOUT_BASE, ELECTION_TIMEOUT_JITTER,
    HEARTBEAT_INTERVAL, NUM_NODES,
};
use chaoscontrol_sdk::assert::details;
use chaoscontrol_sdk::prelude::*;
use chaoscontrol_sdk::{coverage, kcov, lifecycle, random};
use chaoscontrol_smr::{
    admit_profile, validate_history, LivenessProfile, ObservationMode, SafetyVerdict,
    SmrConsumerAdapter, SmrWorkloadPlan, SmrWorkloadProfile, WorkloadBounds, PROFILE_SCHEMA,
    SUPPORTED_MAXIMUM_COMMANDS, SUPPORTED_MAXIMUM_EVIDENCE_BYTES, SUPPORTED_MAXIMUM_FAULT_ACTIONS,
    SUPPORTED_MAXIMUM_REDUCTION_ATTEMPTS, SUPPORTED_MAXIMUM_REPLAY_EVENTS,
    SUPPORTED_MAXIMUM_TRACE_EVENTS, SUPPORTED_MAXIMUM_VIRTUAL_PROGRESS,
};
use serde_json::json;

const RAFT_SMR_INITIAL_STATE_REF: &str =
    "blake3:0000000000000000000000000000000000000000000000000000000000000000";
const RAFT_SMR_MAXIMUM_COMMAND_BYTES: u64 = std::mem::size_of::<u64>() as u64;
const RAFT_SMR_MAXIMUM_CLIENTS: u32 = 9;
const RAFT_SMR_MAXIMUM_CONCURRENCY: u32 = RAFT_SMR_MAXIMUM_CLIENTS;
const RAFT_SMR_LIVENESS_HORIZON: u64 = 1_000;

fn raft_smr_plan() -> SmrWorkloadPlan {
    admit_profile(&SmrWorkloadProfile {
        schema: PROFILE_SCHEMA.to_string(),
        profile_id: "raft-guest-smr-chain-v1".to_string(),
        initial_state_ref: RAFT_SMR_INITIAL_STATE_REF.to_string(),
        observation_mode: ObservationMode::Lossless,
        bounds: WorkloadBounds {
            maximum_commands: SUPPORTED_MAXIMUM_COMMANDS,
            maximum_command_bytes: RAFT_SMR_MAXIMUM_COMMAND_BYTES,
            maximum_clients: RAFT_SMR_MAXIMUM_CLIENTS,
            maximum_concurrency: RAFT_SMR_MAXIMUM_CONCURRENCY,
            maximum_virtual_progress: SUPPORTED_MAXIMUM_VIRTUAL_PROGRESS,
            maximum_trace_events: SUPPORTED_MAXIMUM_TRACE_EVENTS,
            maximum_fault_actions: SUPPORTED_MAXIMUM_FAULT_ACTIONS,
            maximum_replay_events: SUPPORTED_MAXIMUM_REPLAY_EVENTS,
            maximum_evidence_bytes: SUPPORTED_MAXIMUM_EVIDENCE_BYTES,
            maximum_reduction_attempts: SUPPORTED_MAXIMUM_REDUCTION_ATTEMPTS,
        },
        liveness: LivenessProfile {
            profile_id: "raft-guest-recovered-quorum-v1".to_string(),
            required_progress: 1,
            virtual_progress_horizon: RAFT_SMR_LIVENESS_HORIZON,
        },
    })
    .expect("the static Raft SMR profile must be valid")
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::Follower => "follower",
        Role::Candidate => "candidate",
        Role::Leader => "leader",
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Kernel cmdline parsing
// ═══════════════════════════════════════════════════════════════════════

/// Parse `raft_bug=NAME` from /proc/cmdline.
fn parse_bug_mode() -> BugMode {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    for token in cmdline.split_whitespace() {
        if let Some(val) = token.strip_prefix("raft_bug=") {
            return BugMode::parse(val);
        }
    }
    BugMode::None
}

/// Parse `raft_nodes=N` from /proc/cmdline. Default: NUM_NODES (3).
fn parse_num_nodes() -> usize {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    for token in cmdline.split_whitespace() {
        if let Some(val) = token.strip_prefix("raft_nodes=") {
            if let Ok(n) = val.parse::<usize>() {
                if (3..=9).contains(&n) && n % 2 == 1 {
                    return n;
                }
            }
        }
    }
    NUM_NODES
}

/// Parse `raft_snapshot_probe_fail_after=N` from /proc/cmdline.
fn parse_snapshot_probe_fail_after() -> usize {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    for token in cmdline.split_whitespace() {
        if let Some(val) = token.strip_prefix("raft_snapshot_probe_fail_after=") {
            if let Ok(n) = val.parse::<usize>() {
                return n;
            }
        }
    }
    25
}

// ═══════════════════════════════════════════════════════════════════════
//  Fault state
// ═══════════════════════════════════════════════════════════════════════

/// Per-node crash state and per-link partition matrix.
struct FaultState {
    num_nodes: usize,
    /// Whether each node is currently crashed.
    crashed: Vec<bool>,
    /// Asymmetric partition matrix: partitioned[a][b] = messages from a→b dropped.
    partitioned: Vec<Vec<bool>>,
    /// Delayed messages: (from, to, message, deliver_at_tick).
    delay_queue: Vec<(usize, usize, Message, usize)>,
}

impl FaultState {
    fn new(num_nodes: usize) -> Self {
        Self {
            num_nodes,
            crashed: vec![false; num_nodes],
            partitioned: vec![vec![false; num_nodes]; num_nodes],
            delay_queue: Vec::new(),
        }
    }

    /// Count of active partitions.
    fn partition_count(&self) -> usize {
        self.partitioned
            .iter()
            .flat_map(|row| row.iter())
            .filter(|&&p| p)
            .count()
    }

    /// N-bit bitmap: bit i set if node i is alive.
    fn alive_mask(&self) -> usize {
        let mut mask = 0;
        for i in 0..self.num_nodes {
            if !self.crashed[i] {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// Count of alive nodes.
    fn alive_count(&self) -> usize {
        self.crashed.iter().filter(|&&c| !c).count()
    }

    /// Whether a majority of nodes can reach each other (no majority-isolating partition).
    fn quorum_reachable(&self) -> bool {
        let quorum = quorum_for(self.num_nodes);
        if self.alive_count() < quorum {
            return false;
        }
        // Check that at least 2 alive nodes can talk bidirectionally.
        let alive: Vec<usize> = (0..self.num_nodes).filter(|&i| !self.crashed[i]).collect();
        for &a in &alive {
            for &b in &alive {
                if a != b && !self.partitioned[a][b] && !self.partitioned[b][a] {
                    return true; // Found a pair that can communicate bidirectionally
                }
            }
        }
        false
    }
}

/// Snapshot of committed values for data integrity checking.
/// Maps (node_id, log_index) → value at the time it was first committed.
struct CommittedValues {
    /// committed_at[node][index] = value when first observed as committed.
    values: Vec<Vec<Option<u64>>>,
}

impl CommittedValues {
    fn new(num_nodes: usize) -> Self {
        Self {
            values: vec![Vec::new(); num_nodes],
        }
    }

    /// Record current committed entries and check none have changed.
    /// Returns true if all committed entries match their previously recorded values.
    fn check_and_record(&mut self, nodes: &[Node]) -> Vec<(usize, usize, u64, u64)> {
        let mut violations = Vec::new();
        for (node_id, node) in nodes.iter().enumerate() {
            // Extend storage if needed
            while self.values[node_id].len() < node.log.len() {
                self.values[node_id].push(None);
            }

            // Check all committed entries
            for idx in 0..node.commit_index.min(node.log.len()) {
                let current_value = node.log[idx].value;
                match self.values[node_id][idx] {
                    Some(recorded) => {
                        if recorded != current_value {
                            violations.push((node_id, idx, recorded, current_value));
                        }
                    }
                    None => {
                        self.values[node_id][idx] = Some(current_value);
                    }
                }
            }
        }
        violations
    }
}

struct SmrCheckSummary {
    pass: bool,
    adapter_errors: Vec<String>,
    history_error: Option<String>,
    violations: usize,
    observations: usize,
}

struct RaftSmrRuntime {
    plan: SmrWorkloadPlan,
    adapters: Vec<RaftSmrAdapter>,
    checked_observations: usize,
    last_pass: bool,
    last_history_error: Option<String>,
    last_violations: usize,
}

impl RaftSmrRuntime {
    fn new(num_nodes: usize) -> Self {
        let plan = raft_smr_plan();
        let adapters = (0..num_nodes)
            .map(|node_id| {
                let mut adapter =
                    RaftSmrAdapter::new(plan.clone(), format!("raft-replica-{node_id}"));
                adapter.mark_ready();
                adapter
            })
            .collect();
        Self {
            plan,
            adapters,
            checked_observations: 0,
            last_pass: true,
            last_history_error: None,
            last_violations: 0,
        }
    }

    fn check(&mut self, nodes: &[Node]) -> SmrCheckSummary {
        let mut adapter_errors = Vec::new();
        for (node, adapter) in nodes.iter().zip(self.adapters.iter_mut()) {
            let committed = node.committed_application_values();
            if committed.len() >= adapter.observations().len() {
                if let Err(error) = adapter.observe_committed_values(&committed) {
                    adapter_errors.push(format!("replica={} {error}", node.id));
                }
            }
        }
        let observation_count = self
            .adapters
            .iter()
            .map(|adapter| adapter.observations().len())
            .sum();
        if adapter_errors.is_empty() && observation_count != self.checked_observations {
            let observations = self
                .adapters
                .iter()
                .flat_map(|adapter| adapter.observations().iter().cloned())
                .collect::<Vec<_>>();
            match validate_history(&self.plan, &observations) {
                Ok(report) => {
                    self.last_pass = report.verdict == SafetyVerdict::Pass;
                    self.last_history_error = None;
                    self.last_violations = report.violations.len();
                }
                Err(error) => {
                    self.last_pass = false;
                    self.last_history_error = Some(error.to_string());
                    self.last_violations = 0;
                }
            }
            self.checked_observations = observation_count;
        }
        SmrCheckSummary {
            pass: adapter_errors.is_empty() && self.last_pass,
            adapter_errors,
            history_error: self.last_history_error.clone(),
            violations: self.last_violations,
            observations: observation_count,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    guest_init();

    let bug = parse_bug_mode();
    let num_nodes = parse_num_nodes();
    let snapshot_probe_fail_after = parse_snapshot_probe_fail_after();
    println!(
        "raft: starting {}-node cluster (bug={})",
        num_nodes,
        bug.name()
    );

    lifecycle::setup_complete(
        &json!({"program": "raft-guest", "nodes": num_nodes, "bug": bug.name()}),
    );
    println!("raft: setup_complete (bug={})", bug.name());

    // Initialize nodes with the selected bug mode
    let mut nodes: Vec<Node> = (0..num_nodes)
        .map(|i| Node::new_with_config(i, num_nodes, bug))
        .collect();
    // Stagger initial election timers
    for (i, node) in nodes.iter_mut().enumerate() {
        node.election_timer = ELECTION_TIMEOUT_BASE + i * 3;
    }

    let mut faults = FaultState::new(num_nodes);
    let mut committed_values = CommittedValues::new(num_nodes);
    let mut smr_runtime = RaftSmrRuntime::new(num_nodes);
    let mut values_proposed = 0u64;
    let mut values_committed = 0usize;
    let mut tick = 0usize;
    let mut ticks_quorum_healthy = 0usize;
    let mut commit_at_healthy_start = 0usize;

    loop {
        // ── Fault injection: crashes and restarts ────────────
        #[allow(clippy::needless_range_loop)]
        for i in 0..num_nodes {
            if faults.crashed[i] {
                // Restart decision: ~2% per tick
                if random::random_choice(50) == 0 {
                    // Restart: keep persistent state (log, current_term, voted_for),
                    // reset volatile state
                    nodes[i].commit_index = 0;
                    nodes[i].role = Role::Follower;
                    nodes[i].election_timer = ELECTION_TIMEOUT_BASE;
                    nodes[i].next_index = vec![1; num_nodes];
                    nodes[i].match_index = vec![0; num_nodes];
                    nodes[i].votes_received = 0;
                    nodes[i].heartbeat_timer = 0;
                    nodes[i].inbox.clear();
                    faults.crashed[i] = false;

                    cc_assert_reachable_category!(
                        "raft",
                        "branch",
                        "node restarted",
                        &json!({"node": i, "tick": tick, "term": nodes[i].current_term,
                                "log_len": nodes[i].log.len()}),
                    );
                }
            } else {
                // Crash decision: ~0.5% per tick
                if random::random_choice(200) == 0 {
                    faults.crashed[i] = true;
                    nodes[i].inbox.clear();

                    cc_assert_reachable_category!(
                        "raft",
                        "branch",
                        "node crashed",
                        &json!({"node": i, "tick": tick, "term": nodes[i].current_term}),
                    );
                }
            }
        }

        // ── Fault injection: partitions ─────────────────────
        // Create new partitions
        if random::random_choice(300) == 0 {
            let src = random::random_choice(num_nodes);
            let dst = random::random_choice(num_nodes);
            if src != dst && !faults.partitioned[src][dst] {
                faults.partitioned[src][dst] = true;
                cc_assert_reachable_category!(
                    "raft",
                    "branch",
                    "link partitioned",
                    &json!({"src": src, "dst": dst, "tick": tick}),
                );
            }
        }

        // Heal existing partitions
        for src in 0..num_nodes {
            for dst in 0..num_nodes {
                if faults.partitioned[src][dst] && random::random_choice(30) == 0 {
                    faults.partitioned[src][dst] = false;
                    cc_assert_reachable_category!(
                        "raft",
                        "branch",
                        "partition healed",
                        &json!({"src": src, "dst": dst, "tick": tick}),
                    );
                }
            }
        }

        // ── Deliver delayed messages ────────────────────────
        let mut still_delayed = Vec::new();
        for (from, to, msg, deliver_at) in faults.delay_queue.drain(..) {
            if tick >= deliver_at {
                if !faults.crashed[to] && !faults.partitioned[from][to] {
                    nodes[to].inbox.push((from, msg));
                }
            } else {
                still_delayed.push((from, to, msg, deliver_at));
            }
        }
        faults.delay_queue = still_delayed;

        // ── Pick which node to activate this tick ────────────
        let active = random::random_choice(num_nodes);
        coverage::record_edge(6000 + tick * 7 + active * 3);

        // Skip crashed nodes
        if faults.crashed[active] {
            // ── Per-node state coverage ─────────────────────
            coverage::record_edge(9000 + faults.alive_mask() * 10 + faults.partition_count());

            tick += 1;
            // Still check safety invariants even with crashed nodes
            check_safety_invariants(
                &nodes,
                &mut committed_values,
                &mut smr_runtime,
                values_committed,
                active,
                tick,
            );
            kcov::collect();
            if tick.is_multiple_of(40) {
                print_status(&nodes, &faults, tick, values_proposed);
            }
            continue;
        }

        // ── Get jitter for this tick ─────────────────────────
        let jitter = random::random_choice(ELECTION_TIMEOUT_JITTER + 1);

        // ── Tick timers on active node ───────────────────────
        let mut outbox: Vec<(usize, usize, Message)> = Vec::new(); // (from, to, msg)

        {
            let node = &mut nodes[active];

            // Drain inbox
            let inbox: Vec<(usize, Message)> = node.inbox.drain(..).collect();
            for (from, msg) in inbox {
                // ── Handler reachability ─────────────────────
                match &msg {
                    Message::RequestVote {
                        term, candidate_id, ..
                    } => {
                        cc_assert_reachable_category!(
                            "raft",
                            "branch",
                            "request_vote handler",
                            &json!({"node": active, "from": from, "term": *term, "candidate_id": *candidate_id}),
                        );
                    }
                    Message::RequestVoteResponse { term, vote_granted } => {
                        cc_assert_reachable_category!(
                            "raft",
                            "branch",
                            "request_vote_response handler",
                            &json!({"node": active, "from": from, "term": *term, "vote_granted": *vote_granted}),
                        );
                    }
                    Message::AppendEntries { term, entries, .. } => {
                        cc_assert_reachable_category!(
                            "raft",
                            "branch",
                            "append_entries handler",
                            &json!({"node": active, "from": from, "term": *term, "entry_count": entries.len()}),
                        );
                    }
                    Message::AppendEntriesResponse {
                        term,
                        success,
                        match_index,
                    } => {
                        cc_assert_reachable_category!(
                            "raft",
                            "branch",
                            "append_entries_response handler",
                            &json!({"node": active, "from": from, "term": *term, "success": *success, "match_index": *match_index}),
                        );
                    }
                }

                // Capture pre-state for post-call checks
                let old_role = node.role;
                let old_term = node.current_term;
                let old_commit = node.commit_index;
                let is_ae = matches!(&msg, Message::AppendEntries { .. });
                let is_aer = matches!(&msg, Message::AppendEntriesResponse { .. });

                // Capture AE context for log conflict detection
                let ae_context = if let Message::AppendEntries {
                    prev_log_index,
                    entries,
                    ..
                } = &msg
                {
                    if !entries.is_empty() {
                        let old_terms: Vec<Option<u64>> = (0..entries.len())
                            .map(|i| {
                                let idx = *prev_log_index + i;
                                if idx < node.log.len() {
                                    Some(node.log[idx].term)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        let new_terms: Vec<u64> = entries.iter().map(|e| e.term).collect();
                        Some((*prev_log_index, old_terms, new_terms))
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Capture RequestVote candidate_id for voted_for check
                let rv_candidate = if let Message::RequestVote { candidate_id, .. } = &msg {
                    Some(*candidate_id)
                } else {
                    None
                };

                let replies = node.handle_message(from, msg, jitter);

                // ── State transitions ───────────────────────
                if node.current_term > old_term && node.role == Role::Follower {
                    cc_assert_reachable_category!(
                        "raft",
                        "branch",
                        "stepped down to follower",
                        &json!({"node": active, "old_term": old_term, "new_term": node.current_term}),
                    );
                }
                if old_role == Role::Candidate && node.role == Role::Follower && is_ae {
                    cc_assert_reachable_category!(
                        "raft",
                        "branch",
                        "candidate stepped down on append_entries",
                        &json!({"node": active, "term": node.current_term}),
                    );
                }
                if old_role == Role::Candidate && node.role == Role::Leader {
                    cc_assert_reachable_category!(
                        "raft",
                        "branch",
                        "candidate won election",
                        &json!({"node": active, "term": node.current_term}),
                    );
                }

                // ── Data invariant: commit_index bounded ────
                if node.commit_index != old_commit {
                    cc_assert_always_category!(
                        "raft",
                        "invariant",
                        node.commit_index <= node.log.len(),
                        "commit_index within log bounds",
                        &json!({"node": active, "commit_index": node.commit_index, "log_len": node.log.len()}),
                    );
                }

                // ── Liveness: commit advances after replication ──
                if is_aer && node.role == Role::Leader {
                    cc_assert_sometimes_category!(
                        "raft",
                        "branch",
                        node.commit_index > old_commit,
                        "leader commit advanced after replication",
                        &json!({"node": active, "old": old_commit, "new": node.commit_index}),
                    );
                }

                // ── Reply-based sometimes-pairs + invariants ─
                for (to, reply) in &replies {
                    match reply {
                        Message::RequestVoteResponse { vote_granted, .. } => {
                            cc_assert_sometimes_category!(
                                "raft",
                                "branch",
                                *vote_granted,
                                "vote granted",
                                &json!({"voter": active, "candidate": *to}),
                            );
                            cc_assert_sometimes_category!(
                                "raft",
                                "branch",
                                !*vote_granted,
                                "vote denied",
                                &json!({"voter": active, "candidate": *to}),
                            );
                            if let (true, Some(cid)) = (*vote_granted, rv_candidate) {
                                cc_assert_always_category!(
                                    "raft",
                                    "invariant",
                                    node.voted_for == Some(cid),
                                    "voted_for matches granted candidate",
                                    &json!({"node": active, "candidate_id": cid}),
                                );
                            }
                        }
                        Message::AppendEntriesResponse { success, .. } => {
                            cc_assert_sometimes_category!(
                                "raft",
                                "branch",
                                *success,
                                "append accepted",
                                &json!({"node": active, "from": from}),
                            );
                            cc_assert_sometimes_category!(
                                "raft",
                                "branch",
                                !*success,
                                "append rejected",
                                &json!({"node": active, "from": from}),
                            );
                        }
                        _ => {}
                    }
                }

                // ── Data invariants for leader processing AER ──
                if node.role == Role::Leader && is_aer {
                    cc_assert_always_category!(
                        "raft",
                        "invariant",
                        node.match_index[from] <= node.log.len(),
                        "match_index within bounds",
                        &json!({"leader": active, "peer": from, "match_index": node.match_index[from], "log_len": node.log.len()}),
                    );
                    cc_assert_always_category!(
                        "raft",
                        "invariant",
                        node.next_index[from] >= 1,
                        "next_index stays positive",
                        &json!({"leader": active, "peer": from, "next_index": node.next_index[from]}),
                    );
                }

                // ── Log conflict paths ──────────────────────
                let ae_accepted = replies.iter().any(|(_to, r)| {
                    matches!(r, Message::AppendEntriesResponse { success: true, .. })
                });
                if ae_accepted {
                    if let Some((prev_idx, ref old_terms, ref new_terms)) = ae_context {
                        let mut had_conflict = false;
                        let mut had_consistent = false;
                        let mut had_new = false;
                        for (old_opt, new_t) in old_terms.iter().zip(new_terms.iter()) {
                            match old_opt {
                                Some(old_t) if old_t != new_t => had_conflict = true,
                                Some(_) => had_consistent = true,
                                None => had_new = true,
                            }
                        }
                        if had_conflict {
                            cc_assert_reachable_category!(
                                "raft",
                                "branch",
                                "log conflict: truncated",
                                &json!({"node": active, "prev_log_index": prev_idx}),
                            );
                        }
                        if had_consistent {
                            cc_assert_reachable_category!(
                                "raft",
                                "branch",
                                "log entries consistent",
                                &json!({"node": active}),
                            );
                        }
                        if had_new {
                            cc_assert_reachable_category!(
                                "raft",
                                "branch",
                                "new entries appended",
                                &json!({"node": active, "count": old_terms.iter().filter(|t| t.is_none()).count()}),
                            );
                        }
                    }
                }

                for (to, reply) in replies {
                    outbox.push((active, to, reply));
                }
            }

            // Timer logic
            match node.role {
                Role::Follower | Role::Candidate => {
                    let timer_expired = node.election_timer == 0;
                    cc_assert_sometimes_category!(
                        "raft",
                        "branch",
                        timer_expired,
                        "election timeout fired",
                        &json!({"node": active}),
                    );
                    cc_assert_sometimes_category!(
                        "raft",
                        "branch",
                        !timer_expired,
                        "election timer decremented",
                        &json!({"node": active}),
                    );

                    if timer_expired {
                        cc_assert_reachable_category!(
                            "raft",
                            "branch",
                            "follower started election",
                            &json!({"node": active, "new_term": node.current_term + 1}),
                        );
                        let msgs = node.become_candidate(jitter);
                        coverage::record_state(&[("candidate", &node.id.to_string())]);
                        for (to, msg) in msgs {
                            outbox.push((active, to, msg));
                        }
                    } else {
                        node.election_timer -= 1;
                    }
                }
                Role::Leader => {
                    if node.heartbeat_timer == 0 {
                        node.heartbeat_timer = HEARTBEAT_INTERVAL;
                        let msgs = node.send_heartbeats();
                        for (to, msg) in msgs {
                            outbox.push((active, to, msg));
                        }
                    } else {
                        node.heartbeat_timer -= 1;
                    }

                    // Leader proposes a value sometimes
                    let proposed = random::random_choice(4) == 0;
                    if proposed {
                        values_proposed += 1;
                        let entry = LogEntry {
                            term: node.current_term,
                            value: values_proposed,
                        };
                        node.log.push(entry);
                        node.match_index[node.id] = node.log.len();
                        cc_assert_always_category!(
                            "raft",
                            "invariant",
                            node.match_index[node.id] == node.log.len(),
                            "leader self match_index tracks log",
                            &json!({"node": active, "match_index": node.match_index[node.id], "log_len": node.log.len()}),
                        );
                        cc_assert_always_category!(
                            "raft",
                            "invariant",
                            node.commit_index <= node.log.len(),
                            "commit_index within log bounds",
                            &json!({"node": active, "commit_index": node.commit_index, "log_len": node.log.len()}),
                        );
                        coverage::record_state(&[(
                            "values_proposed",
                            &values_proposed.to_string(),
                        )]);
                    }
                    cc_assert_sometimes_category!(
                        "raft",
                        "branch",
                        proposed,
                        "leader proposed value",
                        &json!({"node": active}),
                    );
                    cc_assert_sometimes_category!(
                        "raft",
                        "branch",
                        !proposed,
                        "leader skipped proposal",
                        &json!({"node": active}),
                    );
                }
            }
        }

        // ── Message delivery with per-link faults ───────────
        for (from, to, msg) in outbox {
            // Drop if destination is crashed
            if faults.crashed[to] {
                continue;
            }

            // Drop if partitioned
            if faults.partitioned[from][to] {
                coverage::record_state(&[
                    ("drop_from", &from.to_string()),
                    ("drop_to", &to.to_string()),
                ]);
                continue;
            }

            // Random drop (5% base rate)
            if random::random_choice(100) >= 95 {
                cc_assert_sometimes_category!(
                    "raft",
                    "branch",
                    true,
                    "message dropped",
                    &details::network(from, to, false),
                );
                coverage::record_state(&[
                    ("drop_from", &from.to_string()),
                    ("drop_to", &to.to_string()),
                ]);
                continue;
            }

            // Message reordering: ~5% chance
            if random::random_choice(20) == 0 {
                let delay = 1 + random::random_choice(5);
                faults
                    .delay_queue
                    .push((from, to, msg.clone(), tick + delay));
                cc_assert_reachable_category!(
                    "raft",
                    "branch",
                    "message reordered",
                    &json!({"from": from, "to": to, "delay": delay}),
                );
                // Still deliver the original (or not — let's just delay it)
                continue;
            }

            // Message duplication: ~2% chance
            if random::random_choice(50) == 0 {
                nodes[to].inbox.push((from, msg.clone()));
                nodes[to].inbox.push((from, msg));
                cc_assert_reachable_category!(
                    "raft",
                    "branch",
                    "message duplicated",
                    &json!({"from": from, "to": to}),
                );
                continue;
            }

            // Normal delivery
            cc_assert_sometimes_category!(
                "raft",
                "branch",
                true,
                "message delivered",
                &details::network(from, to, true),
            );
            nodes[to].inbox.push((from, msg));
        }

        // ── Track committed values ──────────────────────────
        let max_commit = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
        if max_commit > values_committed {
            values_committed = max_commit;
        }

        // ── Fault-aware liveness ────────────────────────────
        if faults.alive_count() == num_nodes && faults.quorum_reachable() {
            ticks_quorum_healthy += 1;
            if ticks_quorum_healthy == 1 {
                // Record commit level at start of healthy window
                commit_at_healthy_start = max_commit;
            }
            if ticks_quorum_healthy > 200 {
                cc_assert_sometimes_category!(
                    "raft",
                    "branch",
                    max_commit > commit_at_healthy_start,
                    "commits advance when quorum healthy",
                    &json!({"ticks_healthy": ticks_quorum_healthy,
                            "commit_now": max_commit,
                            "commit_at_start": commit_at_healthy_start}),
                );
            }
        } else {
            ticks_quorum_healthy = 0;
        }

        // ── Per-node state coverage ─────────────────────────
        coverage::record_edge(9000 + faults.alive_mask() * 10 + faults.partition_count());

        // ── Protocol-state coverage ─────────────────────────
        // These edges capture Raft-specific state dimensions that
        // structural coverage misses. They guide the explorer toward
        // scenarios with log divergence, term gaps, and multi-leader
        // transitions — the conditions needed to trigger subtle bugs
        // like fig8_commit.
        {
            // Leader count: 0, 1, or 2+ (split-brain)
            let leader_count = nodes
                .iter()
                .filter(|n| n.role == Role::Leader && !faults.crashed[n.id])
                .count();

            // Term spread: max_term - min_term across alive nodes.
            // High spread means nodes are out of sync.
            let alive_terms: Vec<u64> = nodes
                .iter()
                .filter(|n| !faults.crashed[n.id])
                .map(|n| n.current_term)
                .collect();
            let term_spread = if !alive_terms.is_empty() {
                alive_terms.iter().max().unwrap() - alive_terms.iter().min().unwrap()
            } else {
                0
            };

            // Log length divergence: max - min across all nodes.
            let max_log = nodes.iter().map(|n| n.log.len()).max().unwrap_or(0);
            let min_log = nodes.iter().map(|n| n.log.len()).min().unwrap_or(0);

            coverage::record_state(&[
                ("leaders", &leader_count.min(3).to_string()),
                ("term_spread", &term_spread.min(10).to_string()),
                ("log_divergence", &(max_log - min_log).min(20).to_string()),
            ]);

            // Term diversity in each node's log: how many distinct terms.
            for (i, node) in nodes.iter().enumerate() {
                if node.log.is_empty() {
                    continue;
                }
                let mut seen = [false; 64];
                let mut distinct = 0usize;
                for e in &node.log {
                    let bucket = (e.term as usize) % 64;
                    if !seen[bucket] {
                        seen[bucket] = true;
                        distinct += 1;
                    }
                }
                coverage::record_state(&[
                    ("node_term_diversity", &i.to_string()),
                    ("distinct_terms", &distinct.min(15).to_string()),
                ]);
            }

            // Old-term entries in leader's log (the fig8 precondition).
            if let Some(leader) = nodes
                .iter()
                .find(|n| n.role == Role::Leader && !faults.crashed[n.id])
            {
                let has_old_term_entries = leader.log.iter().any(|e| e.term < leader.current_term);
                let old_term_count = leader
                    .log
                    .iter()
                    .filter(|e| e.term < leader.current_term)
                    .count();

                // Uncommitted old-term entries (the exact fig8 trigger).
                let uncommitted_old = leader
                    .log
                    .iter()
                    .enumerate()
                    .skip(leader.commit_index)
                    .any(|(_, e)| e.term < leader.current_term);

                coverage::record_state(&[
                    ("old_term_entries", &has_old_term_entries.to_string()),
                    ("old_term_count", &old_term_count.min(9).to_string()),
                    ("uncommitted_old", &uncommitted_old.to_string()),
                ]);
            }

            // Commit index divergence across nodes.
            let max_ci = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
            let min_ci = nodes.iter().map(|n| n.commit_index).min().unwrap_or(0);

            // Log entry disagreement: do any two nodes have different
            // terms at the same log index? This is the precursor to
            // log matching violations.
            let mut has_disagreement = false;
            for i in 0..num_nodes {
                for j in (i + 1)..num_nodes {
                    let shared = nodes[i].log.len().min(nodes[j].log.len());
                    for idx in 0..shared {
                        if nodes[i].log[idx].term != nodes[j].log[idx].term {
                            has_disagreement = true;
                            break;
                        }
                    }
                    if has_disagreement {
                        break;
                    }
                }
                if has_disagreement {
                    break;
                }
            }

            // Leader transition count (bucketed).
            let current_max_term = nodes.iter().map(|n| n.current_term).max().unwrap_or(0);

            coverage::record_state(&[
                ("commit_divergence", &(max_ci - min_ci).min(20).to_string()),
                ("log_disagreement", &has_disagreement.to_string()),
                ("max_term", &(current_max_term % 30).to_string()),
            ]);
        }

        // ── Safety invariants + data integrity ──────────────
        check_safety_invariants(
            &nodes,
            &mut committed_values,
            &mut smr_runtime,
            values_committed,
            active,
            tick,
        );

        if bug == BugMode::SnapshotReplayProbe {
            cc_assert_always_stable!(
                "org.onixresearch.chaoscontrol.raft",
                "snapshot-replay-probe.v1",
                "raft",
                "invariant",
                tick < snapshot_probe_fail_after,
                "snapshot replay probe trips only after restored parent context",
                &json!({"tick": tick, "fail_after": snapshot_probe_fail_after}),
            );
        }

        // ── Liveness checks ─────────────────────────────────
        let leader_node = nodes
            .iter()
            .find(|n| n.role == Role::Leader && !faults.crashed[n.id]);
        cc_assert_sometimes_category!(
            "raft",
            "branch",
            leader_node.is_some(),
            "leader elected",
            &json!({"tick": tick}),
        );
        cc_assert_sometimes_category!(
            "raft",
            "branch",
            values_committed > 0,
            "value committed",
            &json!({"tick": tick, "committed": values_committed}),
        );
        cc_assert_sometimes_category!(
            "raft",
            "branch",
            values_committed >= 3,
            "3+ values committed",
            &json!({"tick": tick, "committed": values_committed}),
        );

        // ── Drain kernel coverage into bitmap ───────────────
        kcov::collect();

        // ── Heartbeat ───────────────────────────────────────
        if tick.is_multiple_of(40) {
            print_status(&nodes, &faults, tick, values_proposed);
        }

        tick += 1;
    }
}

/// Check safety invariants and data integrity.
fn check_safety_invariants(
    nodes: &[Node],
    committed_values: &mut CommittedValues,
    smr_runtime: &mut RaftSmrRuntime,
    _values_committed: usize,
    _active: usize,
    tick: usize,
) {
    let max_commit = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
    let leader_node = nodes.iter().find(|n| n.role == Role::Leader);
    let leader_term = leader_node.map_or(0, |n| n.current_term);
    let leader_detail = details::node(
        leader_node.map_or(usize::MAX, |n| n.id),
        leader_term,
        leader_node.map_or("none", |n| role_str(n.role)),
    );

    // Data integrity: no committed entry overwritten
    let integrity_violations = committed_values.check_and_record(nodes);
    cc_assert_always_category!(
        "raft",
        "invariant",
        integrity_violations.is_empty(),
        "data integrity: committed entry never overwritten",
        &json!({"tick": tick, "violations": integrity_violations.len(),
        "first": integrity_violations.first().map(|(n, idx, old, new)|
            json!({"node": n, "index": idx, "old_value": old, "new_value": new})
        )}),
    );

    let smr_summary = smr_runtime.check(nodes);
    cc_assert_always_category!(
        "raft",
        "invariant",
        smr_summary.pass,
        "SMR chain: committed application histories agree",
        &json!({
            "tick": tick,
            "adapter_errors": smr_summary.adapter_errors,
            "history_error": smr_summary.history_error,
            "violations": smr_summary.violations,
            "observations": smr_summary.observations,
        }),
    );

    // Election safety
    let election_violations = check_election_safety(nodes);
    cc_assert_always_category!(
        "raft",
        "invariant",
        election_violations.is_empty(),
        "election safety: at most one leader per term",
        &leader_detail,
    );

    // Log matching
    let log_violations = check_log_matching(nodes);
    cc_assert_always_category!(
        "raft",
        "invariant",
        log_violations.is_empty(),
        "log matching: divergence before agreement",
        &details::merge(
            &leader_detail,
            &details::log(
                max_commit,
                leader_term,
                nodes.iter().map(|n| n.log.len()).max().unwrap_or(0),
            ),
        ),
    );

    // Leader completeness
    let completeness_violations = check_leader_completeness(nodes);
    cc_assert_always_category!(
        "raft",
        "invariant",
        completeness_violations.is_empty(),
        "leader completeness: committed entry preserved",
        &details::merge(&leader_detail, &json!({"max_commit": max_commit})),
    );
}

/// Print periodic status line.
fn print_status(nodes: &[Node], faults: &FaultState, tick: usize, values_proposed: u64) {
    let leader_id = nodes
        .iter()
        .find(|n| n.role == Role::Leader && !faults.crashed[n.id])
        .map(|n| n.id);
    let crashed_str: String = (0..nodes.len())
        .filter(|&i| faults.crashed[i])
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let terms: Vec<String> = nodes.iter().map(|n| n.current_term.to_string()).collect();
    let commits: Vec<String> = nodes.iter().map(|n| n.commit_index.to_string()).collect();
    println!(
        "raft: tick={} leader={:?} terms=[{}] commits=[{}] proposed={} crashed=[{}] partitions={}",
        tick,
        leader_id,
        terms.join(","),
        commits.join(","),
        values_proposed,
        crashed_str,
        faults.partition_count(),
    );
}
