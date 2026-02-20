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
    check_election_safety, check_leader_completeness, check_log_matching, LogEntry, Message, Node,
    Role, ELECTION_TIMEOUT_BASE, ELECTION_TIMEOUT_JITTER, HEARTBEAT_INTERVAL, NUM_NODES,
};
use chaoscontrol_sdk::{assert, coverage, kcov, lifecycle, random};

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
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    mount_devtmpfs();
    println!("raft: starting 3-node cluster");

    coverage::init();
    let kcov_ok = kcov::init();
    lifecycle::setup_complete(&[("program", "raft-guest"), ("nodes", "3")]);
    println!(
        "raft: setup_complete (kcov={})",
        if kcov_ok { "active" } else { "unavailable" }
    );

    // Initialize 3 nodes
    let mut nodes: Vec<Node> = (0..NUM_NODES).map(Node::new).collect();
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
        let election_violations = check_election_safety(&nodes);
        assert::always(
            election_violations.is_empty(),
            "election safety: at most one leader per term",
            &[],
        );

        let log_violations = check_log_matching(&nodes);
        assert::always(
            log_violations.is_empty(),
            "log matching: divergence before agreement",
            &[],
        );

        let completeness_violations = check_leader_completeness(&nodes);
        assert::always(
            completeness_violations.is_empty(),
            "leader completeness: committed entry preserved",
            &[],
        );

        // ── Liveness checks ─────────────────────────────────
        let has_leader = nodes.iter().any(|n| n.role == Role::Leader);
        assert::sometimes(has_leader, "leader elected", &[]);
        assert::sometimes(values_committed > 0, "value committed", &[]);
        assert::sometimes(values_committed >= 3, "3+ values committed", &[]);

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
