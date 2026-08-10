//! Pure source-direction validation for migrated architecture cores.

// r[impl chaoscontrol.architecture_modules.boundary]
// r[impl chaoscontrol.architecture_modules.validation]

/// One forbidden dependency found in a pure core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryViolation {
    pub module: String,
    pub effect_class: &'static str,
    pub token: &'static str,
}

const FORBIDDEN_EFFECTS: &[(&str, &str)] = &[
    ("filesystem", "std::fs"),
    ("environment", "std::env"),
    ("process", "std::process"),
    ("process", "Command::"),
    ("clock", "SystemTime"),
    ("clock", "Instant::"),
    ("output", "println!"),
    ("output", "eprintln!"),
    ("kvm-or-os-unsafe", "libc::"),
    ("kvm-or-os-unsafe", "kvm_ioctls"),
    ("thread", "std::thread"),
    ("thread", "thread::spawn"),
    ("shell-dependency", "replay_readiness_surfaces"),
];

/// Validate one supplied source file without reading files or inspecting state.
pub fn validate_core_source(module: &str, source: &str) -> Vec<BoundaryViolation> {
    let mut violations = Vec::new();
    for (effect_class, token) in FORBIDDEN_EFFECTS {
        if source.contains(token) {
            violations.push(BoundaryViolation {
                module: module.to_string(),
                effect_class,
                token,
            });
        }
    }
    violations
}

/// Validate that manual unsafe trait ownership has one explicit owner.
pub fn validate_unsafe_owner(
    module: &str,
    source: &str,
    expected_owner: &str,
) -> Option<BoundaryViolation> {
    let has_manual_unsafe_trait = source.contains("unsafe impl");
    (has_manual_unsafe_trait && module != expected_owner).then(|| BoundaryViolation {
        module: module.to_string(),
        effect_class: "unsafe-ownership",
        token: "unsafe impl",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_source_is_admitted() {
        let source = "pub fn next(value: u64) -> u64 { value.saturating_add(1) }";
        assert!(validate_core_source("valid_core.rs", source).is_empty());
    }

    #[test]
    fn forbidden_effect_fixtures_name_module_and_effect_class() {
        let fixtures = [
            (
                "filesystem.invalid.rs",
                include_str!("../../../contracts/architecture-modules/fixtures/invalid/filesystem.invalid.rs"),
                "filesystem",
            ),
            (
                "process.invalid.rs",
                include_str!("../../../contracts/architecture-modules/fixtures/invalid/process.invalid.rs"),
                "process",
            ),
            (
                "kvm.invalid.rs",
                include_str!("../../../contracts/architecture-modules/fixtures/invalid/kvm.invalid.rs"),
                "kvm-or-os-unsafe",
            ),
        ];
        for (module, source, expected_class) in fixtures {
            let violations = validate_core_source(module, source);
            assert_eq!(violations.len(), 1, "fixture {module}");
            assert_eq!(violations[0].module, module);
            assert_eq!(violations[0].effect_class, expected_class);
        }
    }

    #[test]
    fn unsafe_impl_outside_owner_is_rejected() {
        let violation =
            validate_unsafe_owner("vm.rs", "unsafe impl Send for Timer {}", "unsafe_owner.rs")
                .expect("reject misplaced unsafe impl");
        assert_eq!(violation.effect_class, "unsafe-ownership");
        assert!(validate_unsafe_owner(
            "unsafe_owner.rs",
            "unsafe impl Send for Timer {}",
            "unsafe_owner.rs"
        )
        .is_none());
    }
}
