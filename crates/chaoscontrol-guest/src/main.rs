//! Minimal SDK-instrumented guest program for ChaosControl.
//!
//! Runs as `/init` (PID 1) inside a deterministic VM.  Exercises the
//! full guest-side SDK surface:
//!
//! - **Lifecycle**: `setup_complete()` to ungate fault injection.
//! - **Assertions**: `always`, `sometimes`, `reachable` properties.
//! - **Randomness**: `random_choice()` for VMM-guided decisions.
//! - **Coverage**: `record_edge()` for AFL-style coverage feedback.
//!
//! The explore loop reads coverage from the guest bitmap and collects
//! assertion verdicts from the property oracle.  Without this program
//! the exploration runs blind and the SDK is dead code.
//!
//! # Build & package
//!
//! ```sh
//! scripts/build-guest.sh          # → guest/initrd-sdk.gz
//! ```
//!
//! # Integration test
//!
//! ```sh
//! cargo run --release --bin sdk_guest_test -- result-dev/vmlinux
//! ```

use chaoscontrol_sdk::prelude::*;
use chaoscontrol_sdk::{coverage, kcov, lifecycle, random};
use serde_json::json;

// ═══════════════════════════════════════════════════════════════════════
//  Workload
// ═══════════════════════════════════════════════════════════════════════

/// Number of workload iterations.
const ITERATIONS: usize = 50;
/// Number of random choices per iteration.
const NUM_CHOICES: usize = 4;

fn main() {
    // ── Phase 0: early init ─────────────────────────────────────
    guest_init();
    println!("chaoscontrol-guest: starting");

    // ── Phase 2: signal setup complete ──────────────────────────
    lifecycle::setup_complete(&json!({"program": "chaoscontrol-guest", "version": "0.1.0"}));
    println!("chaoscontrol-guest: setup_complete");

    // ── Phase 3: SDK-instrumented workload ──────────────────────
    let mut choice_counts = [0u32; NUM_CHOICES];

    for i in 0..ITERATIONS {
        // VMM-guided random decision
        let choice = random::random_choice(NUM_CHOICES);
        choice_counts[choice] += 1;

        // AFL-style edge: hash of (iteration, choice)
        coverage::record_edge(i * 31 + choice * 17);

        // ── Safety property: choice always in range ─────────────
        cc_assert_always!(choice < NUM_CHOICES, "random choice in range", &json!({}));

        // ── Liveness: eventually see each choice value ──────────
        cc_assert_sometimes!(choice == 0, "saw choice 0", &json!({}));
        cc_assert_sometimes!(choice == 1, "saw choice 1", &json!({}));
        cc_assert_sometimes!(choice == 2, "saw choice 2", &json!({}));
        cc_assert_sometimes!(choice == 3, "saw choice 3", &json!({}));

        // ── Path-specific coverage + reachability ───────────────
        match choice {
            0 => {
                cc_assert_reachable!("path A", &json!({}));
                coverage::record_edge(10_000);
            }
            1 => {
                cc_assert_reachable!("path B", &json!({}));
                coverage::record_edge(20_000);
            }
            2 => {
                cc_assert_reachable!("path C", &json!({}));
                coverage::record_edge(30_000);
            }
            3 => {
                cc_assert_reachable!("path D", &json!({}));
                coverage::record_edge(40_000);
            }
            _ => {
                cc_assert_unreachable!("impossible choice value", &json!({}));
            }
        }

        // ── Drain kernel coverage into bitmap ────────────────────
        kcov::collect();

        // ── Heartbeat every 10 iterations ───────────────────────
        if i % 10 == 0 {
            println!("heartbeat {}", i / 10);
        }
    }

    // ── Phase 4: summary ────────────────────────────────────────
    cc_assert_sometimes!(true, "workload completed", &json!({}));

    lifecycle::send_event("workload_done", &json!({"iterations": 50}));

    println!("chaoscontrol-guest: workload complete");
    println!(
        "chaoscontrol-guest: choices={},{},{},{}",
        choice_counts[0], choice_counts[1], choice_counts[2], choice_counts[3],
    );
    if kcov::is_active() {
        println!(
            "chaoscontrol-guest: kcov collected {} kernel PCs",
            kcov::total_pcs_collected()
        );
    }

    // ── Phase 5: halt ───────────────────────────────────────────
    println!("chaoscontrol-guest: done, idling");
    loop {
        unsafe {
            libc::pause();
        }
    }
}
