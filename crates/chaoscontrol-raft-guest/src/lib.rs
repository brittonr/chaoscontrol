//! Pure Raft consensus implementation for ChaosControl exploration.
//!
//! This module contains the core Raft protocol logic with **zero SDK dependencies**.
//! All types, state transitions, message handling, and safety checks are pure Rust
//! functions that can be unit-tested without a VM.
//!
//! The `main.rs` binary wires this to the ChaosControl SDK for in-VM execution.

// ═══════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════

pub const NUM_NODES: usize = 3;
pub const QUORUM: usize = 2;
/// Ticks before a follower starts an election (base).
pub const ELECTION_TIMEOUT_BASE: usize = 8;
/// Random jitter range added to election timeout.
pub const ELECTION_TIMEOUT_JITTER: usize = 8;
/// How often the leader sends heartbeats (ticks).
pub const HEARTBEAT_INTERVAL: usize = 3;

// ═══════════════════════════════════════════════════════════════════════
//  Bug injection
// ═══════════════════════════════════════════════════════════════════════

/// Injected bug variants for validation testing.
///
/// Each variant introduces a realistic, subtle implementation error that
/// violates one or more Raft safety properties. The exploration engine
/// should find each bug by discovering the right fault schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BugMode {
    /// Correct implementation — no bugs.
    None,
    /// **Figure 8 commit bug** (Raft §5.4.2).
    ///
    /// Removes the guard that prevents committing entries from prior terms.
    /// A new leader can commit old-term entries that may later be overwritten
    /// by a different leader, violating leader completeness.
    Fig8Commit,
    /// **Missing log truncation**.
    ///
    /// When a follower receives AppendEntries with a conflicting entry,
    /// it skips the truncation, leaving stale entries that diverge from
    /// the leader's log.
    SkipTruncate,
    /// **Accept stale-term AppendEntries**.
    ///
    /// Follower accepts AppendEntries from leaders with lower terms instead
    /// of rejecting them. A partitioned old leader can write conflicting
    /// entries.
    AcceptStaleTerm,
    /// **Leader ignores higher-term AppendEntries**.
    ///
    /// Leader does not step down when receiving AppendEntries or
    /// AppendEntriesResponse with a higher term, causing concurrent leaders.
    LeaderNoStepdown,
    /// **Double voting in same term**.
    ///
    /// Node grants votes to multiple candidates in the same term by
    /// ignoring the `voted_for` check, enabling two leaders per term.
    DoubleVote,
    /// **Premature commit advance**.
    ///
    /// Advances commit_index BEFORE verifying log consistency, committing
    /// entries the follower hasn't actually validated.
    PrematureCommit,
}

impl BugMode {
    /// Parse from a string (for kernel cmdline parsing).
    pub fn parse(s: &str) -> Self {
        match s {
            "fig8" | "fig8_commit" => Self::Fig8Commit,
            "skip_truncate" | "no_truncate" => Self::SkipTruncate,
            "accept_stale" | "accept_stale_term" => Self::AcceptStaleTerm,
            "leader_no_stepdown" | "no_stepdown" => Self::LeaderNoStepdown,
            "double_vote" => Self::DoubleVote,
            "premature_commit" => Self::PrematureCommit,
            "none" | "" => Self::None,
            _ => Self::None,
        }
    }

    /// Short name for display/logging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fig8Commit => "fig8_commit",
            Self::SkipTruncate => "skip_truncate",
            Self::AcceptStaleTerm => "accept_stale_term",
            Self::LeaderNoStepdown => "leader_no_stepdown",
            Self::DoubleVote => "double_vote",
            Self::PrematureCommit => "premature_commit",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub term: u64,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
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
pub struct Node {
    pub id: usize,
    pub current_term: u64,
    pub voted_for: Option<usize>,
    pub log: Vec<LogEntry>,
    pub commit_index: usize,
    pub role: Role,
    pub election_timer: usize,
    pub heartbeat_timer: usize,
    pub votes_received: usize,
    /// Per-peer next_index (leader only).
    pub next_index: [usize; NUM_NODES],
    /// Per-peer match_index (leader only).
    pub match_index: [usize; NUM_NODES],
    /// Inbox of pending messages from other nodes.
    pub inbox: Vec<(usize, Message)>,
    /// Injected bug mode for validation testing.
    pub bug: BugMode,
}

impl Node {
    pub fn new(id: usize) -> Self {
        Self::new_with_bug(id, BugMode::None)
    }

    pub fn new_with_bug(id: usize, bug: BugMode) -> Self {
        debug_assert!(id < NUM_NODES, "node id must be < NUM_NODES");
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
            bug,
        }
    }

    pub fn last_log_index(&self) -> usize {
        self.log.len()
    }

    pub fn last_log_term(&self) -> u64 {
        self.log.last().map(|e| e.term).unwrap_or(0)
    }

    /// Reset the election timer with the given jitter value.
    /// In production, `jitter` comes from `random::random_choice(ELECTION_TIMEOUT_JITTER + 1)`.
    pub fn reset_election_timer_with(&mut self, jitter: usize) {
        self.election_timer = ELECTION_TIMEOUT_BASE + jitter;
    }

    /// Transition to follower state for the given term.
    /// `jitter` is used to reset the election timer (from external RNG).
    pub fn become_follower(&mut self, term: u64, jitter: usize) {
        if term > self.current_term {
            self.current_term = term;
            self.voted_for = None;
        }
        self.role = Role::Follower;
        self.reset_election_timer_with(jitter);
    }

    /// Start an election. Returns outgoing vote requests.
    /// `jitter` is used to reset the election timer (from external RNG).
    pub fn become_candidate(&mut self, jitter: usize) -> Vec<(usize, Message)> {
        self.current_term += 1;
        self.role = Role::Candidate;
        self.voted_for = Some(self.id);
        self.votes_received = 1; // Vote for self
        self.reset_election_timer_with(jitter);

        let mut outbox = Vec::new();
        for peer in 0..NUM_NODES {
            if peer != self.id {
                outbox.push((
                    peer,
                    Message::RequestVote {
                        term: self.current_term,
                        candidate_id: self.id,
                        last_log_index: self.last_log_index(),
                        last_log_term: self.last_log_term(),
                    },
                ));
            }
        }
        outbox
    }

    /// Transition to leader and send initial heartbeats.
    pub fn become_leader(&mut self) -> Vec<(usize, Message)> {
        self.role = Role::Leader;
        self.heartbeat_timer = 0;
        // Initialize next_index to leader's log length + 1
        for i in 0..NUM_NODES {
            self.next_index[i] = self.last_log_index() + 1;
            self.match_index[i] = 0;
        }
        self.match_index[self.id] = self.last_log_index();

        // Send initial heartbeats
        self.send_heartbeats()
    }

    /// Send AppendEntries heartbeats/replication to all peers.
    pub fn send_heartbeats(&mut self) -> Vec<(usize, Message)> {
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
            outbox.push((
                peer,
                Message::AppendEntries {
                    term: self.current_term,
                    leader_id: self.id,
                    prev_log_index,
                    prev_log_term,
                    entries,
                    leader_commit: self.commit_index,
                },
            ));
        }
        outbox
    }

    /// Process one message, returning outgoing messages.
    /// `jitter` is used for election timer resets (from external RNG).
    pub fn handle_message(
        &mut self,
        from: usize,
        msg: Message,
        jitter: usize,
    ) -> Vec<(usize, Message)> {
        let mut outbox = Vec::new();

        match msg {
            Message::RequestVote {
                term,
                candidate_id,
                last_log_index,
                last_log_term,
            } => {
                if term > self.current_term {
                    self.become_follower(term, jitter);
                }
                let vote_granted = term >= self.current_term
                    && (self.bug == BugMode::DoubleVote
                        || self.voted_for.is_none()
                        || self.voted_for == Some(candidate_id))
                    && (last_log_term > self.last_log_term()
                        || (last_log_term == self.last_log_term()
                            && last_log_index >= self.last_log_index()));

                if vote_granted {
                    self.voted_for = Some(candidate_id);
                    self.reset_election_timer_with(jitter);
                }

                outbox.push((
                    from,
                    Message::RequestVoteResponse {
                        term: self.current_term,
                        vote_granted,
                    },
                ));
            }

            Message::RequestVoteResponse { term, vote_granted } => {
                if term > self.current_term {
                    self.become_follower(term, jitter);
                    return outbox;
                }
                if self.role == Role::Candidate && term == self.current_term && vote_granted {
                    self.votes_received += 1;
                    if self.votes_received >= QUORUM {
                        return self.become_leader();
                    }
                }
            }

            Message::AppendEntries {
                term,
                leader_id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit,
            } => {
                let _ = leader_id; // Raft protocol field; used for redirect in full impl

                // BUG(LeaderNoStepdown): leader ignores higher-term AE entirely
                if self.bug == BugMode::LeaderNoStepdown
                    && self.role == Role::Leader
                    && term > self.current_term
                {
                    return outbox;
                }

                if term > self.current_term {
                    self.become_follower(term, jitter);
                // BUG(AcceptStaleTerm): accept AE from lower terms
                } else if self.bug != BugMode::AcceptStaleTerm && term < self.current_term {
                    outbox.push((
                        from,
                        Message::AppendEntriesResponse {
                            term: self.current_term,
                            success: false,
                            match_index: 0,
                        },
                    ));
                    return outbox;
                }

                if self.role != Role::Follower {
                    self.become_follower(term, jitter);
                }
                self.reset_election_timer_with(jitter);

                // BUG(PrematureCommit): advance commit BEFORE log check
                if self.bug == BugMode::PrematureCommit && leader_commit > self.commit_index {
                    let new_commit = leader_commit.min(self.log.len());
                    if new_commit > self.commit_index {
                        self.commit_index = new_commit;
                    }
                }

                // Check log consistency
                let log_ok = if prev_log_index == 0 {
                    true
                } else if prev_log_index <= self.log.len() {
                    self.log[prev_log_index - 1].term == prev_log_term
                } else {
                    false
                };

                if !log_ok {
                    outbox.push((
                        from,
                        Message::AppendEntriesResponse {
                            term: self.current_term,
                            success: false,
                            match_index: 0,
                        },
                    ));
                    return outbox;
                }

                // Append entries
                let mut insert_index = prev_log_index;
                for entry in &entries {
                    insert_index += 1;
                    if insert_index <= self.log.len() {
                        if self.log[insert_index - 1].term != entry.term {
                            // BUG(SkipTruncate): don't truncate conflicting entries
                            if self.bug != BugMode::SkipTruncate {
                                self.log.truncate(insert_index - 1);
                            }
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
                    }
                }

                // Report match up to the verified point only.  The follower may
                // have additional entries beyond insert_index from a previous
                // leader — those have NOT been verified against the current
                // leader's log and must not be counted.
                outbox.push((
                    from,
                    Message::AppendEntriesResponse {
                        term: self.current_term,
                        success: true,
                        match_index: insert_index,
                    },
                ));
            }

            Message::AppendEntriesResponse {
                term,
                success,
                match_index,
            } => {
                // BUG(LeaderNoStepdown): leader ignores higher-term response
                if self.bug == BugMode::LeaderNoStepdown
                    && self.role == Role::Leader
                    && term > self.current_term
                {
                    // Leader stays as-is, doesn't step down
                } else if term > self.current_term {
                    self.become_follower(term, jitter);
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
    pub fn try_advance_commit(&mut self) {
        for n in (self.commit_index + 1)..=self.log.len() {
            if self.bug != BugMode::Fig8Commit && self.log[n - 1].term != self.current_term {
                continue; // Only commit entries from current term (§5.4.2)
            }
            let mut match_count = 0;
            for peer in 0..NUM_NODES {
                if self.match_index[peer] >= n {
                    match_count += 1;
                }
            }
            if match_count >= QUORUM {
                self.commit_index = n;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Safety checks (pure — return violations instead of calling SDK)
// ═══════════════════════════════════════════════════════════════════════

/// Check election safety: at most one leader per term.
/// Returns a list of (term, leader_ids) violations.
pub fn check_election_safety(nodes: &[Node]) -> Vec<(u64, Vec<usize>)> {
    let mut violations = Vec::new();
    let max_term = nodes.iter().map(|n| n.current_term).max().unwrap_or(0);
    for term in 1..=max_term {
        let leaders: Vec<usize> = nodes
            .iter()
            .filter(|n| n.role == Role::Leader && n.current_term == term)
            .map(|n| n.id)
            .collect();
        if leaders.len() > 1 {
            violations.push((term, leaders));
        }
    }
    violations
}

/// Check log matching: if two logs agree at index i (same term), all prior entries agree.
/// Returns a list of (node_i, node_j, divergence_index) violations.
pub fn check_log_matching(nodes: &[Node]) -> Vec<(usize, usize, usize)> {
    let mut violations = Vec::new();
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let min_len = nodes[i].log.len().min(nodes[j].log.len());
            let mut matching = true;
            for k in 0..min_len {
                if nodes[i].log[k].term == nodes[j].log[k].term {
                    if !matching {
                        violations.push((i, j, k));
                        break;
                    }
                } else {
                    matching = false;
                }
            }
        }
    }
    violations
}

/// Check leader completeness: committed entries must be in any current-term leader's log.
///
/// Raft §5.4.3: "If a log entry is committed in a given term, then that entry
/// will be present in the logs of the leaders for all higher-numbered terms."
///
/// Only checks leaders whose term equals the highest observed term.  Stale leaders
/// (lower term) are transient — they haven't learned about the new election yet and
/// will step down once they receive a higher-term message.  Their uncommitted entries
/// may legitimately conflict with entries committed by a newer-term leader.
///
/// Returns a list of (leader_id, missing_index) violations.
pub fn check_leader_completeness(nodes: &[Node]) -> Vec<(usize, usize)> {
    let mut violations = Vec::new();
    let committed: usize = nodes.iter().map(|n| n.commit_index).max().unwrap_or(0);
    if committed == 0 {
        return violations;
    }
    let max_term = nodes.iter().map(|n| n.current_term).max().unwrap_or(0);
    let best = nodes.iter().max_by_key(|n| n.commit_index).unwrap();
    for node in nodes {
        // Only check leaders in the current term — stale leaders are zombies
        if node.role == Role::Leader && node.current_term >= max_term {
            for idx in 0..committed.min(best.log.len()).min(node.log.len()) {
                if node.log[idx].term != best.log[idx].term
                    || node.log[idx].value != best.log[idx].value
                {
                    violations.push((node.id, idx));
                }
            }
            // Leader log too short to cover committed entries
            if node.log.len() < committed.min(best.log.len()) {
                violations.push((node.id, node.log.len()));
            }
        }
    }
    violations
}

// ═══════════════════════════════════════════════════════════════════════
//  Cluster helper — drives the simulation without SDK
// ═══════════════════════════════════════════════════════════════════════

/// A deterministic cluster runner for testing. Uses a simple LCG for randomness
/// instead of the SDK, allowing full Raft simulation in unit tests.
pub struct TestCluster {
    pub nodes: Vec<Node>,
    pub tick: usize,
    pub values_proposed: u64,
    rng_state: u64,
    pub bug: BugMode,
}

impl TestCluster {
    /// Create a new test cluster with staggered election timers.
    pub fn new(seed: u64) -> Self {
        Self::new_with_bug(seed, BugMode::None)
    }

    /// Create a test cluster with the given bug mode injected into all nodes.
    pub fn new_with_bug(seed: u64, bug: BugMode) -> Self {
        let mut nodes: Vec<Node> = (0..NUM_NODES).map(|i| Node::new_with_bug(i, bug)).collect();
        for (i, node) in nodes.iter_mut().enumerate() {
            node.election_timer = ELECTION_TIMEOUT_BASE + i * 3;
        }
        Self {
            nodes,
            tick: 0,
            values_proposed: 0,
            rng_state: seed,
            bug,
        }
    }

    /// Simple LCG for deterministic test randomness.
    fn rand(&mut self, bound: usize) -> usize {
        // LCG parameters from Numerical Recipes
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        ((self.rng_state >> 33) as usize) % bound
    }

    /// Run one tick of the cluster simulation. Returns the active node index.
    pub fn step(&mut self) -> usize {
        let active = self.rand(NUM_NODES);
        let jitter = self.rand(ELECTION_TIMEOUT_JITTER + 1);

        let mut outbox: Vec<(usize, usize, Message)> = Vec::new();

        {
            let node = &mut self.nodes[active];

            // Drain inbox
            let inbox: Vec<(usize, Message)> = node.inbox.drain(..).collect();
            for (from, msg) in inbox {
                let replies = node.handle_message(from, msg, jitter);
                for (to, reply) in replies {
                    outbox.push((active, to, reply));
                }
            }

            // Timer logic
            match node.role {
                Role::Follower | Role::Candidate => {
                    if node.election_timer == 0 {
                        let msgs = node.become_candidate(jitter);
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
                }
            }
        }

        // Leader proposes a value sometimes (outside node borrow)
        if self.nodes[active].role == Role::Leader && self.rand(4) == 0 {
            self.values_proposed += 1;
            let term = self.nodes[active].current_term;
            let id = self.nodes[active].id;
            let entry = LogEntry {
                term,
                value: self.values_proposed,
            };
            self.nodes[active].log.push(entry);
            self.nodes[active].match_index[id] = self.nodes[active].log.len();
            self.nodes[active].try_advance_commit();
        }

        // Message delivery with 5% drop rate
        let drop_threshold = 95;
        for (from, to, msg) in outbox {
            let deliver = self.rand(100) < drop_threshold;
            if deliver {
                self.nodes[to].inbox.push((from, msg));
            }
        }

        self.tick += 1;
        active
    }

    /// Run the cluster for N ticks, checking safety invariants each tick.
    /// Panics on any safety violation.
    pub fn run_checked(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.step();

            let election_violations = check_election_safety(&self.nodes);
            assert!(
                election_violations.is_empty(),
                "election safety violated: {:?}",
                election_violations
            );

            let log_violations = check_log_matching(&self.nodes);
            assert!(
                log_violations.is_empty(),
                "log matching violated: {:?}",
                log_violations
            );

            let completeness_violations = check_leader_completeness(&self.nodes);
            assert!(
                completeness_violations.is_empty(),
                "leader completeness violated: {:?}",
                completeness_violations
            );
        }
    }

    /// Check if any node is currently a leader.
    pub fn has_leader(&self) -> bool {
        self.nodes.iter().any(|n| n.role == Role::Leader)
    }

    /// Get the current leader(s), if any.
    pub fn leaders(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .filter(|n| n.role == Role::Leader)
            .map(|n| n.id)
            .collect()
    }

    /// Get the maximum commit index across all nodes.
    pub fn max_commit(&self) -> usize {
        self.nodes.iter().map(|n| n.commit_index).max().unwrap_or(0)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Category A: Node construction & basic accessors ─────────

    #[test]
    fn new_node_starts_as_follower() {
        let node = Node::new(0);
        assert_eq!(node.role, Role::Follower);
        assert_eq!(node.current_term, 0);
        assert!(node.voted_for.is_none());
        assert!(node.log.is_empty());
        assert_eq!(node.commit_index, 0);
        assert_eq!(node.votes_received, 0);
    }

    #[test]
    fn new_node_initializes_peer_indices() {
        let node = Node::new(1);
        assert_eq!(node.next_index, [1; NUM_NODES]);
        assert_eq!(node.match_index, [0; NUM_NODES]);
        assert!(node.inbox.is_empty());
    }

    #[test]
    fn last_log_index_empty() {
        let node = Node::new(0);
        assert_eq!(node.last_log_index(), 0);
        assert_eq!(node.last_log_term(), 0);
    }

    #[test]
    fn last_log_index_with_entries() {
        let mut node = Node::new(0);
        node.log.push(LogEntry { term: 1, value: 10 });
        node.log.push(LogEntry { term: 3, value: 20 });
        assert_eq!(node.last_log_index(), 2);
        assert_eq!(node.last_log_term(), 3);
    }

    #[test]
    fn election_timer_reset_with_jitter() {
        let mut node = Node::new(0);
        node.reset_election_timer_with(0);
        assert_eq!(node.election_timer, ELECTION_TIMEOUT_BASE);

        node.reset_election_timer_with(5);
        assert_eq!(node.election_timer, ELECTION_TIMEOUT_BASE + 5);

        node.reset_election_timer_with(ELECTION_TIMEOUT_JITTER);
        assert_eq!(
            node.election_timer,
            ELECTION_TIMEOUT_BASE + ELECTION_TIMEOUT_JITTER
        );
    }

    // ─── Category B: Follower/Candidate transitions ──────────────

    #[test]
    fn become_follower_updates_term_and_clears_vote() {
        let mut node = Node::new(0);
        node.current_term = 3;
        node.voted_for = Some(1);
        node.role = Role::Leader;

        node.become_follower(5, 0);

        assert_eq!(node.current_term, 5);
        assert_eq!(node.role, Role::Follower);
        assert!(node.voted_for.is_none());
    }

    #[test]
    fn become_follower_same_term_keeps_vote() {
        let mut node = Node::new(0);
        node.current_term = 5;
        node.voted_for = Some(2);
        node.role = Role::Candidate;

        // Same term — voted_for should NOT be cleared
        node.become_follower(5, 0);

        assert_eq!(node.current_term, 5);
        assert_eq!(node.role, Role::Follower);
        assert_eq!(node.voted_for, Some(2));
    }

    #[test]
    fn become_candidate_increments_term_and_votes_self() {
        let mut node = Node::new(1);
        node.current_term = 3;

        let msgs = node.become_candidate(0);

        assert_eq!(node.current_term, 4);
        assert_eq!(node.role, Role::Candidate);
        assert_eq!(node.voted_for, Some(1)); // voted for self
        assert_eq!(node.votes_received, 1);
        // Should send RequestVote to 2 peers (0 and 2)
        assert_eq!(msgs.len(), 2);
        for (peer, msg) in &msgs {
            assert_ne!(*peer, 1); // never to self
            match msg {
                Message::RequestVote {
                    term, candidate_id, ..
                } => {
                    assert_eq!(*term, 4);
                    assert_eq!(*candidate_id, 1);
                }
                _ => panic!("expected RequestVote"),
            }
        }
    }

    #[test]
    fn become_leader_initializes_indices_and_sends_heartbeats() {
        let mut node = Node::new(0);
        node.current_term = 2;
        node.role = Role::Candidate;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 2, value: 2 });

        let msgs = node.become_leader();

        assert_eq!(node.role, Role::Leader);
        assert_eq!(node.heartbeat_timer, 0);
        // next_index should be log.len() + 1 = 3 for all
        assert_eq!(node.next_index, [3, 3, 3]);
        // match_index: self = log.len(), others = 0
        assert_eq!(node.match_index[0], 2);
        assert_eq!(node.match_index[1], 0);
        assert_eq!(node.match_index[2], 0);
        // Should send heartbeats to 2 peers
        assert_eq!(msgs.len(), 2);
        for (peer, msg) in &msgs {
            assert_ne!(*peer, 0);
            assert!(matches!(msg, Message::AppendEntries { term: 2, .. }));
        }
    }

    // ─── Category C: RequestVote handling ────────────────────────

    #[test]
    fn vote_granted_when_not_voted_and_log_current() {
        let mut node = Node::new(1);
        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );

        assert_eq!(msgs.len(), 1);
        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, term } => {
                assert!(*vote_granted);
                assert_eq!(*term, 1);
            }
            _ => panic!("expected RequestVoteResponse"),
        }
        assert_eq!(node.voted_for, Some(0));
    }

    #[test]
    fn vote_denied_already_voted_for_other() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.voted_for = Some(2);

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, .. } => {
                assert!(!vote_granted);
            }
            _ => panic!("expected RequestVoteResponse"),
        }
        assert_eq!(node.voted_for, Some(2)); // unchanged
    }

    #[test]
    fn vote_granted_to_same_candidate_again() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.voted_for = Some(0); // already voted for candidate 0

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, .. } => {
                assert!(*vote_granted, "re-voting for same candidate should succeed");
            }
            _ => panic!("expected RequestVoteResponse"),
        }
    }

    #[test]
    fn vote_denied_candidate_log_stale_term() {
        let mut node = Node::new(1);
        node.current_term = 2;
        node.log.push(LogEntry { term: 2, value: 1 });

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 2,
                candidate_id: 0,
                last_log_index: 1,
                last_log_term: 1, // candidate's last term < our last term
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, .. } => {
                assert!(!vote_granted, "stale candidate log should be rejected");
            }
            _ => panic!("expected RequestVoteResponse"),
        }
    }

    #[test]
    fn vote_denied_candidate_log_shorter_same_term() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 });

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 1, // candidate has 1 entry
                last_log_term: 1,  // same term
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, .. } => {
                assert!(!vote_granted, "shorter candidate log should be rejected");
            }
            _ => panic!("expected RequestVoteResponse"),
        }
    }

    #[test]
    fn vote_granted_candidate_log_longer_same_term() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.log.push(LogEntry { term: 1, value: 1 });

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 3, // candidate has more entries
                last_log_term: 1,
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, .. } => {
                assert!(*vote_granted, "longer candidate log should be accepted");
            }
            _ => panic!("expected RequestVoteResponse"),
        }
    }

    #[test]
    fn vote_granted_candidate_higher_last_term() {
        let mut node = Node::new(1);
        node.current_term = 2;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 });

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 2,
                candidate_id: 0,
                last_log_index: 1, // fewer entries
                last_log_term: 2,  // but higher term — more up to date
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse { vote_granted, .. } => {
                assert!(
                    *vote_granted,
                    "higher last_log_term wins even with fewer entries"
                );
            }
            _ => panic!("expected RequestVoteResponse"),
        }
    }

    #[test]
    fn node_steps_down_on_higher_term_vote_request() {
        let mut node = Node::new(1);
        node.current_term = 3;
        node.role = Role::Leader;
        node.voted_for = Some(1);

        node.handle_message(
            0,
            Message::RequestVote {
                term: 5,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );

        assert_eq!(node.current_term, 5);
        assert_eq!(node.role, Role::Follower);
    }

    #[test]
    fn vote_rejected_for_stale_term() {
        let mut node = Node::new(1);
        node.current_term = 5;

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 3,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::RequestVoteResponse {
                vote_granted, term, ..
            } => {
                assert!(!vote_granted);
                assert_eq!(*term, 5);
            }
            _ => panic!("expected RequestVoteResponse"),
        }
    }

    // ─── Category D: RequestVoteResponse handling ────────────────

    #[test]
    fn candidate_becomes_leader_on_quorum() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Candidate;
        node.voted_for = Some(0);
        node.votes_received = 1;

        // One more vote → quorum (2/3)
        let msgs = node.handle_message(
            1,
            Message::RequestVoteResponse {
                term: 1,
                vote_granted: true,
            },
            0,
        );

        assert_eq!(node.role, Role::Leader);
        assert_eq!(node.votes_received, 2);
        // Should have sent heartbeats
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn candidate_stays_candidate_without_quorum() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Candidate;
        node.voted_for = Some(0);
        node.votes_received = 1;

        // Rejected vote
        let msgs = node.handle_message(
            1,
            Message::RequestVoteResponse {
                term: 1,
                vote_granted: false,
            },
            0,
        );

        assert_eq!(node.role, Role::Candidate);
        assert_eq!(node.votes_received, 1); // unchanged
        assert!(msgs.is_empty());
    }

    #[test]
    fn candidate_steps_down_on_higher_term_vote_response() {
        let mut node = Node::new(0);
        node.current_term = 2;
        node.role = Role::Candidate;

        node.handle_message(
            1,
            Message::RequestVoteResponse {
                term: 5,
                vote_granted: false,
            },
            0,
        );

        assert_eq!(node.current_term, 5);
        assert_eq!(node.role, Role::Follower);
    }

    #[test]
    fn stale_vote_response_ignored() {
        let mut node = Node::new(0);
        node.current_term = 3;
        node.role = Role::Candidate;
        node.votes_received = 1;

        // Response from old term
        node.handle_message(
            1,
            Message::RequestVoteResponse {
                term: 2,
                vote_granted: true,
            },
            0,
        );

        assert_eq!(node.votes_received, 1); // unchanged
        assert_eq!(node.role, Role::Candidate);
    }

    #[test]
    fn vote_response_ignored_if_not_candidate() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Follower; // not a candidate

        node.handle_message(
            1,
            Message::RequestVoteResponse {
                term: 1,
                vote_granted: true,
            },
            0,
        );

        assert_eq!(node.role, Role::Follower); // no transition
    }

    // ─── Category E: AppendEntries handling ──────────────────────

    #[test]
    fn append_entries_accepted_empty_log() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;

        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![LogEntry { term: 1, value: 42 }],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.log.len(), 1);
        assert_eq!(node.log[0].value, 42);
        match &msgs[0].1 {
            Message::AppendEntriesResponse {
                success,
                match_index,
                ..
            } => {
                assert!(*success);
                assert_eq!(*match_index, 1);
            }
            _ => panic!("expected AppendEntriesResponse"),
        }
    }

    #[test]
    fn append_entries_rejected_stale_term() {
        let mut node = Node::new(1);
        node.current_term = 5;

        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 3,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::AppendEntriesResponse { success, term, .. } => {
                assert!(!success);
                assert_eq!(*term, 5);
            }
            _ => panic!("expected AppendEntriesResponse"),
        }
        assert_eq!(node.current_term, 5); // unchanged
    }

    #[test]
    fn append_entries_rejected_log_too_short() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;

        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 5, // node has empty log
                prev_log_term: 1,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::AppendEntriesResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("expected AppendEntriesResponse"),
        }
    }

    #[test]
    fn append_entries_rejected_term_mismatch() {
        let mut node = Node::new(1);
        node.current_term = 2;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 }); // index 1 has term 1

        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 2,
                leader_id: 0,
                prev_log_index: 1,
                prev_log_term: 2, // leader thinks index 1 has term 2 — mismatch
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::AppendEntriesResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("expected AppendEntriesResponse"),
        }
    }

    #[test]
    fn conflicting_entry_truncates_log() {
        let mut node = Node::new(1);
        node.current_term = 2;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 }); // index 2 has term 1

        // Leader sends entry at index 2 with term 2 → conflict → truncate
        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 2,
                leader_id: 0,
                prev_log_index: 1,
                prev_log_term: 1, // matches index 1
                entries: vec![LogEntry { term: 2, value: 99 }],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.log.len(), 2);
        assert_eq!(node.log[1].term, 2);
        assert_eq!(node.log[1].value, 99);
        match &msgs[0].1 {
            Message::AppendEntriesResponse { success, .. } => assert!(*success),
            _ => panic!("expected AppendEntriesResponse"),
        }
    }

    #[test]
    fn append_multiple_entries() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![
                    LogEntry { term: 1, value: 10 },
                    LogEntry { term: 1, value: 20 },
                    LogEntry { term: 1, value: 30 },
                ],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.log.len(), 3);
        assert_eq!(node.log[0].value, 10);
        assert_eq!(node.log[1].value, 20);
        assert_eq!(node.log[2].value, 30);
    }

    #[test]
    fn idempotent_append_same_entries() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 10 });
        node.log.push(LogEntry { term: 1, value: 20 });

        // Re-send same entries — should be idempotent
        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![
                    LogEntry { term: 1, value: 10 },
                    LogEntry { term: 1, value: 20 },
                ],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.log.len(), 2);
        assert_eq!(node.log[0].value, 10);
        assert_eq!(node.log[1].value, 20);
    }

    #[test]
    fn commit_index_advances_from_leader_commit() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 });

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 2,
                prev_log_term: 1,
                entries: vec![],
                leader_commit: 2,
            },
            0,
        );

        assert_eq!(node.commit_index, 2);
    }

    #[test]
    fn commit_index_capped_by_log_length() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 1,
                prev_log_term: 1,
                entries: vec![],
                leader_commit: 10, // leader says 10, but we only have 1 entry
            },
            0,
        );

        assert_eq!(node.commit_index, 1); // capped at log length
    }

    #[test]
    fn leader_steps_down_on_higher_term_append() {
        let mut node = Node::new(0);
        node.current_term = 3;
        node.role = Role::Leader;

        node.handle_message(
            1,
            Message::AppendEntries {
                term: 5,
                leader_id: 1,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.current_term, 5);
        assert_eq!(node.role, Role::Follower);
    }

    #[test]
    fn candidate_steps_down_on_append_entries() {
        let mut node = Node::new(0);
        node.current_term = 2;
        node.role = Role::Candidate;

        node.handle_message(
            1,
            Message::AppendEntries {
                term: 2,
                leader_id: 1,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.role, Role::Follower);
    }

    // ─── Category F: AppendEntriesResponse handling ──────────────

    #[test]
    fn leader_updates_indices_on_success() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Leader;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.next_index = [2, 1, 1];
        node.match_index = [1, 0, 0];

        node.handle_message(
            1,
            Message::AppendEntriesResponse {
                term: 1,
                success: true,
                match_index: 1,
            },
            0,
        );

        assert_eq!(node.next_index[1], 2);
        assert_eq!(node.match_index[1], 1);
    }

    #[test]
    fn leader_decrements_next_index_on_failure() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Leader;
        node.next_index[1] = 5;

        node.handle_message(
            1,
            Message::AppendEntriesResponse {
                term: 1,
                success: false,
                match_index: 0,
            },
            0,
        );

        assert_eq!(node.next_index[1], 4);
    }

    #[test]
    fn leader_next_index_does_not_go_below_one() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Leader;
        node.next_index[1] = 1;

        node.handle_message(
            1,
            Message::AppendEntriesResponse {
                term: 1,
                success: false,
                match_index: 0,
            },
            0,
        );

        assert_eq!(node.next_index[1], 1); // doesn't go below 1
    }

    #[test]
    fn leader_steps_down_on_higher_term_append_response() {
        let mut node = Node::new(0);
        node.current_term = 2;
        node.role = Role::Leader;

        node.handle_message(
            1,
            Message::AppendEntriesResponse {
                term: 5,
                success: false,
                match_index: 0,
            },
            0,
        );

        assert_eq!(node.current_term, 5);
        assert_eq!(node.role, Role::Follower);
    }

    #[test]
    fn non_leader_ignores_append_response() {
        let mut node = Node::new(0);
        node.current_term = 1;
        node.role = Role::Follower;
        node.next_index[1] = 5;

        node.handle_message(
            1,
            Message::AppendEntriesResponse {
                term: 1,
                success: true,
                match_index: 3,
            },
            0,
        );

        assert_eq!(node.next_index[1], 5); // unchanged
    }

    // ─── Category G: Commit quorum logic ─────────────────────────

    #[test]
    fn try_advance_commit_requires_quorum() {
        let mut leader = Node::new(0);
        leader.current_term = 1;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.log.push(LogEntry { term: 1, value: 2 });
        leader.match_index = [2, 0, 0]; // only leader has entries

        leader.try_advance_commit();
        assert_eq!(leader.commit_index, 0, "no quorum yet");

        leader.match_index[1] = 1; // one peer has index 1
        leader.try_advance_commit();
        assert_eq!(leader.commit_index, 1, "quorum at index 1");
    }

    #[test]
    fn try_advance_commit_skips_prior_term_entries() {
        // Raft §5.4.2: leader only commits entries from its own term
        let mut leader = Node::new(0);
        leader.current_term = 3;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 }); // old term
        leader.log.push(LogEntry { term: 1, value: 2 }); // old term
        leader.match_index = [2, 2, 2]; // all peers replicated

        leader.try_advance_commit();
        assert_eq!(
            leader.commit_index, 0,
            "entries from old term should not be committed directly"
        );
    }

    #[test]
    fn try_advance_commit_commits_current_term_entries() {
        let mut leader = Node::new(0);
        leader.current_term = 2;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 }); // old term
        leader.log.push(LogEntry { term: 2, value: 2 }); // current term
        leader.match_index = [2, 2, 0]; // quorum at index 2

        leader.try_advance_commit();
        // Committing index 2 (current term) also implicitly commits index 1
        assert_eq!(leader.commit_index, 2);
    }

    #[test]
    fn try_advance_commit_sequential() {
        let mut leader = Node::new(0);
        leader.current_term = 1;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.log.push(LogEntry { term: 1, value: 2 });
        leader.log.push(LogEntry { term: 1, value: 3 });
        leader.match_index = [3, 3, 3]; // all replicated

        leader.try_advance_commit();
        assert_eq!(leader.commit_index, 3);
    }

    // ─── Category H: Heartbeats & replication messages ───────────

    #[test]
    fn heartbeats_include_entries_for_lagging_peer() {
        let mut leader = Node::new(0);
        leader.current_term = 1;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.log.push(LogEntry { term: 1, value: 2 });
        // Peer 1 is up to date, peer 2 needs entries
        leader.next_index = [3, 3, 1];

        let msgs = leader.send_heartbeats();

        for (peer, msg) in &msgs {
            match msg {
                Message::AppendEntries { entries, .. } => {
                    if *peer == 1 {
                        assert!(entries.is_empty(), "peer 1 is up to date");
                    } else if *peer == 2 {
                        assert_eq!(entries.len(), 2, "peer 2 needs both entries");
                    }
                }
                _ => panic!("expected AppendEntries"),
            }
        }
    }

    #[test]
    fn heartbeats_set_correct_prev_log() {
        let mut leader = Node::new(0);
        leader.current_term = 2;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.log.push(LogEntry { term: 2, value: 2 });
        leader.next_index = [3, 2, 1]; // peer 1 needs entry 2, peer 2 needs both

        let msgs = leader.send_heartbeats();

        for (peer, msg) in &msgs {
            match msg {
                Message::AppendEntries {
                    prev_log_index,
                    prev_log_term,
                    ..
                } => {
                    if *peer == 1 {
                        assert_eq!(*prev_log_index, 1);
                        assert_eq!(*prev_log_term, 1);
                    } else if *peer == 2 {
                        assert_eq!(*prev_log_index, 0);
                        assert_eq!(*prev_log_term, 0);
                    }
                }
                _ => panic!("expected AppendEntries"),
            }
        }
    }

    // ─── Category I: Safety check functions ──────────────────────

    #[test]
    fn election_safety_passes_one_leader_per_term() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        nodes[0].role = Role::Leader;
        nodes[0].current_term = 2;
        nodes[1].current_term = 2;
        nodes[2].current_term = 2;

        let violations = check_election_safety(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn election_safety_detects_two_leaders_same_term() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        nodes[0].role = Role::Leader;
        nodes[0].current_term = 2;
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 2;
        nodes[2].current_term = 2;

        let violations = check_election_safety(&nodes);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, 2); // term 2
        assert_eq!(violations[0].1, vec![0, 1]); // both leaders
    }

    #[test]
    fn election_safety_allows_leaders_in_different_terms() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        nodes[0].role = Role::Leader;
        nodes[0].current_term = 2;
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 3;
        nodes[2].current_term = 3;

        let violations = check_election_safety(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn election_safety_no_leaders() {
        let nodes: Vec<Node> = (0..3).map(Node::new).collect();
        let violations = check_election_safety(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn log_matching_passes_identical_logs() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        for node in &mut nodes {
            node.log.push(LogEntry { term: 1, value: 1 });
            node.log.push(LogEntry { term: 1, value: 2 });
            node.log.push(LogEntry { term: 2, value: 3 });
        }

        let violations = check_log_matching(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn log_matching_passes_different_lengths() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        // Node 0 has 3 entries, node 1 has 2, node 2 has 1
        // All share the common prefix
        nodes[0].log.push(LogEntry { term: 1, value: 1 });
        nodes[0].log.push(LogEntry { term: 1, value: 2 });
        nodes[0].log.push(LogEntry { term: 2, value: 3 });

        nodes[1].log.push(LogEntry { term: 1, value: 1 });
        nodes[1].log.push(LogEntry { term: 1, value: 2 });

        nodes[2].log.push(LogEntry { term: 1, value: 1 });

        let violations = check_log_matching(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn log_matching_detects_divergence_before_agreement() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        // Node 0: [term1, term2, term3]
        // Node 1: [term1, term9, term3]  ← diverges at index 1, agrees at index 2
        nodes[0].log.push(LogEntry { term: 1, value: 1 });
        nodes[0].log.push(LogEntry { term: 2, value: 2 });
        nodes[0].log.push(LogEntry { term: 3, value: 3 });

        nodes[1].log.push(LogEntry { term: 1, value: 1 });
        nodes[1].log.push(LogEntry { term: 9, value: 2 }); // different term
        nodes[1].log.push(LogEntry { term: 3, value: 3 }); // same term again — violation!

        let violations = check_log_matching(&nodes);
        assert!(
            !violations.is_empty(),
            "should detect divergence-then-agreement"
        );
    }

    #[test]
    fn log_matching_allows_trailing_divergence() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        // Node 0: [term1, term2]
        // Node 1: [term1, term3]  ← diverges at end, no re-agreement
        nodes[0].log.push(LogEntry { term: 1, value: 1 });
        nodes[0].log.push(LogEntry { term: 2, value: 2 });

        nodes[1].log.push(LogEntry { term: 1, value: 1 });
        nodes[1].log.push(LogEntry { term: 3, value: 2 }); // different term at end

        let violations = check_log_matching(&nodes);
        assert!(
            violations.is_empty(),
            "trailing divergence is ok (will be resolved)"
        );
    }

    #[test]
    fn leader_completeness_passes_leader_has_all_committed() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        let entries = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        for node in &mut nodes {
            node.log = entries.clone();
            node.commit_index = 2;
        }
        nodes[0].role = Role::Leader;
        nodes[0].current_term = 1;

        let violations = check_leader_completeness(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn leader_completeness_detects_missing_entry() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();
        nodes[0].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        nodes[0].commit_index = 2;

        nodes[1].log = vec![LogEntry { term: 1, value: 1 }]; // missing entry 2
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 2;

        nodes[2].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        nodes[2].commit_index = 2;

        let violations = check_leader_completeness(&nodes);
        assert!(!violations.is_empty(), "leader is missing committed entry");
    }

    #[test]
    fn leader_completeness_no_committed_entries() {
        let nodes: Vec<Node> = (0..3).map(Node::new).collect();
        let violations = check_leader_completeness(&nodes);
        assert!(violations.is_empty());
    }

    #[test]
    fn leader_completeness_skips_stale_leader() {
        // Reproduces the false-positive bug from the exploration run:
        // A stale leader (old term) coexists with a new leader that has committed
        // different entries at indices beyond the old leader's commit point.
        // This is normal Raft behavior — the stale leader hasn't learned about
        // the new election yet and will step down once it does.
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Node 0: stale leader in term 1, committed index 1,
        // has an uncommitted entry at index 2 from its own term
        nodes[0].role = Role::Leader;
        nodes[0].current_term = 1;
        nodes[0].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 }, // uncommitted, will be overwritten
        ];
        nodes[0].commit_index = 1;

        // Node 1: new leader in term 2, committed index 2 with a DIFFERENT
        // entry at index 2 (proposed in term 2 after winning election)
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 2;
        nodes[1].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry {
                term: 2,
                value: 99,
            }, // different from node 0's entry
        ];
        nodes[1].commit_index = 2;

        // Node 2: follower in term 2, has node 1's entries
        nodes[2].current_term = 2;
        nodes[2].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry {
                term: 2,
                value: 99,
            },
        ];
        nodes[2].commit_index = 2;

        // The stale leader (node 0) has a conflicting entry at index 2,
        // but it's UNCOMMITTED. Node 0 will step down and adopt node 1's
        // log once it receives a message with term 2.
        // This must NOT be flagged as a violation.
        let violations = check_leader_completeness(&nodes);
        assert!(
            violations.is_empty(),
            "stale leader should not trigger false positive: {:?}",
            violations
        );
    }

    #[test]
    fn leader_completeness_stale_leader_short_log() {
        // Stale leader with a shorter log than the committed prefix.
        // Still not a violation — it's a zombie that will step down.
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Node 0: stale leader in term 1, only 1 entry
        nodes[0].role = Role::Leader;
        nodes[0].current_term = 1;
        nodes[0].log = vec![LogEntry { term: 1, value: 1 }];
        nodes[0].commit_index = 1;

        // Node 1: new leader in term 2, committed 3 entries
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 2;
        nodes[1].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 2, value: 2 },
            LogEntry { term: 2, value: 3 },
        ];
        nodes[1].commit_index = 3;

        // Node 2: follower in term 2
        nodes[2].current_term = 2;
        nodes[2].log = nodes[1].log.clone();
        nodes[2].commit_index = 3;

        let violations = check_leader_completeness(&nodes);
        assert!(
            violations.is_empty(),
            "stale leader with short log should not trigger: {:?}",
            violations
        );
    }

    #[test]
    fn leader_completeness_catches_current_term_violation() {
        // A leader in the CURRENT term (not stale) with a wrong entry
        // IS a real violation and must be detected.
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Node 0: committed 2 entries
        nodes[0].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        nodes[0].commit_index = 2;

        // Node 1: leader in term 2 (current term), but has a WRONG entry
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 2;
        nodes[1].log = vec![
            LogEntry {
                term: 1,
                value: 999,
            }, // wrong!
            LogEntry { term: 1, value: 2 },
        ];

        // Node 2: follower in term 2
        nodes[2].current_term = 2;
        nodes[2].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        nodes[2].commit_index = 2;

        let violations = check_leader_completeness(&nodes);
        assert!(
            !violations.is_empty(),
            "current-term leader with wrong entry must be caught"
        );
    }

    // ─── Category J: Term monotonicity ───────────────────────────

    #[test]
    fn term_never_decreases_on_stale_append() {
        let mut node = Node::new(0);
        node.current_term = 5;

        node.handle_message(
            1,
            Message::AppendEntries {
                term: 3,
                leader_id: 1,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        assert!(node.current_term >= 5);
    }

    #[test]
    fn term_advances_on_higher_term_message() {
        let mut node = Node::new(0);
        node.current_term = 2;

        node.handle_message(
            1,
            Message::AppendEntries {
                term: 7,
                leader_id: 1,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.current_term, 7);
    }

    // ─── Category K: Full election scenario ──────────────────────

    #[test]
    fn full_election_three_nodes() {
        // Node 0 starts election, gets votes from 1 and 2, becomes leader
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Node 0 becomes candidate
        let vote_requests = nodes[0].become_candidate(0);
        assert_eq!(nodes[0].role, Role::Candidate);
        assert_eq!(nodes[0].current_term, 1);
        assert_eq!(vote_requests.len(), 2);

        // Deliver RequestVote to nodes 1 and 2
        let mut responses: Vec<(usize, Message)> = Vec::new();
        for (peer, msg) in vote_requests {
            let reply = nodes[peer].handle_message(0, msg, 0);
            for (to, resp) in reply {
                responses.push((to, resp));
            }
        }

        // Both should grant votes
        assert_eq!(responses.len(), 2);
        for (_, msg) in &responses {
            match msg {
                Message::RequestVoteResponse { vote_granted, .. } => {
                    assert!(*vote_granted);
                }
                _ => panic!("expected RequestVoteResponse"),
            }
        }

        // Deliver first vote response to node 0 — should become leader
        let (_, first_response) = responses.remove(0);
        let heartbeats = nodes[0].handle_message(1, first_response, 0);
        assert_eq!(nodes[0].role, Role::Leader);
        assert!(!heartbeats.is_empty(), "leader should send heartbeats");
    }

    #[test]
    fn split_vote_no_leader() {
        // Nodes 0 and 1 both start elections in the same term
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Node 0 becomes candidate for term 1
        let _ = nodes[0].become_candidate(0);
        // Node 1 also becomes candidate for term 1
        let _ = nodes[1].become_candidate(0);

        // Node 2 votes for node 0 (first request it sees)
        let resp = nodes[2].handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );
        assert!(matches!(
            &resp[0].1,
            Message::RequestVoteResponse {
                vote_granted: true,
                ..
            }
        ));

        // Node 2 rejects node 1's request (already voted for 0)
        let resp = nodes[2].handle_message(
            1,
            Message::RequestVote {
                term: 1,
                candidate_id: 1,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );
        assert!(matches!(
            &resp[0].1,
            Message::RequestVoteResponse {
                vote_granted: false,
                ..
            }
        ));

        // Node 0 gets vote from 2 → has 2 votes (self + node 2) → becomes leader
        nodes[0].handle_message(
            2,
            Message::RequestVoteResponse {
                term: 1,
                vote_granted: true,
            },
            0,
        );
        assert_eq!(nodes[0].role, Role::Leader);

        // Node 1 gets rejection from 2 → still candidate with 1 vote
        nodes[1].handle_message(
            2,
            Message::RequestVoteResponse {
                term: 1,
                vote_granted: false,
            },
            0,
        );
        assert_eq!(nodes[1].role, Role::Candidate);
        assert_eq!(nodes[1].votes_received, 1); // only self-vote
    }

    // ─── Category L: Full replication scenario ───────────────────

    #[test]
    fn leader_replicates_and_commits() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Set up node 0 as leader
        let _ = nodes[0].become_candidate(0);
        // Get votes
        for peer in 1..NUM_NODES {
            let resp = nodes[peer].handle_message(
                0,
                Message::RequestVote {
                    term: 1,
                    candidate_id: 0,
                    last_log_index: 0,
                    last_log_term: 0,
                },
                0,
            );
            for (to, msg) in resp {
                nodes[to].handle_message(peer, msg, 0);
            }
        }
        assert_eq!(nodes[0].role, Role::Leader);

        // Leader proposes a value
        nodes[0].log.push(LogEntry { term: 1, value: 42 });
        nodes[0].match_index[0] = nodes[0].log.len();

        // Send heartbeats (which include the new entry)
        let heartbeats = nodes[0].send_heartbeats();

        // Deliver to followers, collect responses
        let mut responses = Vec::new();
        for (peer, msg) in heartbeats {
            let reply = nodes[peer].handle_message(0, msg, 0);
            for (to, resp) in reply {
                responses.push((peer, to, resp));
            }
        }

        // Deliver responses back to leader
        for (from, _to, msg) in responses {
            nodes[0].handle_message(from, msg, 0);
        }

        // Leader should have committed the entry
        assert_eq!(
            nodes[0].commit_index, 1,
            "leader should commit after quorum replication"
        );

        // Followers should have the entry in their log
        assert_eq!(nodes[1].log.len(), 1);
        assert_eq!(nodes[1].log[0].value, 42);
        assert_eq!(nodes[2].log.len(), 1);
        assert_eq!(nodes[2].log[0].value, 42);
    }

    #[test]
    fn leader_retries_after_log_inconsistency() {
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Set up node 0 as leader at term 2
        nodes[0].current_term = 2;
        nodes[0].role = Role::Leader;
        nodes[0].log.push(LogEntry { term: 1, value: 1 });
        nodes[0].log.push(LogEntry { term: 2, value: 2 });
        nodes[0].match_index[0] = 2;
        nodes[0].next_index = [3, 3, 3]; // optimistically high

        // Node 1 has a conflicting log (different entry at index 2)
        nodes[1].current_term = 2;
        nodes[1].log.push(LogEntry { term: 1, value: 1 });
        // Node 1 is missing index 2

        // Leader sends AppendEntries with prev_log_index=2 — will fail
        let msgs = nodes[0].send_heartbeats();
        let peer1_msg = msgs.into_iter().find(|(p, _)| *p == 1).unwrap().1;
        let resp = nodes[1].handle_message(0, peer1_msg, 0);

        // Should get failure response
        match &resp[0].1 {
            Message::AppendEntriesResponse { success, .. } => assert!(!success),
            _ => panic!("expected AppendEntriesResponse"),
        }

        // Deliver failure to leader — next_index[1] should decrement
        nodes[0].handle_message(1, resp[0].1.clone(), 0);
        assert_eq!(nodes[0].next_index[1], 2); // decremented from 3

        // Leader retries with lower prev_log_index
        let msgs = nodes[0].send_heartbeats();
        let peer1_msg = msgs.into_iter().find(|(p, _)| *p == 1).unwrap().1;
        let resp = nodes[1].handle_message(0, peer1_msg, 0);

        // Should succeed now
        match &resp[0].1 {
            Message::AppendEntriesResponse { success, .. } => assert!(success),
            _ => panic!("expected AppendEntriesResponse"),
        }

        // Node 1 should now have both entries
        assert_eq!(nodes[1].log.len(), 2);
        assert_eq!(nodes[1].log[1].value, 2);
    }

    // ─── Category M: TestCluster simulation tests ────────────────

    #[test]
    fn cluster_elects_leader_within_50_ticks() {
        let mut cluster = TestCluster::new(42);
        cluster.run_checked(50);
        assert!(
            cluster.has_leader(),
            "should elect a leader within 50 ticks"
        );
    }

    #[test]
    fn cluster_commits_values_within_200_ticks() {
        let mut cluster = TestCluster::new(42);
        cluster.run_checked(200);
        assert!(
            cluster.max_commit() > 0,
            "should commit at least one value within 200 ticks"
        );
    }

    #[test]
    fn cluster_safety_holds_1000_ticks() {
        // Extended run — checks all safety invariants every tick
        let mut cluster = TestCluster::new(12345);
        cluster.run_checked(1000);
    }

    #[test]
    fn cluster_safety_multiple_seeds() {
        // Run with multiple seeds to increase coverage
        for seed in [0, 1, 42, 100, 999, 0xDEAD, 0xBEEF, 0xCAFE] {
            let mut cluster = TestCluster::new(seed);
            cluster.run_checked(500);
        }
    }

    #[test]
    fn cluster_deterministic_with_same_seed() {
        let mut c1 = TestCluster::new(42);
        let mut c2 = TestCluster::new(42);

        for _ in 0..200 {
            let a1 = c1.step();
            let a2 = c2.step();
            assert_eq!(a1, a2, "same seed should produce same active node");
        }

        // State should be identical
        for i in 0..NUM_NODES {
            assert_eq!(c1.nodes[i].current_term, c2.nodes[i].current_term);
            assert_eq!(c1.nodes[i].role, c2.nodes[i].role);
            assert_eq!(c1.nodes[i].commit_index, c2.nodes[i].commit_index);
            assert_eq!(c1.nodes[i].log.len(), c2.nodes[i].log.len());
        }
    }

    #[test]
    fn cluster_different_seeds_diverge() {
        let mut c1 = TestCluster::new(1);
        let mut c2 = TestCluster::new(2);

        c1.run_checked(200);
        c2.run_checked(200);

        // With different seeds, at least some state should differ
        let same_terms =
            (0..NUM_NODES).all(|i| c1.nodes[i].current_term == c2.nodes[i].current_term);
        let same_commits =
            (0..NUM_NODES).all(|i| c1.nodes[i].commit_index == c2.nodes[i].commit_index);
        assert!(
            !same_terms || !same_commits,
            "different seeds should produce different cluster states"
        );
    }

    // ─── Category N: Edge cases ──────────────────────────────────

    #[test]
    fn empty_append_entries_is_heartbeat() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;
        let initial_timer = node.election_timer;

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            ELECTION_TIMEOUT_JITTER, // max jitter
        );

        assert!(node.log.is_empty(), "no entries appended");
        // Timer was reset (should be different from initial value)
        assert_eq!(
            node.election_timer,
            ELECTION_TIMEOUT_BASE + ELECTION_TIMEOUT_JITTER
        );
        assert_ne!(node.election_timer, initial_timer);
    }

    #[test]
    fn candidate_resets_on_new_election() {
        let mut node = Node::new(0);
        node.current_term = 3;
        node.role = Role::Candidate;
        node.votes_received = 1;
        node.voted_for = Some(0);

        // Start a new election
        let msgs = node.become_candidate(0);

        assert_eq!(node.current_term, 4);
        assert_eq!(node.votes_received, 1); // reset to self-vote
        assert_eq!(node.voted_for, Some(0));
        assert_eq!(msgs.len(), 2); // new vote requests
    }

    #[test]
    fn commit_index_never_decreases() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 });
        node.commit_index = 2;

        // Receive AppendEntries with lower leader_commit
        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 2,
                prev_log_term: 1,
                entries: vec![],
                leader_commit: 1, // lower than current commit
            },
            0,
        );

        assert_eq!(node.commit_index, 2, "commit_index should never decrease");
    }

    #[test]
    fn leader_commit_advances_via_replication_response() {
        // End-to-end: leader proposes, replicates, gets ack, commits
        let mut leader = Node::new(0);
        leader.current_term = 1;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.match_index = [1, 0, 0]; // only leader has it
        leader.next_index = [2, 1, 1];

        assert_eq!(leader.commit_index, 0);

        // Peer 1 acks
        leader.handle_message(
            1,
            Message::AppendEntriesResponse {
                term: 1,
                success: true,
                match_index: 1,
            },
            0,
        );

        assert_eq!(leader.commit_index, 1, "quorum reached → committed");
    }

    // ─── Category O: Coverage gap tests ──────────────────────────

    #[test]
    fn leader_completeness_detects_wrong_entry_content() {
        // Covers line 444: leader has an entry at the committed index but
        // with the wrong term/value (not just "log too short").
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        // Node 0 has the canonical committed log
        nodes[0].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        nodes[0].commit_index = 2;

        // Node 1 is leader but has a WRONG entry at index 1
        nodes[1].role = Role::Leader;
        nodes[1].current_term = 2;
        nodes[1].log = vec![
            LogEntry {
                term: 1,
                value: 999,
            }, // wrong value
            LogEntry { term: 1, value: 2 },
        ];
        nodes[1].commit_index = 0;

        nodes[2].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 1, value: 2 },
        ];
        nodes[2].commit_index = 2;

        let violations = check_leader_completeness(&nodes);
        assert!(!violations.is_empty(), "leader has wrong entry at index 0");
        assert_eq!(violations[0], (1, 0)); // leader 1, index 0
    }

    #[test]
    fn leader_completeness_detects_wrong_term() {
        // Leader has same value but wrong term at a committed index.
        let mut nodes: Vec<Node> = (0..3).map(Node::new).collect();

        nodes[0].log = vec![LogEntry { term: 1, value: 1 }];
        nodes[0].commit_index = 1;

        nodes[1].role = Role::Leader;
        nodes[1].current_term = 3;
        nodes[1].log = vec![LogEntry { term: 2, value: 1 }]; // wrong term
        nodes[1].commit_index = 0;

        nodes[2].log = vec![LogEntry { term: 1, value: 1 }];
        nodes[2].commit_index = 1;

        let violations = check_leader_completeness(&nodes);
        assert!(!violations.is_empty(), "leader has wrong term at index 0");
    }

    #[test]
    fn cluster_leaders_returns_current_leaders() {
        // Covers lines 601-605: TestCluster::leaders() method.
        let mut cluster = TestCluster::new(42);

        // Initially no leaders
        assert!(cluster.leaders().is_empty());

        // Run until a leader is elected
        cluster.run_checked(50);
        let leaders = cluster.leaders();
        assert!(
            !leaders.is_empty(),
            "should have at least one leader after 50 ticks"
        );
        // All returned IDs should be valid node IDs
        for &id in &leaders {
            assert!(id < NUM_NODES);
            assert_eq!(cluster.nodes[id].role, Role::Leader);
        }
    }

    #[test]
    #[should_panic(expected = "leader completeness violated")]
    fn run_checked_panics_on_completeness_violation() {
        // Covers the panic branch in run_checked().
        // Poison the cluster with a current-term leader that has the wrong entry.
        let mut cluster = TestCluster::new(42);

        // Force node 0 as the sole current-term leader with a bad log.
        // All nodes in the same term so the stale-leader guard doesn't skip it.
        let term = 10;
        cluster.nodes[0].role = Role::Leader;
        cluster.nodes[0].current_term = term;
        cluster.nodes[0].log = vec![LogEntry {
            term: 1,
            value: 999, // wrong entry
        }];

        cluster.nodes[1].role = Role::Follower;
        cluster.nodes[1].current_term = term;
        cluster.nodes[1].log = vec![LogEntry { term: 1, value: 1 }];
        cluster.nodes[1].commit_index = 1;

        cluster.nodes[2].role = Role::Follower;
        cluster.nodes[2].current_term = term;
        cluster.nodes[2].log = vec![LogEntry { term: 1, value: 1 }];
        cluster.nodes[2].commit_index = 1;

        cluster.run_checked(1);
    }

    #[test]
    #[should_panic(expected = "election safety violated")]
    fn run_checked_panics_on_election_safety_violation() {
        // Covers the election safety panic branch in run_checked().
        let mut cluster = TestCluster::new(42);

        // Poison: two leaders in the same term
        cluster.nodes[0].role = Role::Leader;
        cluster.nodes[0].current_term = 1;
        cluster.nodes[1].role = Role::Leader;
        cluster.nodes[1].current_term = 1;

        cluster.run_checked(1);
    }

    #[test]
    #[should_panic(expected = "log matching violated")]
    fn run_checked_panics_on_log_matching_violation() {
        // Covers the log matching panic branch in run_checked().
        let mut cluster = TestCluster::new(42);

        // Poison: logs that diverge then re-agree (violates log matching)
        cluster.nodes[0].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 2, value: 2 },
            LogEntry { term: 3, value: 3 },
        ];
        cluster.nodes[1].log = vec![
            LogEntry { term: 1, value: 1 },
            LogEntry { term: 9, value: 2 }, // diverges
            LogEntry { term: 3, value: 3 }, // re-agrees — violation
        ];

        cluster.run_checked(1);
    }

    // ═══════════════════════════════════════════════════════════════
    //  Category P: Bug injection validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn bug_mode_parse() {
        assert_eq!(BugMode::parse("fig8"), BugMode::Fig8Commit);
        assert_eq!(BugMode::parse("fig8_commit"), BugMode::Fig8Commit);
        assert_eq!(BugMode::parse("skip_truncate"), BugMode::SkipTruncate);
        assert_eq!(BugMode::parse("no_truncate"), BugMode::SkipTruncate);
        assert_eq!(BugMode::parse("accept_stale"), BugMode::AcceptStaleTerm);
        assert_eq!(
            BugMode::parse("leader_no_stepdown"),
            BugMode::LeaderNoStepdown
        );
        assert_eq!(BugMode::parse("double_vote"), BugMode::DoubleVote);
        assert_eq!(BugMode::parse("premature_commit"), BugMode::PrematureCommit);
        assert_eq!(BugMode::parse("none"), BugMode::None);
        assert_eq!(BugMode::parse("unknown"), BugMode::None);
    }

    #[test]
    fn bug_mode_name_roundtrip() {
        for bug in [
            BugMode::None,
            BugMode::Fig8Commit,
            BugMode::SkipTruncate,
            BugMode::AcceptStaleTerm,
            BugMode::LeaderNoStepdown,
            BugMode::DoubleVote,
            BugMode::PrematureCommit,
        ] {
            assert_eq!(BugMode::parse(bug.name()), bug);
        }
    }

    #[test]
    fn bug_none_is_safe() {
        let mut cluster = TestCluster::new_with_bug(42, BugMode::None);
        cluster.run_checked(1000);
    }

    #[test]
    fn bug_none_safe_extended_10k_seeds() {
        // Aggressive search for false positives — exercises the corrected
        // check_leader_completeness (stale-leader guard) and the corrected
        // match_index (verified-point-only) across 10K seeds.
        for seed in 0..10_000 {
            let mut cluster = TestCluster::new(seed);
            cluster.run_checked(500);
        }
    }

    // ── Fig8Commit ───────────────────────────────────────────────

    #[test]
    fn bug_fig8_commit_allows_old_term_commit() {
        let mut leader = Node::new_with_bug(0, BugMode::Fig8Commit);
        leader.current_term = 3;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.log.push(LogEntry { term: 2, value: 2 });
        leader.match_index = [2, 2, 0];

        leader.try_advance_commit();
        assert_eq!(leader.commit_index, 2, "fig8 bug allows old-term commit");
    }

    #[test]
    fn bug_fig8_commit_correct_blocks_old_term() {
        let mut leader = Node::new(0);
        leader.current_term = 3;
        leader.role = Role::Leader;
        leader.log.push(LogEntry { term: 1, value: 1 });
        leader.log.push(LogEntry { term: 2, value: 2 });
        leader.match_index = [2, 2, 0];

        leader.try_advance_commit();
        assert_eq!(
            leader.commit_index, 0,
            "correct impl blocks old-term commit"
        );
    }

    // ── SkipTruncate ─────────────────────────────────────────────

    #[test]
    fn bug_skip_truncate_keeps_stale_entries() {
        let mut node = Node::new_with_bug(1, BugMode::SkipTruncate);
        node.current_term = 2;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 });

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 2,
                leader_id: 0,
                prev_log_index: 1,
                prev_log_term: 1,
                entries: vec![LogEntry { term: 2, value: 99 }],
                leader_commit: 0,
            },
            0,
        );

        // Bug: old entry NOT truncated, new entry appended after it
        assert!(
            node.log.len() > 2,
            "skip_truncate appends without truncating"
        );
    }

    #[test]
    fn bug_skip_truncate_correct_truncates() {
        let mut node = Node::new(1);
        node.current_term = 2;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });
        node.log.push(LogEntry { term: 1, value: 2 });

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 2,
                leader_id: 0,
                prev_log_index: 1,
                prev_log_term: 1,
                entries: vec![LogEntry { term: 2, value: 99 }],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(
            node.log.len(),
            2,
            "correct impl truncates conflicting entry"
        );
        assert_eq!(node.log[1].term, 2);
    }

    // ── AcceptStaleTerm ──────────────────────────────────────────

    #[test]
    fn bug_accept_stale_term_processes_old_leader() {
        let mut node = Node::new_with_bug(1, BugMode::AcceptStaleTerm);
        node.current_term = 5;
        node.role = Role::Follower;

        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 3,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![LogEntry { term: 3, value: 42 }],
                leader_commit: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::AppendEntriesResponse { success, .. } => {
                assert!(*success, "bug accepts stale-term AppendEntries");
            }
            _ => panic!("expected AppendEntriesResponse"),
        }
        assert_eq!(node.log.len(), 1);
    }

    #[test]
    fn bug_accept_stale_term_correct_rejects() {
        let mut node = Node::new(1);
        node.current_term = 5;

        let msgs = node.handle_message(
            0,
            Message::AppendEntries {
                term: 3,
                leader_id: 0,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![LogEntry { term: 3, value: 42 }],
                leader_commit: 0,
            },
            0,
        );

        match &msgs[0].1 {
            Message::AppendEntriesResponse { success, .. } => {
                assert!(!success, "correct impl rejects stale-term AppendEntries");
            }
            _ => panic!("expected AppendEntriesResponse"),
        }
    }

    // ── LeaderNoStepdown ─────────────────────────────────────────

    #[test]
    fn bug_leader_no_stepdown_stays_leader() {
        let mut node = Node::new_with_bug(0, BugMode::LeaderNoStepdown);
        node.current_term = 3;
        node.role = Role::Leader;

        node.handle_message(
            1,
            Message::AppendEntries {
                term: 5,
                leader_id: 1,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(
            node.role,
            Role::Leader,
            "bug: leader ignores higher-term AE"
        );
        assert_eq!(node.current_term, 3, "bug: term not updated");
    }

    #[test]
    fn bug_leader_no_stepdown_correct_steps_down() {
        let mut node = Node::new(0);
        node.current_term = 3;
        node.role = Role::Leader;

        node.handle_message(
            1,
            Message::AppendEntries {
                term: 5,
                leader_id: 1,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            },
            0,
        );

        assert_eq!(node.role, Role::Follower);
        assert_eq!(node.current_term, 5);
    }

    // ── DoubleVote ───────────────────────────────────────────────

    #[test]
    fn bug_double_vote_grants_two_votes() {
        let mut node = Node::new_with_bug(2, BugMode::DoubleVote);
        node.current_term = 1;

        let msgs = node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );
        assert!(matches!(
            &msgs[0].1,
            Message::RequestVoteResponse {
                vote_granted: true,
                ..
            }
        ));

        let msgs = node.handle_message(
            1,
            Message::RequestVote {
                term: 1,
                candidate_id: 1,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );
        assert!(
            matches!(
                &msgs[0].1,
                Message::RequestVoteResponse {
                    vote_granted: true,
                    ..
                }
            ),
            "bug: grants vote to second candidate"
        );
    }

    #[test]
    fn bug_double_vote_correct_denies_second() {
        let mut node = Node::new(2);
        node.current_term = 1;

        node.handle_message(
            0,
            Message::RequestVote {
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );

        let msgs = node.handle_message(
            1,
            Message::RequestVote {
                term: 1,
                candidate_id: 1,
                last_log_index: 0,
                last_log_term: 0,
            },
            0,
        );
        assert!(
            matches!(
                &msgs[0].1,
                Message::RequestVoteResponse {
                    vote_granted: false,
                    ..
                }
            ),
            "correct impl denies second vote"
        );
    }

    // ── PrematureCommit ──────────────────────────────────────────

    #[test]
    fn bug_premature_commit_advances_before_check() {
        let mut node = Node::new_with_bug(1, BugMode::PrematureCommit);
        node.current_term = 1;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 5, // inconsistent
                prev_log_term: 1,
                entries: vec![],
                leader_commit: 1,
            },
            0,
        );

        assert_eq!(
            node.commit_index, 1,
            "bug: premature commit before log check"
        );
    }

    #[test]
    fn bug_premature_commit_correct_no_advance() {
        let mut node = Node::new(1);
        node.current_term = 1;
        node.role = Role::Follower;
        node.log.push(LogEntry { term: 1, value: 1 });

        node.handle_message(
            0,
            Message::AppendEntries {
                term: 1,
                leader_id: 0,
                prev_log_index: 5,
                prev_log_term: 1,
                entries: vec![],
                leader_commit: 1,
            },
            0,
        );

        assert_eq!(
            node.commit_index, 0,
            "correct impl doesn't commit on failed check"
        );
    }

    // ── Cluster-level bug detection ──────────────────────────────

    /// Run a buggy cluster and return whether a safety violation was found.
    fn detect_violation(bug: BugMode, seed: u64, ticks: usize) -> bool {
        let mut cluster = TestCluster::new_with_bug(seed, bug);
        for _ in 0..ticks {
            cluster.step();
            if !check_election_safety(&cluster.nodes).is_empty()
                || !check_log_matching(&cluster.nodes).is_empty()
                || !check_leader_completeness(&cluster.nodes).is_empty()
            {
                return true;
            }
        }
        false
    }

    #[test]
    fn bug_double_vote_found_by_cluster() {
        // DoubleVote: two candidates in same term, node votes for both
        // → two leaders in same term → election safety violation.
        let found = (0..1000).any(|seed| detect_violation(BugMode::DoubleVote, seed, 3000));
        assert!(
            found,
            "DoubleVote should be detected within 1000 seeds × 3000 ticks"
        );
    }

    #[test]
    fn bug_skip_truncate_found_by_cluster() {
        // SkipTruncate: conflicting entries not removed → log diverges then
        // re-agrees at a later index → log matching violation.
        let found = (0..100).any(|seed| detect_violation(BugMode::SkipTruncate, seed, 2000));
        assert!(
            found,
            "SkipTruncate should be detected within 100 seeds × 2000 ticks"
        );
    }

    #[test]
    fn bug_leader_no_stepdown_cluster_runs() {
        // LeaderNoStepdown primarily causes a liveness issue (stale leader
        // persists, wastes resources) rather than a safety violation.
        // With correct match_index, the stale leader can't replicate to
        // followers in the new term (they reject old-term AppendEntries).
        // The explorer's fault injection (partitions, targeted kills) is
        // needed to surface the safety violation.
        let mut cluster = TestCluster::new_with_bug(42, BugMode::LeaderNoStepdown);
        for _ in 0..2000 {
            cluster.step();
        }
    }

    #[test]
    fn bug_accept_stale_term_cluster_runs() {
        // AcceptStaleTerm: follower accepts entries from stale-term leader.
        // With correct match_index, the stale leader's entries are not
        // counted toward commit (match_index reflects only verified entries).
        // Safety violations require specific partition/heal sequences best
        // tested through the exploration engine.
        let mut cluster = TestCluster::new_with_bug(42, BugMode::AcceptStaleTerm);
        for _ in 0..2000 {
            cluster.step();
        }
    }

    #[test]
    fn bug_fig8_cluster_runs_without_panic() {
        // Fig8 is the subtlest bug — may not trigger in simple cluster runs.
        // Just verify the code runs (we'll find it with the explorer).
        let mut cluster = TestCluster::new_with_bug(42, BugMode::Fig8Commit);
        for _ in 0..1000 {
            cluster.step();
        }
    }

    #[test]
    fn bug_premature_commit_cluster_runs() {
        let mut cluster = TestCluster::new_with_bug(42, BugMode::PrematureCommit);
        for _ in 0..1000 {
            cluster.step();
        }
    }
}
