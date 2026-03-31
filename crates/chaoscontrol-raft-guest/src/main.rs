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
    check_election_safety, check_leader_completeness, check_log_matching, BugMode, LogEntry,
    Message, Node, Role, ELECTION_TIMEOUT_BASE, ELECTION_TIMEOUT_JITTER, HEARTBEAT_INTERVAL,
    NUM_NODES,
};
use chaoscontrol_sdk::assert::details;
use chaoscontrol_sdk::prelude::*;
use chaoscontrol_sdk::{coverage, kcov, lifecycle, random};
use serde_json::json;

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

// ═══════════════════════════════════════════════════════════════════════
//  Fault state
// ═══════════════════════════════════════════════════════════════════════

/// Per-node crash state and per-link partition matrix.
struct FaultState {
    /// Whether each node is currently crashed.
    crashed: [bool; NUM_NODES],
    /// Asymmetric partition matrix: partitioned[a][b] = messages from a→b dropped.
    partitioned: [[bool; NUM_NODES]; NUM_NODES],
    /// Delayed messages: (from, to, message, deliver_at_tick).
    delay_queue: Vec<(usize, usize, Message, usize)>,
}

impl FaultState {
    fn new() -> Self {
        Self {
            crashed: [false; NUM_NODES],
            partitioned: [[false; NUM_NODES]; NUM_NODES],
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

    /// 3-bit bitmap: bit i set if node i is alive.
    fn alive_mask(&self) -> usize {
        let mut mask = 0;
        for i in 0..NUM_NODES {
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
        // Simple check: at least QUORUM nodes alive and no node in the alive set
        // is partitioned from all other alive nodes.
        if self.alive_count() < 2 {
            return false;
        }
        // For 3-node Raft: quorum=2. Check that at least 2 alive nodes can talk.
        let alive: Vec<usize> = (0..NUM_NODES).filter(|&i| !self.crashed[i]).collect();
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
    fn new() -> Self {
        Self {
            values: vec![Vec::new(); NUM_NODES],
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

// ═══════════════════════════════════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    guest_init();

    let bug = parse_bug_mode();
    println!("raft: starting 3-node cluster (bug={})", bug.name());

    lifecycle::setup_complete(&json!({"program": "raft-guest", "nodes": 3, "bug": bug.name()}));
    println!("raft: setup_complete (bug={})", bug.name());

    // Initialize 3 nodes with the selected bug mode
    let mut nodes: Vec<Node> = (0..NUM_NODES).map(|i| Node::new_with_bug(i, bug)).collect();
    // Stagger initial election timers
    for (i, node) in nodes.iter_mut().enumerate() {
        node.election_timer = ELECTION_TIMEOUT_BASE + i * 3;
    }

    let mut faults = FaultState::new();
    let mut committed_values = CommittedValues::new();
    let mut values_proposed = 0u64;
    let mut values_committed = 0usize;
    let mut tick = 0usize;
    let mut ticks_quorum_healthy = 0usize;
    let mut commit_at_healthy_start = 0usize;

    loop {
        // ── Fault injection: crashes and restarts ────────────
        for i in 0..NUM_NODES {
            if faults.crashed[i] {
                // Restart decision: ~2% per tick
                if random::random_choice(50) == 0 {
                    // Restart: keep persistent state (log, current_term, voted_for),
                    // reset volatile state
                    nodes[i].commit_index = 0;
                    nodes[i].role = Role::Follower;
                    nodes[i].election_timer = ELECTION_TIMEOUT_BASE;
                    nodes[i].next_index = [1; NUM_NODES];
                    nodes[i].match_index = [0; NUM_NODES];
                    nodes[i].votes_received = 0;
                    nodes[i].heartbeat_timer = 0;
                    nodes[i].inbox.clear();
                    faults.crashed[i] = false;

                    cc_assert_reachable!(
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

                    cc_assert_reachable!(
                        "node crashed",
                        &json!({"node": i, "tick": tick, "term": nodes[i].current_term}),
                    );
                }
            }
        }

        // ── Fault injection: partitions ─────────────────────
        // Create new partitions
        if random::random_choice(300) == 0 {
            let src = random::random_choice(NUM_NODES);
            let dst = random::random_choice(NUM_NODES);
            if src != dst && !faults.partitioned[src][dst] {
                faults.partitioned[src][dst] = true;
                cc_assert_reachable!(
                    "link partitioned",
                    &json!({"src": src, "dst": dst, "tick": tick}),
                );
            }
        }

        // Heal existing partitions
        for src in 0..NUM_NODES {
            for dst in 0..NUM_NODES {
                if faults.partitioned[src][dst] && random::random_choice(30) == 0 {
                    faults.partitioned[src][dst] = false;
                    cc_assert_reachable!(
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
        let active = random::random_choice(NUM_NODES);
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
                        cc_assert_reachable!(
                            "request_vote handler",
                            &json!({"node": active, "from": from, "term": *term, "candidate_id": *candidate_id}),
                        );
                    }
                    Message::RequestVoteResponse { term, vote_granted } => {
                        cc_assert_reachable!(
                            "request_vote_response handler",
                            &json!({"node": active, "from": from, "term": *term, "vote_granted": *vote_granted}),
                        );
                    }
                    Message::AppendEntries { term, entries, .. } => {
                        cc_assert_reachable!(
                            "append_entries handler",
                            &json!({"node": active, "from": from, "term": *term, "entry_count": entries.len()}),
                        );
                    }
                    Message::AppendEntriesResponse {
                        term,
                        success,
                        match_index,
                    } => {
                        cc_assert_reachable!(
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
                    cc_assert_reachable!(
                        "stepped down to follower",
                        &json!({"node": active, "old_term": old_term, "new_term": node.current_term}),
                    );
                }
                if old_role == Role::Candidate && node.role == Role::Follower && is_ae {
                    cc_assert_reachable!(
                        "candidate stepped down on append_entries",
                        &json!({"node": active, "term": node.current_term}),
                    );
                }
                if old_role == Role::Candidate && node.role == Role::Leader {
                    cc_assert_reachable!(
                        "candidate won election",
                        &json!({"node": active, "term": node.current_term}),
                    );
                }

                // ── Data invariant: commit_index bounded ────
                if node.commit_index != old_commit {
                    cc_assert_always!(
                        node.commit_index <= node.log.len(),
                        "commit_index within log bounds",
                        &json!({"node": active, "commit_index": node.commit_index, "log_len": node.log.len()}),
                    );
                }

                // ── Reply-based sometimes-pairs + invariants ─
                for (to, reply) in &replies {
                    match reply {
                        Message::RequestVoteResponse { vote_granted, .. } => {
                            cc_assert_sometimes!(
                                *vote_granted,
                                "vote granted",
                                &json!({"voter": active, "candidate": *to}),
                            );
                            cc_assert_sometimes!(
                                !*vote_granted,
                                "vote denied",
                                &json!({"voter": active, "candidate": *to}),
                            );
                            if let (true, Some(cid)) = (*vote_granted, rv_candidate) {
                                cc_assert_always!(
                                    node.voted_for == Some(cid),
                                    "voted_for matches granted candidate",
                                    &json!({"node": active, "candidate_id": cid}),
                                );
                            }
                        }
                        Message::AppendEntriesResponse { success, .. } => {
                            cc_assert_sometimes!(
                                *success,
                                "append accepted",
                                &json!({"node": active, "from": from}),
                            );
                            cc_assert_sometimes!(
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
                    cc_assert_always!(
                        node.match_index[from] <= node.log.len(),
                        "match_index within bounds",
                        &json!({"leader": active, "peer": from, "match_index": node.match_index[from], "log_len": node.log.len()}),
                    );
                    cc_assert_always!(
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
                            cc_assert_reachable!(
                                "log conflict: truncated",
                                &json!({"node": active, "prev_log_index": prev_idx}),
                            );
                        }
                        if had_consistent {
                            cc_assert_reachable!(
                                "log entries consistent",
                                &json!({"node": active}),
                            );
                        }
                        if had_new {
                            cc_assert_reachable!(
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
                    cc_assert_sometimes!(
                        timer_expired,
                        "election timeout fired",
                        &json!({"node": active}),
                    );
                    cc_assert_sometimes!(
                        !timer_expired,
                        "election timer decremented",
                        &json!({"node": active}),
                    );

                    if timer_expired {
                        cc_assert_reachable!(
                            "follower started election",
                            &json!({"node": active, "new_term": node.current_term + 1}),
                        );
                        let msgs = node.become_candidate(jitter);
                        coverage::record_edge(1000 + node.id * 100);
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
                        cc_assert_always!(
                            node.match_index[node.id] == node.log.len(),
                            "leader self match_index tracks log",
                            &json!({"node": active, "match_index": node.match_index[node.id], "log_len": node.log.len()}),
                        );
                        let old_commit = node.commit_index;
                        node.try_advance_commit();
                        let commit_advanced = node.commit_index > old_commit;
                        cc_assert_sometimes!(
                            commit_advanced,
                            "leader commit advanced after proposal",
                            &json!({"node": active, "commit": node.commit_index}),
                        );
                        cc_assert_always!(
                            node.commit_index <= node.log.len(),
                            "commit_index within log bounds",
                            &json!({"node": active, "commit_index": node.commit_index, "log_len": node.log.len()}),
                        );
                        coverage::record_edge(7000 + values_proposed as usize);
                    }
                    cc_assert_sometimes!(
                        proposed,
                        "leader proposed value",
                        &json!({"node": active}),
                    );
                    cc_assert_sometimes!(
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
                coverage::record_edge(8000 + from * 10 + to);
                continue;
            }

            // Random drop (5% base rate)
            if random::random_choice(100) >= 95 {
                cc_assert_sometimes!(true, "message dropped", &details::network(from, to, false),);
                coverage::record_edge(8000 + from * 10 + to);
                continue;
            }

            // Message reordering: ~5% chance
            if random::random_choice(20) == 0 {
                let delay = 1 + random::random_choice(5);
                faults
                    .delay_queue
                    .push((from, to, msg.clone(), tick + delay));
                cc_assert_reachable!(
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
                cc_assert_reachable!("message duplicated", &json!({"from": from, "to": to}),);
                continue;
            }

            // Normal delivery
            cc_assert_sometimes!(true, "message delivered", &details::network(from, to, true),);
            nodes[to].inbox.push((from, msg));
        }

        // ── Track committed values ──────────────────────────
        let max_commit = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
        if max_commit > values_committed {
            values_committed = max_commit;
        }

        // ── Fault-aware liveness ────────────────────────────
        if faults.alive_count() == NUM_NODES && faults.quorum_reachable() {
            ticks_quorum_healthy += 1;
            if ticks_quorum_healthy == 1 {
                // Record commit level at start of healthy window
                commit_at_healthy_start = max_commit;
            }
            if ticks_quorum_healthy > 200 {
                cc_assert_sometimes!(
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

        // ── Safety invariants + data integrity ──────────────
        check_safety_invariants(
            &nodes,
            &mut committed_values,
            values_committed,
            active,
            tick,
        );

        // ── Liveness checks ─────────────────────────────────
        let leader_node = nodes
            .iter()
            .find(|n| n.role == Role::Leader && !faults.crashed[n.id]);
        cc_assert_sometimes!(
            leader_node.is_some(),
            "leader elected",
            &json!({"tick": tick}),
        );
        cc_assert_sometimes!(
            values_committed > 0,
            "value committed",
            &json!({"tick": tick, "committed": values_committed}),
        );
        cc_assert_sometimes!(
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
    cc_assert_always!(
        integrity_violations.is_empty(),
        "data integrity: committed entry never overwritten",
        &json!({"tick": tick, "violations": integrity_violations.len(),
        "first": integrity_violations.first().map(|(n, idx, old, new)|
            json!({"node": n, "index": idx, "old_value": old, "new_value": new})
        )}),
    );

    // Election safety
    let election_violations = check_election_safety(nodes);
    cc_assert_always!(
        election_violations.is_empty(),
        "election safety: at most one leader per term",
        &leader_detail,
    );

    // Log matching
    let log_violations = check_log_matching(nodes);
    cc_assert_always!(
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
    cc_assert_always!(
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
    let crashed_str: String = (0..NUM_NODES)
        .filter(|&i| faults.crashed[i])
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "raft: tick={} leader={:?} terms=[{},{},{}] commits=[{},{},{}] proposed={} crashed=[{}] partitions={}",
        tick,
        leader_id,
        nodes[0].current_term,
        nodes[1].current_term,
        nodes[2].current_term,
        nodes[0].commit_index,
        nodes[1].commit_index,
        nodes[2].commit_index,
        values_proposed,
        crashed_str,
        faults.partition_count(),
    );
}
