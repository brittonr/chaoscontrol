use std::io::Read;
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
const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_ENTRIES: usize = 4_096;

fn repository_root() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn admit_root(root: &Path) -> Result<(), String> {
    if root.as_os_str().is_empty() {
        return Err(String::from("root must not be empty"));
    }
    Ok(())
}

fn load_source(root: &Path, relative_path: &str) -> Result<String, String> {
    let path = root.join(relative_path);
    read_bounded_text(&path)
}

fn read_bounded_text(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return Err(format!("{} is not a bounded regular file", path.display()));
    }
    let mut text = String::new();
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if text.len() as u64 > MAX_SOURCE_BYTES {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(text)
}

fn collect_violations(root: &Path) -> Result<Vec<BoundaryViolation>, String> {
    let mut violations = Vec::new();
    for relative_path in CORE_PATHS {
        let source = load_source(root, relative_path)?;
        violations.extend(validate_core_source(relative_path, &source));
    }

    let vmm_source = root.join(VMM_SOURCE_PATH);
    let metadata = std::fs::symlink_metadata(&vmm_source)
        .map_err(|error| format!("inspect {}: {error}", vmm_source.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{} is not a real VMM source directory",
            vmm_source.display()
        ));
    }
    let entries = std::fs::read_dir(&vmm_source)
        .map_err(|error| format!("read {}: {error}", vmm_source.display()))?;
    let mut entries_seen = 0_usize;
    for entry in entries {
        entries_seen += 1;
        if entries_seen > MAX_SOURCE_ENTRIES {
            return Err(format!(
                "VMM source entry bound exceeded: maximum={MAX_SOURCE_ENTRIES}"
            ));
        }
        let entry = entry.map_err(|error| format!("read VMM source entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let module = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("non-UTF-8 module path: {}", path.display()))?;
        let source = read_bounded_text(&path)?;
        if let Some(violation) = validate_unsafe_owner(module, &source, UNSAFE_OWNER_FILE) {
            violations.push(violation);
        }
    }
    Ok(violations)
}

fn run() -> Result<(), String> {
    let root = repository_root();
    admit_root(&root)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn architecture_scan_rejects_excessive_entries() {
        let root = tempfile::tempdir().unwrap();
        for relative in CORE_PATHS {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        let vmm_source = root.path().join(VMM_SOURCE_PATH);
        std::fs::create_dir_all(&vmm_source).unwrap();
        for index in 0..=MAX_SOURCE_ENTRIES {
            std::fs::write(vmm_source.join(format!("module{index}.rs")), b"").unwrap();
        }

        let error = collect_violations(root.path()).expect_err("entry bound must fail");
        assert!(error.contains("entry bound exceeded"));
    }

    #[test]
    fn empty_root_is_rejected() {
        assert!(admit_root(Path::new("")).is_err());
    }

    #[test]
    fn architecture_scan_rejects_symlinked_vmm_source() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        for relative in CORE_PATHS {
            let path = if let Some(suffix) = relative.strip_prefix(&format!("{VMM_SOURCE_PATH}/")) {
                outside.path().join(suffix)
            } else {
                root.path().join(relative)
            };
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        std::fs::write(outside.path().join("module.rs"), b"").unwrap();
        let vmm_source = root.path().join(VMM_SOURCE_PATH);
        if vmm_source.exists() {
            std::fs::remove_dir_all(&vmm_source).unwrap();
        }
        std::fs::create_dir_all(vmm_source.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(outside.path(), &vmm_source).unwrap();

        let error = collect_violations(root.path()).expect_err("symlink rejected");
        assert!(error.contains("not a real VMM source directory"));
    }

    #[test]
    fn source_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.rs");
        std::fs::write(&target, b"source").unwrap();
        let link = root.path().join("module.rs");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded_text(&link).is_err());

        let fifo = root.path().join("module.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded_text(&fifo).is_err());
    }
}
