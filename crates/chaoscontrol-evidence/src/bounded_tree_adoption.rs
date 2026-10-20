//! Bounded Tree adoption admission, parity, rollback, and evidence boundaries.

// r[impl chaoscontrol.bounded_tree_adoption.prerequisite]
pub fn admit_bounded_tree_pin(
    manifest: &str,
    lock: &str,
    flake: &str,
    producer_archive: &str,
) -> Result<(), String> {
    let revision = "b0fd0103bc9eed2c1b6d852045959462d105d8f1";
    for line in manifest
        .lines()
        .filter(|line| line.contains("bounded-tree-"))
    {
        if line.contains("path =") {
            return Err("bounded-tree uses a sibling path".to_string());
        }
        if line.contains("branch =") || line.contains("tag =") {
            return Err("bounded-tree uses a moving ref".to_string());
        }
        if !line.contains(&format!("rev = \"{revision}\"")) {
            return Err("bounded-tree revision mismatch".to_string());
        }
    }
    if !lock.contains(revision) {
        return Err("Cargo.lock does not bind the admitted revision".to_string());
    }
    if !flake.trim().is_empty() && !flake.contains(revision) {
        return Err("flake.nix asserts a different revision".to_string());
    }
    if producer_archive.trim().is_empty() {
        return Err("producer completion archive is missing".to_string());
    }
    Ok(())
}

// r[impl chaoscontrol.bounded_tree_adoption.parity]
pub fn parity_cutover_ready(positive_parity: bool, negative_parity: bool) -> Result<(), String> {
    if !positive_parity {
        return Err("archive byte parity differs".to_string());
    }
    if !negative_parity {
        return Err("negative failure-class parity differs".to_string());
    }
    Ok(())
}

// r[impl chaoscontrol.bounded_tree_adoption.rollback]
pub fn validate_coupled_rollback(
    source_revision: &str,
    dependency_revision: &str,
    pre_adoption_revision: &str,
) -> Result<(), String> {
    if source_revision.is_empty() || dependency_revision.is_empty() {
        return Err("rollback must restore source and dependency state together".to_string());
    }
    if source_revision != dependency_revision || source_revision != pre_adoption_revision {
        return Err("rollback revisions disagree".to_string());
    }
    Ok(())
}

// r[impl chaoscontrol.bounded_tree_adoption.evidence_boundary]
pub fn validate_observation_evidence(role: &str, claims: &[String]) -> Result<(), String> {
    if role != "bounded-tree/source-observation" {
        return Err(format!("unexpected evidence role: {role}"));
    }
    for claim in claims {
        if matches!(
            claim.as_str(),
            "guest_boot" | "boot_correctness" | "deterministic_replay" | "guest_correctness"
        ) {
            return Err(format!("observation promoted to {claim}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify chaoscontrol.bounded_tree_adoption.prerequisite]
    #[test]
    fn current_pin_is_immutable_and_complete_and_mutations_fail() {
        let manifest = include_str!("../Cargo.toml");
        let lock = include_str!("../../../Cargo.lock");
        let flake = include_str!("../../../flake.nix");
        let archive = "2026-08-07-adopt-bounded-tree";
        admit_bounded_tree_pin(manifest, lock, "", archive).expect("current pin");
        assert!(admit_bounded_tree_pin(
            "bounded-tree-core = { path = \"../bounded-tree\" }",
            lock,
            flake,
            archive
        )
        .is_err());
        assert!(admit_bounded_tree_pin(
            "bounded-tree-core = { git = \"x\", branch = \"main\" }",
            lock,
            flake,
            archive
        )
        .is_err());
        assert!(admit_bounded_tree_pin(manifest, "rev = \"other\"", flake, archive).is_err());
        assert!(admit_bounded_tree_pin(manifest, lock, flake, "").is_err());
    }

    // r[verify chaoscontrol.bounded_tree_adoption.parity]
    #[test]
    fn cutover_requires_positive_and_negative_parity() {
        parity_cutover_ready(true, true).expect("complete parity");
        assert!(parity_cutover_ready(false, true).is_err());
        assert!(parity_cutover_ready(true, false).is_err());
    }

    // r[verify chaoscontrol.bounded_tree_adoption.rollback]
    #[test]
    fn rollback_requires_source_and_dependency_together() {
        validate_coupled_rollback("abc", "abc", "abc").expect("coupled rollback");
        assert!(validate_coupled_rollback("abc", "", "abc").is_err());
        assert!(validate_coupled_rollback("abc", "def", "abc").is_err());
    }

    // r[verify chaoscontrol.bounded_tree_adoption.evidence_boundary]
    #[test]
    fn observation_evidence_cannot_claim_guest_behavior() {
        validate_observation_evidence(
            "bounded-tree/source-observation",
            &["dependency_revision".to_string()],
        )
        .expect("bounded observation");
        assert!(validate_observation_evidence(
            "bounded-tree/source-observation",
            &["guest_boot".to_string()]
        )
        .is_err());
        assert!(validate_observation_evidence("kernel-bundle/boot", &[]).is_err());
    }
}
