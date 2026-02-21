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

use chaoscontrol_sdk::{assert, coverage, kcov, lifecycle, random};
use serde_json::json;

// ═══════════════════════════════════════════════════════════════════════
//  Init helpers — mount devtmpfs so /dev/mem + /dev/port exist
// ═══════════════════════════════════════════════════════════════════════

/// Mount devtmpfs on `/dev` so the SDK can access `/dev/mem` (shared
/// memory page) and `/dev/port` (I/O port trigger).  The kernel has
/// already opened `/dev/console` for us on fd 0/1/2, so `println!`
/// works before this call.
fn mount_devtmpfs() {
    unsafe {
        // Ensure /dev exists (may already from initramfs)
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
            // EBUSY (16) is fine — already mounted
            if err != libc::EBUSY {
                eprintln!("chaoscontrol-guest: mount devtmpfs failed (errno={})", err,);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Workload
// ═══════════════════════════════════════════════════════════════════════

/// Number of workload iterations.
const ITERATIONS: usize = 50;
/// Number of random choices per iteration.
const NUM_CHOICES: usize = 4;

fn main() {
    // ── Phase 0: early init ─────────────────────────────────────
    mount_devtmpfs();
    println!("chaoscontrol-guest: starting");

    // ── Phase 1: coverage init ──────────────────────────────────
    coverage::init();
    let kcov_ok = kcov::init();
    println!(
        "chaoscontrol-guest: coverage initialized (kcov={})",
        if kcov_ok { "active" } else { "unavailable" }
    );

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
        assert::always(choice < NUM_CHOICES, "random choice in range", &json!({}));

        // ── Liveness: eventually see each choice value ──────────
        assert::sometimes(choice == 0, "saw choice 0", &json!({}));
        assert::sometimes(choice == 1, "saw choice 1", &json!({}));
        assert::sometimes(choice == 2, "saw choice 2", &json!({}));
        assert::sometimes(choice == 3, "saw choice 3", &json!({}));

        // ── Path-specific coverage + reachability ───────────────
        match choice {
            0 => {
                assert::reachable("path A", &json!({}));
                coverage::record_edge(10_000);
            }
            1 => {
                assert::reachable("path B", &json!({}));
                coverage::record_edge(20_000);
            }
            2 => {
                assert::reachable("path C", &json!({}));
                coverage::record_edge(30_000);
            }
            3 => {
                assert::reachable("path D", &json!({}));
                coverage::record_edge(40_000);
            }
            _ => {
                assert::unreachable("impossible choice value", &json!({}));
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
    assert::sometimes(true, "workload completed", &json!({}));

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
