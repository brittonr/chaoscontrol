//! VM Cohort source-pin admission.

// r[impl chaoscontrol.vm_cohort.source]
pub fn source_pin_is_immutable(manifest: &str, lock: &str, flake: &str) -> bool {
    const REV: &str = "ab123e3673b6dd616b3df5d044026b5e85755149";
    let pinned = manifest
        .lines()
        .filter(|line| line.starts_with("vm-cohort-"))
        .collect::<Vec<_>>();
    pinned.len() == 3
        && pinned.iter().all(|line| {
            line.contains(REV)
                && !line.contains("branch =")
                && !line.contains("tag =")
                && !line.contains("path =")
        })
        && lock.matches(REV).count() == 6
        && flake.contains(REV)
}

// r[verify chaoscontrol.vm_cohort.source]
#[test]
fn current_manifest_lock_and_flake_pin_one_immutable_revision() {
    let manifest = include_str!("../Cargo.toml");
    let lock = include_str!("../../../Cargo.lock");
    let flake = include_str!("../../../flake.nix");
    assert!(source_pin_is_immutable(manifest, lock, flake));
    assert!(!source_pin_is_immutable(
        "vm-cohort-core = { git = \"rad://x\", branch = \"main\" }",
        lock,
        flake
    ));
    assert!(!source_pin_is_immutable(
        "vm-cohort-core = { git = \"rad://x\", path = \"../vm-cohort\" }",
        lock,
        flake
    ));
    assert!(!source_pin_is_immutable(manifest, "rev = \"other\"", flake));
}

#[test]
fn rejected_moving_ref_keeps_current_revision_visible() {
    let lock = include_str!("../../../Cargo.lock");
    let flake = include_str!("../../../flake.nix");
    let rejected = "vm-cohort-core = { git = \"rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3\", rev = \"ab123e3673b6dd616b3df5d044026b5e85755149\", branch = \"main\" }";
    assert!(!source_pin_is_immutable(rejected, lock, flake));
}
