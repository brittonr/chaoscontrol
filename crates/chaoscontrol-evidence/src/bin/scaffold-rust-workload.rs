use std::env;
use std::fs;
use std::io::{Read, Write};
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
    admit_destination(&destination).map_err(|error| (USAGE_EXIT, error))?;
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
    write_atomic(&destination.join(MANIFEST_FILE), &bytes).map_err(|error| {
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

fn admit_destination(destination: &Path) -> Result<(), String> {
    if destination.as_os_str().is_empty() {
        return Err(String::from("scaffold destination must not be empty"));
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String, (u8, String)> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| (USAGE_EXIT, format!("{name} is required")))
}

fn copy_template(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(source).map_err(|error| format!("{}: {error}", source.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "template is not a real directory: {}",
            source.display()
        ));
    }
    fs::create_dir_all(chaoscontrol_evidence::bounded_file::output_parent(
        destination,
    ))
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
            let (bytes, source_mode) =
                read_bounded_with_mode(&entry.path(), MAX_TEMPLATE_FILE_BYTES)?;
            write_atomic(&target, &bytes)
                .map_err(|error| format!("{}: {error}", target.display()))?;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(source_mode | OWNER_WRITE_MODE),
            )
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
            let text = read_bounded_text(&entry.path(), MAX_TEMPLATE_FILE_BYTES)?;
            write_existing(
                &entry.path(),
                transform_text(&text, replacements).as_bytes(),
            )
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

fn read_bounded_with_mode(path: &Path, limit: u64) -> Result<(Vec<u8>, u32), String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("template file {}", path.display()),
        metadata.len(),
        limit,
    )?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok((bytes, metadata.permissions().mode()))
}

fn read_bounded_text(path: &Path, limit: u64) -> Result<String, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{} is not a bounded regular file", path.display()));
    }
    let mut text = String::new();
    file.take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if text.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(text)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = chaoscontrol_evidence::bounded_file::output_parent(path);
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()
}

fn write_existing(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "transformed file target is not a regular file",
        ));
    }
    file.write_all(bytes)?;
    file.sync_all()
}

fn shell_error(error: impl ToString) -> (u8, String) {
    (FAILURE_EXIT, error.to_string())
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn atomic_manifest_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join(MANIFEST_FILE);
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomic(&link, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&link).unwrap(), b"replacement");
    }

    #[test]
    fn empty_destination_is_rejected() {
        assert!(admit_destination(Path::new("")).is_err());
        assert!(admit_destination(Path::new("workload")).is_ok());
    }

    #[test]
    fn template_copy_rejects_symlinked_source() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let template = root.path().join("template");
        std::os::unix::fs::symlink(outside.path(), &template).unwrap();
        let destination = root.path().join("output");
        assert!(copy_template(&template, &destination).is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn template_copy_publishes_content_and_permissions() {
        let root = tempfile::tempdir().unwrap();
        let template = root.path().join("template");
        std::fs::create_dir(&template).unwrap();
        let source = template.join("main.rs");
        fs::write(&source, b"fn main() {}\n").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o440)).unwrap();
        let destination = root.path().join("workload");

        copy_template(&template, &destination).unwrap();
        let copied = destination.join("main.rs");
        assert_eq!(fs::read(&copied).unwrap(), b"fn main() {}\n");
        assert_eq!(
            fs::metadata(&copied).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn template_byte_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.bin");
        fs::write(&target, b"source").unwrap();
        let link = root.path().join("module.bin");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded_with_mode(&link, MAX_TEMPLATE_FILE_BYTES).is_err());

        let fifo = root.path().join("module.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded_with_mode(&fifo, MAX_TEMPLATE_FILE_BYTES).is_err());
    }

    #[test]
    fn template_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.rs");
        fs::write(&target, b"source").unwrap();
        let link = root.path().join("module.rs");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded_text(&link, MAX_TEMPLATE_FILE_BYTES).is_err());

        let fifo = root.path().join("module.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded_text(&fifo, MAX_TEMPLATE_FILE_BYTES).is_err());
    }

    #[test]
    fn transformed_file_write_rejects_fifos() {
        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("main.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let _reader = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)
            .unwrap();
        assert_eq!(
            write_existing(&fifo, b"replacement").unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn transformed_file_write_rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.rs");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("main.rs");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(write_existing(&link, b"replacement").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }
}
