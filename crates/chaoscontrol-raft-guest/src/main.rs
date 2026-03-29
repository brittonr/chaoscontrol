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
//  Init
// ═══════════════════════════════════════════════════════════════════════

fn mount_devtmpfs() {
    unsafe {
        libc::mkdir(c"/dev".as_ptr().cast(), 0o755);
        let ret = libc::mount(
            c"devtmpfs".as_ptr().cast(),
            c"/dev".as_ptr().cast(),
            c"devtmpfs".as_ptr().cast(),
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
//  Kernel cmdline parsing
// ═══════════════════════════════════════════════════════════════════════

fn mount_proc() {
    unsafe {
        libc::mkdir(c"/proc".as_ptr().cast(), 0o555);
        libc::mount(
            c"proc".as_ptr().cast(),
            c"/proc".as_ptr().cast(),
            c"proc".as_ptr().cast(),
            0,
            std::ptr::null(),
        );
    }
}

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
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    mount_devtmpfs();
    mount_proc();

    let bug = parse_bug_mode();
    println!("raft: starting 3-node cluster (bug={})", bug.name());

    chaoscontrol_init();
    coverage::init();
    let kcov_ok = kcov::init();
    lifecycle::setup_complete(&json!({"program": "raft-guest", "nodes": 3, "bug": bug.name()}));
    println!(
        "raft: setup_complete (kcov={}, bug={})",
        if kcov_ok { "active" } else { "unavailable" },
        bug.name(),
    );

    // Initialize 3 nodes with the selected bug mode
    let mut nodes: Vec<Node> = (0..NUM_NODES).map(|i| Node::new_with_bug(i, bug)).collect();
    // Stagger initial election timers
    for (i, node) in nodes.iter_mut().enumerate() {
        node.election_timer = ELECTION_TIMEOUT_BASE + i * 3;
    }

    let mut values_proposed = 0u64;
    let mut values_committed = 0usize;
    let mut tick = 0usize;

    loop {
        // ── Pick which node to activate this tick ────────────
        let active = random::random_choice(NUM_NODES);
        coverage::record_edge(6000 + tick * 7 + active * 3);

        // ── Get jitter for this tick ─────────────────────────
        let jitter = random::random_choice(ELECTION_TIMEOUT_JITTER + 1);

        // ── Tick timers on active node ───────────────────────
        let mut outbox: Vec<(usize, usize, Message)> = Vec::new(); // (from, to, msg)

        {
            let node = &mut nodes[active];

            // Drain inbox
            let inbox: Vec<(usize, Message)> = node.inbox.drain(..).collect();
            for (from, msg) in inbox {
                // ── Handler reachability (1.1–1.4) ───────────
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

                // Capture AE context for log conflict detection (5.1–5.3)
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

                // Capture RequestVote candidate_id for voted_for check (4.4)
                let rv_candidate = if let Message::RequestVote { candidate_id, .. } = &msg {
                    Some(*candidate_id)
                } else {
                    None
                };

                let replies = node.handle_message(from, msg, jitter);

                // ── State transitions (2.2, 2.3, 2.4) ───────
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

                // ── Data invariant: commit_index bounded (4.1) ──
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
                        // 3.4: vote granted vs denied
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
                            // 4.4: voted_for consistent after grant
                            if let (true, Some(cid)) = (*vote_granted, rv_candidate) {
                                cc_assert_always!(
                                    node.voted_for == Some(cid),
                                    "voted_for matches granted candidate",
                                    &json!({"node": active, "candidate_id": cid}),
                                );
                            }
                        }
                        // 3.5: append accepted vs rejected
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

                // ── Data invariants for leader processing AER (4.2, 4.3) ──
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

                // ── Log conflict paths (5.1–5.3) ────────────
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
                                &json!({"node": active})
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
                    // 3.2: election timeout sometimes-pair
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
                        // 2.1: follower started election
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
                        // 4.5: leader match_index self-consistent
                        cc_assert_always!(
                            node.match_index[node.id] == node.log.len(),
                            "leader self match_index tracks log",
                            &json!({"node": active, "match_index": node.match_index[node.id], "log_len": node.log.len()}),
                        );
                        let old_commit = node.commit_index;
                        node.try_advance_commit();
                        // 3.1: commit advancement sometimes-pair
                        let commit_advanced = node.commit_index > old_commit;
                        cc_assert_sometimes!(
                            commit_advanced,
                            "commit index advanced",
                            &json!({"node": active, "commit": node.commit_index}),
                        );
                        cc_assert_sometimes!(
                            !commit_advanced,
                            "commit index not advanced",
                            &json!({"node": active, "commit": node.commit_index}),
                        );
                        // 4.1: commit_index bounded after leader advance
                        cc_assert_always!(
                            node.commit_index <= node.log.len(),
                            "commit_index within log bounds",
                            &json!({"node": active, "commit_index": node.commit_index, "log_len": node.log.len()}),
                        );
                        coverage::record_edge(7000 + values_proposed as usize);
                    }
                    // 3.6: leader proposed vs skipped
                    cc_assert_sometimes!(
                        proposed,
                        "leader proposed value",
                        &json!({"node": active})
                    );
                    cc_assert_sometimes!(
                        !proposed,
                        "leader skipped proposal",
                        &json!({"node": active}),
                    );
                }
            }
        }

        // ── Maybe drop a message (simulated network fault) ──
        // SDK randomness controls whether messages arrive
        for (from, to, msg) in outbox {
            let deliver = random::random_choice(100) < 95; // 5% drop rate
                                                           // 3.3: message delivered vs dropped
            cc_assert_sometimes!(
                deliver,
                "message delivered",
                &details::network(from, to, true),
            );
            cc_assert_sometimes!(
                !deliver,
                "message dropped",
                &details::network(from, to, false),
            );
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
        let leader_node = nodes.iter().find(|n| n.role == Role::Leader);
        let leader_term = leader_node.map_or(0, |n| n.current_term);
        let leader_detail = details::node(
            leader_node.map_or(usize::MAX, |n| n.id),
            leader_term,
            leader_node.map_or("none", |n| role_str(n.role)),
        );

        let election_violations = check_election_safety(&nodes);
        cc_assert_always!(
            election_violations.is_empty(),
            "election safety: at most one leader per term",
            &leader_detail,
        );

        let log_violations = check_log_matching(&nodes);
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

        let completeness_violations = check_leader_completeness(&nodes);
        cc_assert_always!(
            completeness_violations.is_empty(),
            "leader completeness: committed entry preserved",
            &details::merge(&leader_detail, &json!({"max_commit": max_commit})),
        );

        // ── Liveness checks ─────────────────────────────────
        let has_leader = leader_node.is_some();
        cc_assert_sometimes!(has_leader, "leader elected", &leader_detail);
        cc_assert_sometimes!(
            values_committed > 0,
            "value committed",
            &details::node(active, nodes[active].current_term, role_str(nodes[active].role)),
        );
        cc_assert_sometimes!(
            values_committed >= 3,
            "3+ values committed",
            &details::node(active, nodes[active].current_term, role_str(nodes[active].role)),
        );

        // ── Drain kernel coverage into bitmap ───────────────
        kcov::collect();

        // ── Heartbeat ───────────────────────────────────────
        if tick.is_multiple_of(40) {
            let leader_id = nodes.iter().find(|n| n.role == Role::Leader).map(|n| n.id);
            println!(
                "raft: tick={} leader={:?} terms=[{},{},{}] commits=[{},{},{}] proposed={}",
                tick,
                leader_id,
                nodes[0].current_term,
                nodes[1].current_term,
                nodes[2].current_term,
                nodes[0].commit_index,
                nodes[1].commit_index,
                nodes[2].commit_index,
                values_proposed,
            );
        }

        tick += 1;
    }
    // Guest never reaches here — the VMM controls execution via run_bounded().
}
