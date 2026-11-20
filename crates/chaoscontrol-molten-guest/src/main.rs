//! Runs Aspen's collaboration-native guest driver as an observed ChaosControl workload.
//!
//! The driver owns the three signed node processes and their real Raft/effect flow. This
//! wrapper only mounts the guest runtime, selects the independently provisioned source trust,
//! and records the driver outcome as SDK lifecycle and assertion evidence.

use chaoscontrol_sdk::prelude::*;
use serde_json::json;

const WORKLOAD: &str = "molten-collaboration-native";
const DEFAULT_DRIVER: &str = "/usr/bin/molten-collaborationnative";
const DEFAULT_TRUST: &str = "/etc/molten/source-gate-trust.json";
const DRIVER_CASE: &str =
    "driver::installed_native_effect_commits_through_raft_and_returns_a_completion";

fn cmdline() -> String {
    std::fs::read_to_string("/proc/cmdline").unwrap_or_default()
}

fn selected_value(cmdline: &str, name: &str, fallback: &str) -> String {
    cmdline
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{name}=")).map(str::to_owned))
        .unwrap_or_else(|| fallback.to_owned())
}

struct DriverOutcome {
    status: std::process::ExitStatus,
    output: String,
    selected_case_passed: bool,
}

fn run_driver(driver: &str, trust: &str, workspace: &str) -> std::io::Result<DriverOutcome> {
    std::fs::create_dir_all(workspace)?;
    let output = std::process::Command::new(driver)
        .args(["--ignored", "--exact", DRIVER_CASE, "--nocapture"])
        .env("MOLTEN_TEST_SOURCE_TRUST", trust)
        .current_dir(workspace)
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let selected_case_passed = output.status.success() && text.contains("1 passed");
    Ok(DriverOutcome {
        status: output.status,
        output: text,
        selected_case_passed,
    })
}

fn main() {
    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        guest_init();
    }

    let workload = WorkloadHarness::new(WORKLOAD);
    workload.init();
    let driver = std::env::var("MOLTEN_GUEST_DRIVER").unwrap_or_else(|_| DEFAULT_DRIVER.to_owned());
    let cmdline = cmdline();
    let trust = std::env::var("MOLTEN_GUEST_SOURCE_TRUST")
        .unwrap_or_else(|_| selected_value(&cmdline, "molten_source_trust", DEFAULT_TRUST));
    let workspace = std::env::var("MOLTEN_GUEST_WORKSPACE")
        .unwrap_or_else(|_| selected_value(&cmdline, "molten_guest_workspace", "/data/molten"));
    workload.setup_complete(json!({
        "nodes": 3,
        "packaging": "chaoscontrol-initrd",
        "driver": driver,
        "source_trust": trust,
        "evidence_root": workspace,
        "evidence_class": "bounded-vm-campaign-with-external-signed-source-trust",
    }));

    let outcome = workload.scenario("real collaboration-native three-node flow", || {
        run_driver(&driver, &trust, &workspace)
    });
    match outcome {
        Ok(outcome) => {
            println!("{}", outcome.output);
            send_event(
                "molten_driver_done",
                &json!({
                    "workload": WORKLOAD,
                    "success": outcome.selected_case_passed,
                    "exit_code": outcome.status.code(),
                }),
            );
            cc_assert_always_category!(
                WORKLOAD,
                "completion",
                outcome.selected_case_passed,
                "the admitted Molten driver completed its selected case successfully"
            );
        }
        Err(error) => {
            cc_assert_unreachable!(
                "the admitted Molten driver could not start",
                &json!({"workload": WORKLOAD, "error": error.to_string()})
            );
        }
    }

    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        loop {
            unsafe { libc::pause() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmdline_values_default_to_the_packaged_paths() {
        let cmdline =
            "console=ttyS0 molten_source_trust=/trust.json molten_guest_workspace=/data/run";
        assert_eq!(
            selected_value(cmdline, "molten_source_trust", DEFAULT_TRUST),
            "/trust.json"
        );
        assert_eq!(
            selected_value(cmdline, "molten_guest_workspace", "/data/molten"),
            "/data/run"
        );
        assert_eq!(
            selected_value("console=ttyS0", "molten_source_trust", DEFAULT_TRUST),
            DEFAULT_TRUST
        );
    }
}
