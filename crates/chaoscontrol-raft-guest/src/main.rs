//! In-process Raft consensus simulation for ChaosControl exploration.
//!
//! Runs 3 Raft nodes in a single process as PID 1 inside the VM.
//! All randomness flows through the ChaosControl SDK so the VMM can
//! systematically explore message orderings, election timing, and
//! fault injection schedules.
//!
//! Safety invariants checked every tick:
//! - **Election Safety**: at most one leader per term
//! - **Log Matching**: if two logs agree at index i, they agree on all j < i
//! - **Leader Completeness**: committed entries are never lost
//!
//! Build & package:
//!   nix develop --command bash -c "scripts/build-raft-guest.sh"

use chaoscontrol_sdk::{assert, coverage, lifecycle, random};

// ═══════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════

const NUM_NODES: usize = 3;
const TICKS: usize = 200;
const QUORUM: usize = 2;
/// Ticks before a follower starts an election (base).
const ELECTION_TIMEOUT_BASE: usize = 8;
/// Random jitter range added to election timeout.
const ELECTION_TIMEOUT_JITTER: usize = 8;
/// How often the leader sends heartbeats (ticks).
const HEARTBEAT_INTERVAL: usize = 3;

// ═══════════════════════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone)]
struct LogEntry {
    term: u64,
    value: u64,
}

#[derive(Debug, Clone)]
enum Message {
    RequestVote {
        term: u64,
        candidate_id: usize,
        last_log_index: usize,
        last_log_term: u64,
    },
    RequestVoteResponse {
        term: u64,
        vote_granted: bool,
    },
    AppendEntries {
        term: u64,
        leader_id: usize,
        prev_log_index: usize,
        prev_log_term: u64,
        entries: Vec<LogEntry>,
        leader_commit: usize,
    },
    AppendEntriesResponse {
        term: u64,
        success: bool,
        match_index: usize,
    },
}

#[derive(Debug)]
struct Node {
    id: usize,
    current_term: u64,
    voted_for: Option<usize>,
    log: Vec<LogEntry>,
    commit_index: usize,
    role: Role,
    election_timer: usize,
    heartbeat_timer: usize,
    votes_received: usize,
    /// Per-peer next_index (leader only).
    next_index: [usize; NUM_NODES],
    /// Per-peer match_index (leader only).
    match_index: [usize; NUM_NODES],
    /// Inbox of pending messages from other nodes.
    inbox: Vec<(usize, Message)>,
}

impl Node {
    fn new(id: usize) -> Self {
        Self {
            id,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            role: Role::Follower,
            election_timer: ELECTION_TIMEOUT_BASE,
            heartbeat_timer: 0,
            votes_received: 0,
            next_index: [1; NUM_NODES],
            match_index: [0; NUM_NODES],
            inbox: Vec::new(),
        }
    }

    fn last_log_index(&self) -> usize {
        self.log.len()
    }

    fn last_log_term(&self) -> u64 {
        self.log.last().map(|e| e.term).unwrap_or(0)
    }

    fn reset_election_timer(&mut self) {
        let jitter = random::random_choice(ELECTION_TIMEOUT_JITTER + 1);
        self.election_timer = ELECTION_TIMEOUT_BASE + jitter;
    }

    fn become_follower(&mut self, term: u64) {
        if term > self.current_term {
            self.current_term = term;
            self.voted_for = None;
        }
        self.role = Role::Follower;
        self.reset_election_timer();
    }

    fn become_candidate(&mut self) -> Vec<(usize, Message)> {
        self.current_term += 1;
        self.role = Role::Candidate;
        self.voted_for = Some(self.id);
        self.votes_received = 1; // Vote for self
        self.reset_election_timer();

        coverage::record_edge(1000 + self.id * 100);

        let mut outbox = Vec::new();
        for peer in 0..NUM_NODES {
            if peer != self.id {
                outbox.push((peer, Message::RequestVote {
                    term: self.current_term,
                    candidate_id: self.id,
                    last_log_index: self.last_log_index(),
                    last_log_term: self.last_log_term(),
                }));
            }
        }
        outbox
    }

    fn become_leader(&mut self) -> Vec<(usize, Message)> {
        self.role = Role::Leader;
        self.heartbeat_timer = 0;
        // Initialize next_index to leader's log length + 1
        for i in 0..NUM_NODES {
            self.next_index[i] = self.last_log_index() + 1;
            self.match_index[i] = 0;
        }
        self.match_index[self.id] = self.last_log_index();

        coverage::record_edge(2000 + self.id * 100);

        // Send initial heartbeats
        self.send_heartbeats()
    }

    fn send_heartbeats(&mut self) -> Vec<(usize, Message)> {
        let mut outbox = Vec::new();
        for peer in 0..NUM_NODES {
            if peer == self.id {
                continue;
            }
            let prev_log_index = self.next_index[peer].saturating_sub(1);
            let prev_log_term = if prev_log_index > 0 && prev_log_index <= self.log.len() {
                self.log[prev_log_index - 1].term
            } else {
                0
            };
            let entries: Vec<LogEntry> = if self.next_index[peer] <= self.log.len() {
                self.log[self.next_index[peer] - 1..].to_vec()
            } else {
                Vec::new()
            };
            outbox.push((peer, Message::AppendEntries {
                term: self.current_term,
                leader_id: self.id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit: self.commit_index,
            }));
        }
        outbox
    }

    /// Process one message, returning outgoing messages.
    fn handle_message(&mut self, from: usize, msg: Message) -> Vec<(usize, Message)> {
        let mut outbox = Vec::new();

        match msg {
            Message::RequestVote { term, candidate_id, last_log_index, last_log_term } => {
                if term > self.current_term {
                    self.become_follower(term);
                }
                let vote_granted = term >= self.current_term
                    && (self.voted_for.is_none() || self.voted_for == Some(candidate_id))
                    && (last_log_term > self.last_log_term()
                        || (last_log_term == self.last_log_term()
                            && last_log_index >= self.last_log_index()));

                if vote_granted {
                    self.voted_for = Some(candidate_id);
                    self.reset_election_timer();
                    coverage::record_edge(3000 + self.id * 10 + candidate_id);
                }

                outbox.push((from, Message::RequestVoteResponse {
                    term: self.current_term,
                    vote_granted,
                }));
            }

            Message::RequestVoteResponse { term, vote_granted } => {
                if term > self.current_term {
                    self.become_follower(term);
                    return outbox;
                }
                if self.role == Role::Candidate && term == self.current_term && vote_granted {
                    self.votes_received += 1;
                    if self.votes_received >= QUORUM {
                        return self.become_leader();
                    }
                }
            }

            Message::AppendEntries { term, prev_log_index, prev_log_term, entries, leader_commit, .. } => {
                if term > self.current_term {
                    self.become_follower(term);
                } else if term < self.current_term {
                    outbox.push((from, Message::AppendEntriesResponse {
                        term: self.current_term,
                        success: false,
                        match_index: 0,
                    }));
                    return outbox;
                }

                if self.role != Role::Follower {
                    self.become_follower(term);
                }
                self.reset_election_timer();

                // Check log consistency
                let log_ok = if prev_log_index == 0 {
                    true
                } else if prev_log_index <= self.log.len() {
                    self.log[prev_log_index - 1].term == prev_log_term
                } else {
                    false
                };

                if !log_ok {
                    outbox.push((from, Message::AppendEntriesResponse {
                        term: self.current_term,
                        success: false,
                        match_index: 0,
                    }));
                    return outbox;
                }

                // Append entries
                let mut insert_index = prev_log_index;
                for entry in &entries {
                    insert_index += 1;
                    if insert_index <= self.log.len() {
                        if self.log[insert_index - 1].term != entry.term {
                            self.log.truncate(insert_index - 1);
                            self.log.push(entry.clone());
                        }
                    } else {
                        self.log.push(entry.clone());
                    }
                }

                // Advance commit index
                if leader_commit > self.commit_index {
                    let new_commit = leader_commit.min(self.log.len());
                    if new_commit > self.commit_index {
                        self.commit_index = new_commit;
                        coverage::record_edge(4000 + self.id * 100 + self.commit_index);
                    }
                }

                outbox.push((from, Message::AppendEntriesResponse {
                    term: self.current_term,
                    success: true,
                    match_index: self.log.len(),
                }));
            }

            Message::AppendEntriesResponse { term, success, match_index } => {
                if term > self.current_term {
                    self.become_follower(term);
                    return outbox;
                }
                if self.role != Role::Leader || term != self.current_term {
                    return outbox;
                }
                if success {
                    self.next_index[from] = match_index + 1;
                    self.match_index[from] = match_index;
                    self.try_advance_commit();
                } else {
                    // Decrement next_index and retry
                    if self.next_index[from] > 1 {
                        self.next_index[from] -= 1;
                    }
                }
            }
        }

        outbox
    }

    /// Leader: advance commit_index if a majority has replicated.
    fn try_advance_commit(&mut self) {
        for n in (self.commit_index + 1)..=self.log.len() {
            if self.log[n - 1].term != self.current_term {
                continue; // Only commit entries from current term
            }
            let mut match_count = 0;
            for peer in 0..NUM_NODES {
                if self.match_index[peer] >= n {
                    match_count += 1;
                }
            }
            if match_count >= QUORUM {
                self.commit_index = n;
                coverage::record_edge(5000 + n);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Safety checks
// ═══════════════════════════════════════════════════════════════════════

fn check_election_safety(nodes: &[Node]) {
    // For each term, at most one leader
    let max_term = nodes.iter().map(|n| n.current_term).max().unwrap_or(0);
    for term in 1..=max_term {
        let leaders: Vec<usize> = nodes
            .iter()
            .filter(|n| n.role == Role::Leader && n.current_term == term)
            .map(|n| n.id)
            .collect();
        assert::always(
            leaders.len() <= 1,
            "election safety: at most one leader per term",
            &[],
        );
    }
}

fn check_log_matching(nodes: &[Node]) {
    // If two nodes have the same (index, term), all prior entries agree
    for i in 0..NUM_NODES {
        for j in (i + 1)..NUM_NODES {
            let min_len = nodes[i].log.len().min(nodes[j].log.len());
            let mut matching = true;
            for k in 0..min_len {
                if nodes[i].log[k].term == nodes[j].log[k].term {
                    // Same term at same index — all prior should match
                    if !matching {
                        assert::always(false, "log matching: divergence before agreement", &[]);
                        break;
                    }
                } else {
                    matching = false;
                }
            }
        }
    }
}

fn check_leader_completeness(nodes: &[Node]) {
    // Committed entries must be present in any leader's log
    let committed: usize = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
    if committed == 0 {
        return;
    }
    // Find the "canonical" committed prefix from the node with highest commit
    let best = nodes.iter().max_by_key(|n| n.commit_index).unwrap();
    for node in nodes {
        if node.role == Role::Leader {
            for idx in 0..committed.min(best.log.len()).min(node.log.len()) {
                assert::always(
                    node.log[idx].term == best.log[idx].term
                        && node.log[idx].value == best.log[idx].value,
                    "leader completeness: committed entry preserved",
                    &[],
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Init
// ═══════════════════════════════════════════════════════════════════════

fn mount_devtmpfs() {
    unsafe {
        libc::mkdir(b"/dev\0".as_ptr() as *const _, 0o755);
        let ret = libc::mount(
            b"devtmpfs\0".as_ptr() as *const _,
            b"/dev\0".as_ptr() as *const _,
            b"devtmpfs\0".as_ptr() as *const _,
            0,
            std::ptr::null(),
        );
        if ret != 0 {
            let err = *libc::__errno_location();
            if err != libc::EBUSY {
                eprintln!("raft: mount devtmpfs failed (errno={})", err);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    mount_devtmpfs();
    println!("raft: starting 3-node cluster");

    coverage::init();
    lifecycle::setup_complete(&[("program", "raft-guest"), ("nodes", "3")]);
    println!("raft: setup_complete");

    // Initialize 3 nodes
    let mut nodes: Vec<Node> = (0..NUM_NODES).map(Node::new).collect();
    // Stagger initial election timers
    for (i, node) in nodes.iter_mut().enumerate() {
        node.election_timer = ELECTION_TIMEOUT_BASE + i * 3;
    }

    let mut values_proposed = 0u64;
    let mut values_committed = 0usize;

    for tick in 0..TICKS {
        // ── Pick which node to activate this tick ────────────
        let active = random::random_choice(NUM_NODES);
        coverage::record_edge(6000 + tick * 7 + active * 3);

        // ── Tick timers on active node ───────────────────────
        let mut outbox: Vec<(usize, usize, Message)> = Vec::new(); // (from, to, msg)

        {
            let node = &mut nodes[active];

            // Drain inbox
            let inbox: Vec<(usize, Message)> = node.inbox.drain(..).collect();
            for (from, msg) in inbox {
                let replies = node.handle_message(from, msg);
                for (to, reply) in replies {
                    outbox.push((active, to, reply));
                }
            }

            // Timer logic
            match node.role {
                Role::Follower | Role::Candidate => {
                    if node.election_timer == 0 {
                        let msgs = node.become_candidate();
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
                    if random::random_choice(4) == 0 {
                        values_proposed += 1;
                        let entry = LogEntry {
                            term: node.current_term,
                            value: values_proposed,
                        };
                        node.log.push(entry);
                        node.match_index[node.id] = node.log.len();
                        node.try_advance_commit();
                        coverage::record_edge(7000 + values_proposed as usize);
                    }
                }
            }
        }

        // ── Maybe drop a message (simulated network fault) ──
        // SDK randomness controls whether messages arrive
        for (from, to, msg) in outbox {
            let deliver = random::random_choice(100) < 95; // 5% drop rate
            if deliver {
                nodes[to].inbox.push((from, msg));
            } else {
                coverage::record_edge(8000 + from * 10 + to);
            }
        }

        // ── Track committed values ──────────────────────────
        let max_commit = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
        if max_commit > values_committed {
            values_committed = max_commit;
        }

        // ── Safety invariants ───────────────────────────────
        check_election_safety(&nodes);
        check_log_matching(&nodes);
        check_leader_completeness(&nodes);

        // ── Liveness checks ─────────────────────────────────
        let has_leader = nodes.iter().any(|n| n.role == Role::Leader);
        assert::sometimes(has_leader, "leader elected", &[]);
        assert::sometimes(values_committed > 0, "value committed", &[]);
        assert::sometimes(values_committed >= 3, "3+ values committed", &[]);

        // ── Heartbeat ───────────────────────────────────────
        if tick % 40 == 0 {
            let leader_id = nodes.iter().find(|n| n.role == Role::Leader).map(|n| n.id);
            println!(
                "raft: tick={} leader={:?} terms=[{},{},{}] commits=[{},{},{}] proposed={}",
                tick,
                leader_id,
                nodes[0].current_term, nodes[1].current_term, nodes[2].current_term,
                nodes[0].commit_index, nodes[1].commit_index, nodes[2].commit_index,
                values_proposed,
            );
        }
    }

    // ── Final report ────────────────────────────────────────
    lifecycle::send_event("raft_done", &[
        ("proposed", &values_proposed.to_string()),
        ("committed", &values_committed.to_string()),
    ]);

    println!(
        "raft: workload complete proposed={} committed={}",
        values_proposed, values_committed,
    );
    for (i, node) in nodes.iter().enumerate() {
        println!(
            "raft: node[{}] term={} role={:?} log_len={} commit={}",
            i, node.current_term, node.role, node.log.len(), node.commit_index,
        );
    }

    // Idle forever (PID 1 can't exit)
    loop {
        unsafe { libc::pause(); }
    }
}
