//! Campaign policy cutover admission: baseline, source pins, ownership, KVM
//! parity, and receipt-conditioned selection.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const CAMPAIGN_REVISION: &str = "796fe3844c91c3060da5ffc573a6f801a214aba7";
pub const CHOREGRAPH_HISTORY_REVISION: &str = "b3e08e19750f53bdbcae970cdf58a47a791ed20b";

// r[impl chaoscontrol.campaign_policy.baseline]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaselineReceipt {
    pub source_revision: String,
    pub commands: Vec<String>,
    pub positive_results: usize,
    pub negative_results: usize,
}

// r[impl chaoscontrol.campaign_policy.baseline.valid]
// r[impl chaoscontrol.campaign_policy.baseline.missing]
pub fn admit_baseline(
    receipt: Option<&BaselineReceipt>,
    current_revision: &str,
) -> Result<(), String> {
    let receipt = receipt.ok_or("focused adaptive-policy baseline is missing")?;
    if receipt.source_revision != current_revision {
        return Err("baseline binds another source revision".to_string());
    }
    if receipt.commands.is_empty() {
        return Err("baseline records no commands".to_string());
    }
    if receipt.positive_results == 0 || receipt.negative_results == 0 {
        return Err("baseline lacks positive or negative results".to_string());
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.source_pins]
// r[impl chaoscontrol.campaign_policy.source_pins.valid]
// r[impl chaoscontrol.campaign_policy.source_pins.invalid]
pub fn admit_source_pins(manifest: &str, lock: &str, flake: &str) -> Result<(), String> {
    for (name, revision) in [
        ("campaign-core", CAMPAIGN_REVISION),
        ("choregraph-history", CHOREGRAPH_HISTORY_REVISION),
    ] {
        let line = manifest
            .lines()
            .find(|line| line.starts_with(name))
            .ok_or_else(|| format!("{name} declaration is missing"))?;
        if line.contains("path =") || line.contains("branch =") || line.contains("tag =") {
            return Err(format!("{name} uses a moving or sibling source"));
        }
        if !line.contains(revision) {
            return Err(format!("{name} revision differs from the reviewed pin"));
        }
        if !lock.contains(revision) || (!flake.trim().is_empty() && !flake.contains(revision)) {
            return Err(format!(
                "{name} is not bound across Cargo.lock and flake.nix"
            ));
        }
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.boundary.seed_runner]
// r[impl chaoscontrol.campaign_policy.boundary.invalid]
// r[impl chaoscontrol.campaign_policy.effects.invalid]
// r[impl chaoscontrol.campaign_policy.product_authority.invalid]
// r[impl chaoscontrol.campaign_policy.progress.invalid]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SharedApiSurface {
    pub name: String,
    pub ownership: String,
    pub product_types: Vec<String>,
    pub effects: Vec<String>,
}

pub fn admit_shared_api(surfaces: &[SharedApiSurface]) -> Result<(), String> {
    for surface in surfaces {
        if !matches!(
            surface.ownership.as_str(),
            "frontier" | "ranking" | "selection" | "budget" | "pruning" | "stop"
        ) {
            return Err(format!("{} has out-of-bound ownership", surface.name));
        }
        if !surface.product_types.is_empty() {
            return Err(format!(
                "{} requires product types {}",
                surface.name,
                surface.product_types.join(",")
            ));
        }
        if !surface.effects.is_empty() {
            return Err(format!(
                "{} executes effects {}",
                surface.name,
                surface.effects.join(",")
            ));
        }
        if surface.name.contains("seed")
            || surface.name.contains("progress")
            || surface.name.contains("checkpoint")
        {
            return Err(format!(
                "{} moves product lifecycle into Campaign",
                surface.name
            ));
        }
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.kvm_parity]
// r[impl chaoscontrol.campaign_policy.kvm_parity.valid]
// r[impl chaoscontrol.campaign_policy.kvm_parity.missing]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KvmParityReceipt {
    pub source_revision: String,
    pub policy: String,
    pub seed: u64,
    pub legacy_digest: String,
    pub campaign_digest: String,
    pub passed: bool,
}

pub fn admit_kvm_parity(
    receipt: Option<&KvmParityReceipt>,
    current_revision: &str,
) -> Result<(), String> {
    let receipt = receipt.ok_or("adaptive KVM parity receipt is missing")?;
    if receipt.source_revision != current_revision {
        return Err("KVM parity receipt is stale".to_string());
    }
    if receipt.policy != "campaign" || receipt.seed != 42 {
        return Err("KVM parity receipt binds another policy or seed".to_string());
    }
    if receipt.legacy_digest.is_empty() || receipt.campaign_digest.is_empty() {
        return Err("KVM parity receipt lacks both run identities".to_string());
    }
    if receipt.legacy_digest != receipt.campaign_digest {
        return Err("KVM parity decision fields drifted".to_string());
    }
    if !receipt.passed {
        return Err("KVM parity receipt did not pass".to_string());
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.cutover]
// r[impl chaoscontrol.campaign_policy.cutover.invalid]
pub struct CutoverFacts {
    pub baseline: bool,
    pub source_pins: bool,
    pub conformance: bool,
    pub parity: bool,
    pub kvm_parity: bool,
    pub architecture: bool,
}

pub fn admit_cutover(facts: &CutoverFacts) -> Result<(), String> {
    for (name, passed) in [
        ("baseline", facts.baseline),
        ("source pins", facts.source_pins),
        ("conformance", facts.conformance),
        ("model parity", facts.parity),
        ("KVM parity", facts.kvm_parity),
        ("architecture ownership", facts.architecture),
    ] {
        if !passed {
            return Err(format!("Campaign cutover blocked by {name}"));
        }
    }
    Ok(())
}

fn read_policy_source(path: &Path, label: &str) -> Result<String, String> {
    crate::bounded_file::read_bounded_regular_file(path, crate::MAX_EVIDENCE_JSON_BYTES)
        .map_err(|error| format!("{label}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_source_reads_reject_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.toml");
        std::fs::write(&target, b"[package]\nname = \"x\"").unwrap();
        let link = root.path().join("Cargo.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_policy_source(&link, "manifest").is_err());
        assert!(read_policy_source(&target, "manifest").is_ok());
    }

    // r[verify chaoscontrol.campaign_policy.baseline]
    #[test]
    fn baseline_requires_current_revision_commands_and_both_result_classes() {
        let revision = "abc";
        let receipt = BaselineReceipt {
            source_revision: revision.to_string(),
            commands: vec!["focused baseline".to_string()],
            positive_results: 1,
            negative_results: 1,
        };
        admit_baseline(Some(&receipt), revision).expect("complete baseline");
        assert!(admit_baseline(None, revision).is_err());
        let mut stale = receipt.clone();
        stale.source_revision = "other".to_string();
        assert!(admit_baseline(Some(&stale), revision).is_err());
        let mut incomplete = receipt;
        incomplete.negative_results = 0;
        assert!(admit_baseline(Some(&incomplete), revision).is_err());
    }

    // r[verify chaoscontrol.campaign_policy.source_pins]
    #[test]
    fn source_pins_require_matching_immutable_cargo_and_nix_revisions() {
        let manifest = format!(
            "campaign-core = {{ git = \"x\", rev = \"{CAMPAIGN_REVISION}\" }}\nchoregraph-history = {{ git = \"y\", rev = \"{CHOREGRAPH_HISTORY_REVISION}\" }}\n"
        );
        let lock = format!("{CAMPAIGN_REVISION}{CHOREGRAPH_HISTORY_REVISION}");
        let flake = lock.clone();
        admit_source_pins(&manifest, &lock, &flake).expect("matching pins");
        assert!(admit_source_pins(
            &manifest.replace("git = \"x\"", "path = \"../campaign\""),
            &lock,
            &flake
        )
        .is_err());
        assert!(admit_source_pins(
            &manifest,
            &lock.replace(CAMPAIGN_REVISION, "deadbeef"),
            &flake
        )
        .is_err());
    }

    // r[verify chaoscontrol.campaign_policy.boundary.invalid]
    // r[verify chaoscontrol.campaign_policy.effects.invalid]
    // r[verify chaoscontrol.campaign_policy.product_authority.invalid]
    // r[verify chaoscontrol.campaign_policy.progress.invalid]
    #[test]
    fn shared_api_rejects_product_types_effects_and_lifecycle_ownership() {
        let allowed = SharedApiSurface {
            name: "frontier".to_string(),
            ownership: "frontier".to_string(),
            product_types: Vec::new(),
            effects: Vec::new(),
        };
        admit_shared_api(std::slice::from_ref(&allowed)).expect("bounded frontier API");
        let mut product = allowed.clone();
        product.product_types = vec!["Snapshot".to_string()];
        assert!(admit_shared_api(&[product]).is_err());
        let mut effect = allowed.clone();
        effect.effects = vec!["write_receipt".to_string()];
        assert!(admit_shared_api(&[effect]).is_err());
        let mut seed = allowed.clone();
        seed.name = "seed_dispatch".to_string();
        assert!(admit_shared_api(&[seed]).is_err());
        let mut progress = allowed.clone();
        progress.name = "progress_checkpoint".to_string();
        assert!(admit_shared_api(&[progress]).is_err());
        let mut ownership = allowed;
        ownership.ownership = "vm_lifecycle".to_string();
        assert!(admit_shared_api(&[ownership]).is_err());
    }

    // r[verify chaoscontrol.campaign_policy.kvm_parity]
    #[test]
    fn kvm_parity_requires_current_matching_passing_receipt() {
        let revision = "abc";
        let receipt = KvmParityReceipt {
            source_revision: revision.to_string(),
            policy: "campaign".to_string(),
            seed: 42,
            legacy_digest: "same".to_string(),
            campaign_digest: "same".to_string(),
            passed: true,
        };
        admit_kvm_parity(Some(&receipt), revision).expect("fresh parity");
        assert!(admit_kvm_parity(None, revision).is_err());
        let mut stale = receipt.clone();
        stale.source_revision = "other".to_string();
        assert!(admit_kvm_parity(Some(&stale), revision).is_err());
        let mut drift = receipt;
        drift.campaign_digest = "different".to_string();
        assert!(admit_kvm_parity(Some(&drift), revision).is_err());
    }

    // r[verify chaoscontrol.campaign_policy.cutover]
    #[test]
    fn cutover_requires_every_named_gate() {
        let all = CutoverFacts {
            baseline: true,
            source_pins: true,
            conformance: true,
            parity: true,
            kvm_parity: true,
            architecture: true,
        };
        admit_cutover(&all).expect("complete cutover");
        for field in [
            "baseline",
            "source_pins",
            "conformance",
            "parity",
            "kvm_parity",
            "architecture",
        ] {
            let mut facts = CutoverFacts {
                baseline: true,
                source_pins: true,
                conformance: true,
                parity: true,
                kvm_parity: true,
                architecture: true,
            };
            match field {
                "baseline" => facts.baseline = false,
                "source_pins" => facts.source_pins = false,
                "conformance" => facts.conformance = false,
                "parity" => facts.parity = false,
                "kvm_parity" => facts.kvm_parity = false,
                "architecture" => facts.architecture = false,
                _ => unreachable!(),
            }
            assert!(admit_cutover(&facts).is_err(), "{field}");
        }
    }
}

/// Committed cutover receipt. The shell loads this file and admits it before
/// selecting the Campaign policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CutoverReceipt {
    pub source_revision: String,
    pub baseline: BaselineReceipt,
    pub kvm_parity: KvmParityReceipt,
    pub conformance: bool,
    pub parity: bool,
    pub architecture: Vec<SharedApiSurface>,
}

// r[impl chaoscontrol.campaign_policy.cutover]
pub fn admit_cutover_receipt(
    root: &Path,
    receipt: &CutoverReceipt,
    current_revision: &str,
) -> Result<(), String> {
    let manifest = read_policy_source(
        &root.join("crates/chaoscontrol-explore/Cargo.toml"),
        "campaign manifest",
    )?;
    let lock = read_policy_source(&root.join("Cargo.lock"), "Cargo.lock")?;
    // Packaged Nix sources intentionally omit flake.nix; the committed
    // receipt and Cargo lock are the authority there.
    let flake = read_policy_source(&root.join("flake.nix"), "flake.nix").unwrap_or_default();
    admit_source_pins(&manifest, &lock, &flake)?;
    admit_baseline(Some(&receipt.baseline), current_revision)?;
    admit_kvm_parity(Some(&receipt.kvm_parity), current_revision)?;
    admit_shared_api(&receipt.architecture)?;
    admit_cutover(&CutoverFacts {
        baseline: true,
        source_pins: true,
        conformance: receipt.conformance,
        parity: receipt.parity,
        kvm_parity: true,
        architecture: true,
    })
}
