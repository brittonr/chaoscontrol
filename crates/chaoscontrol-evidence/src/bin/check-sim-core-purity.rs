use std::fs;

const CORE_MANIFEST: &str = "crates/chaoscontrol-sim-core/Cargo.toml";
const CORE_SOURCE: &str = "crates/chaoscontrol-sim-core/src";
const MAX_SOURCE_FILES: usize = 256;
const FORBIDDEN_DEPENDENCIES: [&str; 8] = [
    "kvm-bindings",
    "kvm-ioctls",
    "linux-loader",
    "libc",
    "vm-memory",
    "vmm-sys-util",
    "cap-std",
    "tempfile",
];
const FORBIDDEN_SOURCE_PATTERNS: [&str; 10] = [
    "use std::fs",
    "std::fs::",
    "use std::time",
    "std::time::",
    "use std::env",
    "std::env::",
    "use std::process",
    "std::process::",
    "use std::net",
    "std::net::",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    path: String,
    pattern: String,
}

fn find_violations(manifest: &str, sources: &[(String, String)]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for dependency in FORBIDDEN_DEPENDENCIES {
        if manifest.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with(dependency)
                && trimmed[dependency.len()..].trim_start().starts_with('=')
        }) {
            violations.push(Violation {
                path: CORE_MANIFEST.to_string(),
                pattern: dependency.to_string(),
            });
        }
    }
    for (path, source) in sources {
        for pattern in FORBIDDEN_SOURCE_PATTERNS {
            if source.contains(pattern) {
                violations.push(Violation {
                    path: path.clone(),
                    pattern: pattern.to_string(),
                });
            }
        }
    }
    violations
}

fn read_sources(root: &std::path::Path) -> Result<Vec<(String, String)>, String> {
    let source_root = root.join(CORE_SOURCE);
    let mut pending = vec![source_root];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        let entries = fs::read_dir(&path)
            .map_err(|error| format!("read source directory {}: {error}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read source entry: {error}"))?;
            let entry_path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("inspect {}: {error}", entry_path.display()))?;
            if file_type.is_dir() {
                pending.push(entry_path);
            } else if file_type.is_file()
                && entry_path
                    .extension()
                    .is_some_and(|extension| extension == "rs")
            {
                if files.len() >= MAX_SOURCE_FILES {
                    return Err(format!(
                        "sim-core source file bound exceeded: maximum={MAX_SOURCE_FILES}"
                    ));
                }
                let text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("read {}: {error}", entry_path.display()))?;
                let relative = entry_path
                    .strip_prefix(root)
                    .map_err(|error| format!("relativize {}: {error}", entry_path.display()))?
                    .display()
                    .to_string();
                files.push((relative, text));
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn run(root: &std::path::Path) -> Result<(), String> {
    let manifest_path = root.join(CORE_MANIFEST);
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("read {}: {error}", manifest_path.display()))?;
    let sources = read_sources(root)?;
    let violations = find_violations(&manifest, &sources);
    if !violations.is_empty() {
        for violation in &violations {
            eprintln!(
                "sim-core purity violation: path={} pattern={}",
                violation.path, violation.pattern
            );
        }
        return Err(format!(
            "sim-core purity failed: violations={}",
            violations.len()
        ));
    }
    println!(
        "sim-core purity ok: files={} forbidden_dependencies={} forbidden_source_patterns={}",
        sources.len(),
        FORBIDDEN_DEPENDENCIES.len(),
        FORBIDDEN_SOURCE_PATTERNS.len()
    );
    Ok(())
}

fn main() {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    if let Err(error) = run(&root) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_manifest_and_source_are_accepted() {
        let sources = vec![(
            "src/lib.rs".to_string(),
            "pub fn add(a: u64, b: u64) -> u64 { a + b }".to_string(),
        )];
        assert!(find_violations("serde = \"1\"", &sources).is_empty());
    }

    #[test]
    fn forbidden_dependency_and_source_name_the_exact_violation() {
        let sources = vec![(
            "src/shell.rs".to_string(),
            "use std::fs; fn read() { let _ = std::process::Command::new(\"bad\"); }".to_string(),
        )];
        let violations = find_violations("kvm-ioctls = \"0.19\"", &sources);
        assert!(violations.contains(&Violation {
            path: CORE_MANIFEST.to_string(),
            pattern: "kvm-ioctls".to_string(),
        }));
        assert!(violations.contains(&Violation {
            path: "src/shell.rs".to_string(),
            pattern: "use std::fs".to_string(),
        }));
        assert!(violations.contains(&Violation {
            path: "src/shell.rs".to_string(),
            pattern: "std::process::".to_string(),
        }));
    }
}
