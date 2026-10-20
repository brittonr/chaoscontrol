//! Pure source regression checks for Rust-owned automation.

// r[impl chaoscontrol.rust_automation.removal]
// r[impl chaoscontrol.rust_automation.validation]

// r[impl chaoscontrol.rust_automation.inventory]
// r[impl chaoscontrol.rust_automation.boundary]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    pub surface: String,
    pub owner: String,
    pub inputs: String,
    pub outputs: String,
    pub exit_classes: String,
    pub bounds: String,
    pub effects: String,
    pub callers: Vec<String>,
    pub positive_fixture: String,
    pub negative_fixture: String,
}

/// Admit a Python-surface inventory only when every required fact is present
/// and every observed caller is recorded. A hidden caller keeps removal
/// blocked.
pub fn admit_inventory(
    entries: &[InventoryEntry],
    observed_callers: &[String],
) -> Result<(), String> {
    let mut surfaces = std::collections::BTreeSet::new();
    let mut recorded_callers = std::collections::BTreeSet::new();
    for entry in entries {
        if !surfaces.insert(entry.surface.as_str()) {
            return Err(format!("duplicate inventory surface: {}", entry.surface));
        }
        for (label, value) in [
            ("owner", &entry.owner),
            ("inputs", &entry.inputs),
            ("outputs", &entry.outputs),
            ("exit_classes", &entry.exit_classes),
            ("bounds", &entry.bounds),
            ("effects", &entry.effects),
            ("positive_fixture", &entry.positive_fixture),
            ("negative_fixture", &entry.negative_fixture),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{} has no {label}", entry.surface));
            }
        }
        if entry.callers.is_empty() {
            return Err(format!("{} has no recorded callers", entry.surface));
        }
        recorded_callers.extend(entry.callers.iter().cloned());
    }
    for caller in observed_callers {
        if !recorded_callers.contains(caller) {
            return Err(format!("hidden caller remains: {caller}"));
        }
    }
    Ok(())
}

/// Reject a migration report that promotes Rust ownership into a correctness,
/// audit, KVM, sandboxing, or release-eligibility claim.
pub fn validate_claim(report: &str) -> Result<(), String> {
    let lower = report.to_ascii_lowercase();
    for phrase in [
        "workflows are correct",
        "orchestration correctness",
        "complete audit",
        "sandbox effectiveness",
        "release eligible because",
    ] {
        if lower.contains(phrase) {
            return Err(format!("unsupported automation claim: {phrase}"));
        }
    }
    Ok(())
}

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
    use super::{admit_inventory, validate, validate_claim, InventoryEntry, REQUIRED_BINARIES};

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

    // r[verify chaoscontrol.rust_automation.inventory]
    // r[verify chaoscontrol.rust_automation.boundary]
    #[test]
    fn inventory_admission_requires_every_fact_and_hidden_callers_block() {
        let entry = InventoryEntry {
            surface: "scripts/old.py".to_string(),
            owner: "rust-owner".to_string(),
            inputs: "arg path".to_string(),
            outputs: "json".to_string(),
            exit_classes: "0,1".to_string(),
            bounds: "1024 bytes".to_string(),
            effects: "none".to_string(),
            callers: vec!["flake.nix#app".to_string()],
            positive_fixture: "fixtures/positive".to_string(),
            negative_fixture: "fixtures/negative".to_string(),
        };
        admit_inventory(
            std::slice::from_ref(&entry),
            &[String::from("flake.nix#app")],
        )
        .expect("complete inventory");
        let mut missing = entry.clone();
        missing.bounds.clear();
        assert!(admit_inventory(&[missing], &[]).is_err());
        assert!(admit_inventory(&[entry], &[String::from("flake.nix#hidden")]).is_err());
        validate_claim("migration complete; workflows are correct").unwrap_err();
        validate_claim("migration complete; see non-claims").expect("bounded report");
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
