//! Pure source regression checks for Rust-owned automation.

// r[impl chaoscontrol.rust_automation.removal]
// r[impl chaoscontrol.rust_automation.validation]

const REQUIRED_BINARIES: [&str; 9] = [
    "accepted-snapshot-verdict-dogfood.rs",
    "check-cargo-audit-report.rs",
    "check-vm-determinism-drift-receipt.rs",
    "local-multi-hypervisor-kvm-smoke.rs",
    "materialize-dogfood-receipt.rs",
    "materialize-replay-readiness-receipt.rs",
    "render-vm-determinism-matrix-summary.rs",
    "scaffold-rust-workload.rs",
    "summarize-accepted-dogfood-output.rs",
];

pub fn validate(script_paths: &[String], flake: &str, rust_bins: &[String]) -> Result<(), String> {
    let python_scripts = script_paths
        .iter()
        .filter(|path| path.ends_with(".py"))
        .cloned()
        .collect::<Vec<_>>();
    if !python_scripts.is_empty() {
        return Err(format!(
            "Python product automation scripts remain: {}",
            python_scripts.join(",")
        ));
    }
    if flake.to_ascii_lowercase().contains("python") {
        return Err(String::from("flake.nix still references Python"));
    }
    for required in REQUIRED_BINARIES {
        if !rust_bins.iter().any(|path| path.ends_with(required)) {
            return Err(format!("missing Rust automation owner: {required}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate, REQUIRED_BINARIES};

    fn bins() -> Vec<String> {
        REQUIRED_BINARIES
            .iter()
            .map(|name| format!("src/bin/{name}"))
            .collect()
    }

    #[test]
    fn complete_rust_inventory_passes() {
        validate(
            &[String::from("scripts/check.rs")],
            "rustOnly = true;",
            &bins(),
        )
        .expect("valid");
    }

    #[test]
    fn python_flake_and_missing_owner_fail() {
        assert!(validate(&[String::from("scripts/old.py")], "", &bins()).is_err());
        assert!(validate(&[], "pkgs.python3", &bins()).is_err());
        assert!(validate(&[], "", &[])
            .expect_err("missing")
            .contains("missing Rust automation owner"));
    }
}
