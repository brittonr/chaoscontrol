//! Downstream-style Rust workload packaged as a ChaosControl guest.
//!
//! This binary intentionally uses the public workload harness surface so the
//! Nix rail exercises the same shape a downstream Rust project should copy.

use chaoscontrol_sdk::{assert, prelude::*};
use serde_json::json;

const WORKLOAD: &str = "sample-rust-service";
const ITERATIONS: usize = 12;
const RUST_WORKLOAD_SNAPSHOT_REPLAY_PROBE_ASSERTION_ID: u32 = 1_414_213_562;

fn cmdline_value_from(cmdline: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    cmdline
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&prefix).map(str::to_owned))
}

fn snapshot_probe_enabled_from(cmdline: &str) -> bool {
    matches!(
        cmdline_value_from(cmdline, "rust_workload_bug").as_deref(),
        Some("snapshot_replay_probe") | Some("snapshot_probe")
    )
}

fn snapshot_probe_enabled() -> bool {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    snapshot_probe_enabled_from(&cmdline)
}

fn snapshot_probe_fail_after_from(cmdline: &str) -> usize {
    cmdline_value_from(cmdline, "rust_workload_snapshot_probe_fail_after")
        .and_then(|value| value.parse().ok())
        .unwrap_or(25)
}

fn snapshot_probe_fail_after() -> usize {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    snapshot_probe_fail_after_from(&cmdline)
}

fn main() {
    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        guest_init();
    }

    let workload = WorkloadHarness::new(WORKLOAD);
    let snapshot_probe = snapshot_probe_enabled();
    let snapshot_probe_fail_after = snapshot_probe_fail_after();
    workload.init();
    workload.setup_complete(json!({
        "nodes": 3,
        "packaging": "nix-initrd",
        "evidence_class": "instrumentation-or-vm-campaign",
        "snapshot_probe": snapshot_probe,
        "snapshot_probe_fail_after": snapshot_probe_fail_after,
    }));

    let mut writes = 0usize;
    let mut reads = 0usize;

    for iteration in 0..ITERATIONS {
        workload.scenario("writes survive failover", || {
            let action = random_choice(3);
            cc_assert_always_category!(
                WORKLOAD,
                "invariant",
                action < 3,
                "choice remains in range"
            );

            if action == 0 {
                writes += 1;
                cc_assert_sometimes_category!(WORKLOAD, "operation", true, "write succeeds");
            }
            if action == 1 {
                reads += 1;
                cc_assert_reachable_category!(WORKLOAD, "branch", "read branch exercised");
            }

            cc_assert_always_category!(
                WORKLOAD,
                "invariant",
                writes + reads <= iteration + 1,
                "operation counters stay bounded"
            );
        });
    }

    cc_assert_sometimes_category!(
        WORKLOAD,
        "operation",
        writes > 0,
        "at least one write succeeds"
    );
    send_event(
        "workload_done",
        &json!({
            "workload": WORKLOAD,
            "iterations": ITERATIONS,
            "writes": writes,
            "reads": reads,
            "evidence_class": "instrumentation-only unless run under VM campaign",
        }),
    );

    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        if !snapshot_probe {
            loop {
                unsafe { libc::pause() };
            }
        }

        let mut probe_iter = ITERATIONS;
        loop {
            probe_iter += 1;
            let jitter = random_choice(4);
            assert::always_with_id(
                probe_iter < snapshot_probe_fail_after,
                RUST_WORKLOAD_SNAPSHOT_REPLAY_PROBE_ASSERTION_ID,
                "rust workload snapshot replay probe trips only after restored parent context",
                &json!({
                    "iteration": probe_iter,
                    "jitter": jitter,
                    "fail_after": snapshot_probe_fail_after,
                    "workload": WORKLOAD,
                }),
            );
            if jitter == 0 {
                send_event(
                    "snapshot_probe_tick",
                    &json!({"iteration": probe_iter, "workload": WORKLOAD}),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_snapshot_probe_cmdline_values() {
        let cmdline = "console=ttyS0 rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=17";
        assert_eq!(
            cmdline_value_from(cmdline, "rust_workload_bug"),
            Some("snapshot_replay_probe".to_string())
        );
        assert_eq!(
            cmdline_value_from(cmdline, "rust_workload_snapshot_probe_fail_after"),
            Some("17".to_string())
        );
        assert_eq!(cmdline_value_from(cmdline, "missing"), None);
    }

    #[test]
    fn snapshot_probe_is_opt_in_and_defaults_fail_after() {
        assert!(!snapshot_probe_enabled_from("console=ttyS0"));
        assert!(!snapshot_probe_enabled_from("rust_workload_bug=other"));
        assert!(snapshot_probe_enabled_from(
            "rust_workload_bug=snapshot_replay_probe"
        ));
        assert!(snapshot_probe_enabled_from(
            "rust_workload_bug=snapshot_probe"
        ));
        assert_eq!(snapshot_probe_fail_after_from("console=ttyS0"), 25);
        assert_eq!(
            snapshot_probe_fail_after_from("rust_workload_snapshot_probe_fail_after=31"),
            31
        );
        assert_eq!(
            snapshot_probe_fail_after_from("rust_workload_snapshot_probe_fail_after=bad"),
            25
        );
    }
}
