use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::source_guard::validate;

const MAX_ENTRIES: usize = 4_096;
const MAX_FLAKE_BYTES: u64 = 4 * 1_024 * 1_024;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    if args.len() != 1 {
        return Err(String::from(
            "usage: check-rust-product-automation-source ROOT",
        ));
    }
    let root = PathBuf::from(&args[0]);
    if root.as_os_str().is_empty() {
        return Err(String::from("root must not be empty"));
    }
    let scripts = list_files(&root.join("scripts"))?;
    let bins = list_files(&root.join("crates/chaoscontrol-evidence/src/bin"))?;
    let flake_path = root.join("flake.nix");
    let flake = read_bounded_text(&flake_path, MAX_FLAKE_BYTES)?;
    validate(&scripts, &flake, &bins)?;
    Ok(format!(
        "Rust product automation source guard ok: scripts={} rust_bins={}",
        scripts.len(),
        bins.len()
    ))
}

fn read_bounded_text(path: &Path, limit: u64) -> Result<String, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("flake.nix: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("flake.nix: {error}"))?;
    validate_byte_length("flake.nix", metadata.len(), limit)?;
    if !metadata.is_file() {
        return Err(String::from("flake.nix: not a regular file"));
    }
    let mut text = String::new();
    file.take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("flake.nix: {error}"))?;
    if text.len() as u64 > limit {
        return Err(String::from("flake.nix: exceeds the input byte bound"));
    }
    Ok(text)
}

fn list_files(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    let mut discovered = 0_usize;
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for entry in entries {
            discovered += 1;
            if discovered > MAX_ENTRIES {
                return Err(String::from("source guard entry bound exceeded"));
            }
            let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path().display().to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn empty_root_is_rejected() {
        assert!(run(vec![String::new()]).is_err());
    }

    #[test]
    fn source_walk_rejects_excessive_entries() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..=MAX_ENTRIES {
            std::fs::create_dir(root.path().join(format!("entry{index}"))).unwrap();
        }

        let error = list_files(root.path()).expect_err("entry bound must fail");
        assert!(error.contains("entry bound exceeded"));
    }

    #[test]
    fn flake_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.nix");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("flake.nix");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded_text(&link, MAX_FLAKE_BYTES).is_err());

        let fifo = root.path().join("flake.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded_text(&fifo, MAX_FLAKE_BYTES).is_err());
    }
}
