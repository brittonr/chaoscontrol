//! Negative controls for the actual Cargo-deny source and license policy.
//! The Nix check supplies an outer timeout and an isolated build directory.
use std::{env, fs, path::Path, process::Command};

const ADMITTED_SOURCES: &[&str] = &[
    "https://git.onix.computer/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git",
    "https://git.onix.computer/zL2ncTUeASVYwcoGkEXv9JKgGbAF.git",
    "https://seed.radicle.garden/z2pbwQ4Zd52M4oC1W9bLT3nQem9ay.git",
    "rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3",
];
const LICENSE_EXCEPTIONS: &[&str] = &["chaoscontrol-guest-determinism-probe", "choregraph-history"];
const REPLACEMENT_SOURCE: &str = "https://unadmitted.invalid/repository.git";
const REPLACEMENT_CRATE: &str = "unadmitted-license-fixture";

fn reject(config: &Path, diagnostic: &str) {
    let output = Command::new("cargo")
        .args(["--offline", "deny", "--locked", "check"])
        .arg("--config")
        .arg(config)
        .args(["bans", "licenses", "sources"])
        .output()
        .expect("start the real dependency-policy checker");
    assert!(!output.status.success(), "policy mutation was accepted");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 diagnostics");
    assert!(stderr.contains(diagnostic), "wrong denial: {stderr}");
    assert!(!stderr.contains("panicked"), "checker panicked: {stderr}");
}

fn mutation(policy: &str, old: &str, new: &str, path: &Path, diagnostic: &str) {
    assert_eq!(policy.matches(old).count(), 1, "mutation must be exact");
    let changed = policy.replacen(old, new, 1);
    fs::write(path, changed).expect("write isolated policy fixture");
    reject(path, diagnostic);
    fs::remove_file(path).expect("remove owned fixture");
}

fn main() {
    let policy = fs::read_to_string("deny.toml").expect("repository policy");
    let root = env::temp_dir().join(format!("campaign-deny-{}", std::process::id()));
    fs::create_dir(&root).expect("fresh fixture directory");
    let fixture = root.join("deny.toml");
    for source in ADMITTED_SOURCES {
        mutation(
            &policy,
            source,
            REPLACEMENT_SOURCE,
            &fixture,
            "source-not-allowed",
        );
    }
    for name in LICENSE_EXCEPTIONS {
        mutation(
            &policy,
            &format!("crate = \"{name}\""),
            &format!("crate = \"{REPLACEMENT_CRATE}\""),
            &fixture,
            "failed to satisfy license requirements",
        );
    }
    fs::remove_dir(root).expect("remove owned fixture directory");
    println!("Campaign dependency-policy negative controls passed");
}
