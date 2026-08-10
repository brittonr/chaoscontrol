use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::scaffold::{plan, transform_text, TEXT_EXTENSIONS};

const USAGE_EXIT: u8 = 2;
const FAILURE_EXIT: u8 = 1;
const DEFAULT_WORKLOAD: &str = "my-service";
const MANIFEST_FILE: &str = "chaoscontrol-scaffold.json";
const MAX_TEMPLATE_ENTRIES: usize = 1_024;
const MAX_TEMPLATE_FILE_BYTES: u64 = 16 * 1_024 * 1_024;
const OWNER_WRITE_MODE: u32 = 0o200;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err((code, error)) => {
            eprintln!("{error}");
            ExitCode::from(code)
        }
    }
}

fn run(args: Vec<String>) -> Result<String, (u8, String)> {
    if args.as_slice() == ["-h"] || args.as_slice() == ["--help"] {
        return Ok(String::from(
            "usage: scaffold-rust-workload DEST [WORKLOAD_NAME]",
        ));
    }
    if !(args.len() == 1 || args.len() == 2) {
        return Err((
            USAGE_EXIT,
            String::from("usage: scaffold-rust-workload DEST [WORKLOAD_NAME]"),
        ));
    }
    let destination = PathBuf::from(&args[0]);
    let workload = args.get(1).map_or(DEFAULT_WORKLOAD, String::as_str);
    let template = PathBuf::from(required_env("CHAOSCONTROL_SCAFFOLD_TEMPLATE")?);
    let source_root = required_env("CHAOSCONTROL_SOURCE_ROOT")?;
    if destination.exists() {
        return Err((
            FAILURE_EXIT,
            format!("destination already exists: {}", destination.display()),
        ));
    }
    let scaffold = plan(workload, &source_root).map_err(|error| (USAGE_EXIT, error))?;
    copy_template(&template, &destination).map_err(shell_error)?;
    transform_files(&destination, &scaffold.replacements).map_err(shell_error)?;
    let manifest = serde_json::to_vec_pretty(&scaffold.manifest)
        .map_err(|error| shell_error(format!("manifest encode failed: {error}")))?;
    let mut bytes = manifest;
    bytes.push(b'\n');
    fs::write(destination.join(MANIFEST_FILE), bytes).map_err(|error| {
        shell_error(format!(
            "{}: {error}",
            destination.join(MANIFEST_FILE).display()
        ))
    })?;
    Ok(format!(
        "scaffolded Rust workload at: {}\nmanifest: {}",
        destination.display(),
        destination.join(MANIFEST_FILE).display()
    ))
}

fn required_env(name: &str) -> Result<String, (u8, String)> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| (USAGE_EXIT, format!("{name} is required")))
}

fn copy_template(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("template is not a directory: {}", source.display()));
    }
    fs::create_dir_all(destination.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    copy_directory(source, destination, &mut 0)
}

fn copy_directory(source: &Path, destination: &Path, count: &mut usize) -> Result<(), String> {
    fs::create_dir(destination).map_err(|error| format!("{}: {error}", destination.display()))?;
    let entries = fs::read_dir(source).map_err(|error| format!("{}: {error}", source.display()))?;
    for entry in entries {
        *count += 1;
        if *count > MAX_TEMPLATE_ENTRIES {
            return Err(String::from("template entry bound exceeded"));
        }
        let entry = entry.map_err(|error| format!("{}: {error}", source.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target, count)?;
        } else if file_type.is_file() {
            let source_metadata = fs::metadata(entry.path())
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
            validate_byte_length(
                &format!("template file {}", entry.path().display()),
                source_metadata.len(),
                MAX_TEMPLATE_FILE_BYTES,
            )?;
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("{}: {error}", target.display()))?;
            let mut permissions = fs::metadata(&target)
                .map_err(|error| format!("{}: {error}", target.display()))?
                .permissions();
            permissions.set_mode(permissions.mode() | OWNER_WRITE_MODE);
            fs::set_permissions(&target, permissions)
                .map_err(|error| format!("{}: {error}", target.display()))?;
        } else {
            return Err(format!(
                "template contains unsupported entry: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn transform_files(root: &Path, replacements: &[(String, String)]) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).map_err(|error| format!("{}: {error}", path.display()))? {
            count += 1;
            if count > MAX_TEMPLATE_ENTRIES {
                return Err(String::from("scaffold entry bound exceeded"));
            }
            let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() || !has_text_extension(&entry.path()) {
                continue;
            }
            let metadata = fs::metadata(entry.path())
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
            validate_byte_length(
                &format!("scaffold file {}", entry.path().display()),
                metadata.len(),
                MAX_TEMPLATE_FILE_BYTES,
            )?;
            let text = fs::read_to_string(entry.path())
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
            fs::write(entry.path(), transform_text(&text, replacements))
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn has_text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| TEXT_EXTENSIONS.contains(&value))
}

fn shell_error(error: impl ToString) -> (u8, String) {
    (FAILURE_EXIT, error.to_string())
}
