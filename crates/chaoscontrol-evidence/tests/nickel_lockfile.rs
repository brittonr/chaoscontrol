//! Nickel toolchain lockfile provenance check.

// r[impl chaoscontrol.nickel_toolchain.lockfile]
pub fn lockfile_binds_declared_nickel_input(flake: &str, lock: &str) -> bool {
    const REV: &str = "1320a983e6c3d1e2fb53dd2464b084b4903b1426";
    flake.contains(&format!("github:nickel-lang/nickel/{REV}"))
        && lock.matches(REV).count() >= 2
        && lock.contains("\"type\": \"github\"")
}

// r[verify chaoscontrol.nickel_toolchain.lockfile]
// r[verify chaoscontrol.nickel_toolchain.lockfile.scenario.generated]
#[test]
fn declared_input_and_generated_lock_agree_and_mutation_fails() {
    let flake = include_str!("../../../flake.nix");
    let lock = include_str!("../../../flake.lock");
    assert!(lockfile_binds_declared_nickel_input(flake, lock));
    assert!(!lockfile_binds_declared_nickel_input(
        flake,
        &lock.replace("1320a983e6c3d1e2fb53dd2464b084b4903b1426", "deadbeef")
    ));
}
