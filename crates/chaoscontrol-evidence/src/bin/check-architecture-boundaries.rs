use std::path::{Path, PathBuf};

use chaoscontrol_evidence::architecture_boundaries::{
    validate_core_source, validate_unsafe_owner, BoundaryViolation,
};

const CORE_PATHS: &[&str] = &[
    "crates/chaoscontrol-vmm/src/vm_core.rs",
    "crates/chaoscontrol-vmm/src/controller_core.rs",
    "crates/chaoscontrol-evidence/src/replay_readiness_core.rs",
    "crates/chaoscontrol-evidence/src/replay_readiness_render.rs",
];
const VMM_SOURCE_PATH: &str = "crates/chaoscontrol-vmm/src";
const UNSAFE_OWNER_FILE: &str = "unsafe_owner.rs";

fn repository_root() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn load_source(root: &Path, relative_path: &str) -> Result<String, String> {
    let path = root.join(relative_path);
    std::fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn collect_violations(root: &Path) -> Result<Vec<BoundaryViolation>, String> {
    let mut violations = Vec::new();
    for relative_path in CORE_PATHS {
        let source = load_source(root, relative_path)?;
        violations.extend(validate_core_source(relative_path, &source));
    }

    let vmm_source = root.join(VMM_SOURCE_PATH);
    let entries = std::fs::read_dir(&vmm_source)
        .map_err(|error| format!("read {}: {error}", vmm_source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read VMM source entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let module = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("non-UTF-8 module path: {}", path.display()))?;
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if let Some(violation) = validate_unsafe_owner(module, &source, UNSAFE_OWNER_FILE) {
            violations.push(violation);
        }
    }
    Ok(violations)
}

fn run() -> Result<(), String> {
    let root = repository_root();
    let violations = collect_violations(&root)?;
    if violations.is_empty() {
        println!(
            "architecture-boundaries: passed {} pure cores; unsafe owner={UNSAFE_OWNER_FILE}",
            CORE_PATHS.len()
        );
        return Ok(());
    }
    for violation in &violations {
        eprintln!(
            "architecture-boundaries: module={} effect_class={} token={:?}",
            violation.module, violation.effect_class, violation.token
        );
    }
    Err(format!(
        "architecture-boundaries: {} violation(s)",
        violations.len()
    ))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
