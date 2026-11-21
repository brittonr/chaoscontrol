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
const DRIVER_CASE: &str = "driver::collaboration_native_baseline";

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
    selected_case_passed: bool,
}

/// Mount the signed output store from the attached read-only disk when it is not
/// already present in the initrd.
fn mount_signed_store(trust: &str) {
    if let Ok(bytes) = std::fs::read(trust) {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(output) = value.get("output").and_then(|value| value.as_str()) {
                if std::path::Path::new(output).exists() {
                    return;
                }
            }
        }
    }
    if std::path::Path::new("/nix/store/nix").exists() {
        return;
    }
    std::fs::create_dir_all("/nix/store").ok();
    for attempt in 0..50 {
        let rc = unsafe {
            libc::mount(
                c"/dev/vda".as_ptr(),
                c"/nix/store".as_ptr(),
                c"ext4".as_ptr(),
                libc::MS_RDONLY,
                std::ptr::null(),
            )
        };
        if rc == 0 {
            eprintln!("molten-guest: mounted /dev/vda at /nix/store");
            return;
        }
        if attempt == 49 {
            eprintln!("molten-guest: failed to mount /dev/vda at /nix/store");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Start the guest-side Nix daemon that Aspen's source admission requires.
///
/// The signed output carries store bytes and a database dump, not a daemon. The
/// dump is loaded once into the initramfs state directory and the daemon serves
/// read-only path, hash, and signature queries from the mounted store.
fn start_nix_daemon(trust: &str) -> Option<std::process::Child> {
    let bytes = std::fs::read(trust).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let nix = value.get("nix")?.as_str()?;
    let nix_store = std::path::Path::new(nix).parent()?.join("nix-store");
    let nix_daemon = std::path::Path::new(nix).parent()?.join("nix-daemon");
    for directory in [
        "/nix/var/nix/db",
        "/nix/var/nix/profiles",
        "/nix/var/nix/gcroots",
        "/nix/var/nix/temproots",
        "/nix/var/nix/daemon-socket",
    ] {
        std::fs::create_dir_all(directory).ok()?;
    }
    if std::path::Path::new("/etc/molten/nix-db.dump").exists() {
        let dump = std::fs::File::open("/etc/molten/nix-db.dump").ok()?;
        let loaded = std::process::Command::new(nix_store)
            .arg("--load-db")
            .stdin(std::process::Stdio::from(dump))
            .status()
            .ok()?;
        if !loaded.success() {
            return None;
        }
    }
    let child = std::process::Command::new(nix_daemon)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    for _ in 0..500 {
        if std::path::Path::new("/nix/var/nix/daemon-socket/socket").exists() {
            return Some(child);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    None
}

fn run_driver(driver: &str, trust: &str, workspace: &str) -> std::io::Result<DriverOutcome> {
    std::fs::create_dir_all(workspace)?;
    let listed = std::process::Command::new(driver).arg("--list").output()?;
    let list = String::from_utf8_lossy(&listed.stdout);
    if !listed.status.success()
        || !list
            .lines()
            .any(|line| line == format!("{DRIVER_CASE}: test"))
    {
        return Ok(DriverOutcome {
            status: listed.status,
            selected_case_passed: false,
        });
    }
    let mut command = std::process::Command::new(driver);
    command
        .args(["--ignored", "--exact", DRIVER_CASE, "--nocapture"])
        .env("MOLTEN_TEST_SOURCE_TRUST", trust)
        .current_dir(workspace);
    if std::path::Path::new("/etc/molten/source-closure-validation.json").exists() {
        command.env(
            "MOLTEN_TEST_CLOSURE_CACHE",
            "/etc/molten/source-closure-validation.json",
        );
    }
    let status = command.status()?;
    Ok(DriverOutcome {
        selected_case_passed: status.success(),
        status,
    })
}

#[repr(C)]
struct IfReq {
    name: [u8; 16],
    flags: libc::c_short,
    pad: [u8; 22],
}

/// Bring up the loopback interface. The collaboration driver starts three
/// processes that exchange over 127.0.0.1, but the guest starts with `lo` down.
fn bring_up_loopback() {
    const SIOCGIFFLAGS: libc::c_ulong = 0x8913;
    const SIOCSIFFLAGS: libc::c_ulong = 0x8914;
    const IFF_UP: libc::c_short = 0x1;
    const IFF_RUNNING: libc::c_short = 0x40;
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        eprintln!("molten-guest: loopback socket failed");
        return;
    }
    let mut request = IfReq {
        name: [0; 16],
        flags: 0,
        pad: [0; 22],
    };
    request.name[..2].copy_from_slice(b"lo");
    if unsafe { libc::ioctl(fd, SIOCGIFFLAGS as libc::Ioctl, &mut request) } != 0 {
        eprintln!("molten-guest: loopback flags read failed");
        unsafe { libc::close(fd) };
        return;
    }
    let before = request.flags;
    request.flags |= IFF_UP | IFF_RUNNING;
    let result = unsafe { libc::ioctl(fd, SIOCSIFFLAGS as libc::Ioctl, &request) };
    eprintln!(
        "molten-guest: loopback flags {before:#x} -> {:#x} result={result}",
        request.flags
    );
    unsafe { libc::close(fd) };
}

fn main() {
    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        guest_init();
    }

    bring_up_loopback();
    std::env::set_var("HOME", "/root");
    std::env::set_var("USER", "root");
    std::env::set_var("LOGNAME", "root");
    std::fs::create_dir_all("/root").ok();
    let cmdline = cmdline();
    let trust = std::env::var("MOLTEN_GUEST_SOURCE_TRUST")
        .unwrap_or_else(|_| selected_value(&cmdline, "molten_source_trust", DEFAULT_TRUST));
    let workspace = std::env::var("MOLTEN_GUEST_WORKSPACE")
        .unwrap_or_else(|_| selected_value(&cmdline, "molten_guest_workspace", "/data/molten"));
    mount_signed_store(&trust);
    let _nix_daemon = start_nix_daemon(&trust);
    let workload = WorkloadHarness::new(WORKLOAD);
    workload.init();
    let driver = std::env::var("MOLTEN_GUEST_DRIVER").unwrap_or_else(|_| DEFAULT_DRIVER.to_owned());
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
