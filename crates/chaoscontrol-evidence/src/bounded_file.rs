use crate::{EvidenceError, EvidenceResult};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) fn read_bounded_regular_file(path: &Path, maximum_bytes: u64) -> EvidenceResult<String> {
    let bytes = read_bounded_regular_bytes(path, maximum_bytes)?;
    String::from_utf8(bytes)
        .map_err(|error| EvidenceError::new(format!("{}: invalid UTF-8: {error}", path.display())))
}

pub(crate) fn read_bounded_regular_bytes(
    path: &Path,
    maximum_bytes: u64,
) -> EvidenceResult<Vec<u8>> {
    let mut file = open_regular_file(path)?;
    let metadata = file
        .metadata()
        .map_err(|error| EvidenceError::new(format!("{}: {error}", path.display())))?;
    if metadata.len() > maximum_bytes {
        return Err(EvidenceError::new(format!(
            "{}: file exceeds the input byte limit",
            path.display()
        )));
    }
    let bounded_length = maximum_bytes
        .checked_add(1)
        .ok_or_else(|| EvidenceError::new("input byte limit overflow"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(bounded_length)
        .read_to_end(&mut bytes)
        .map_err(|error| EvidenceError::new(format!("{}: {error}", path.display())))?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(EvidenceError::new(format!(
            "{}: file exceeds the input byte limit",
            path.display()
        )));
    }
    Ok(bytes)
}

fn open_regular_file(path: &Path) -> EvidenceResult<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| EvidenceError::new(format!("{}: {error}", path.display())))?;
    let file_type = file
        .metadata()
        .map_err(|error| EvidenceError::new(format!("{}: {error}", path.display())))?
        .file_type();
    if !file_type.is_file() {
        return Err(EvidenceError::new(format!(
            "{}: expected a regular file",
            path.display()
        )));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn bounded_reads_accept_regular_files_at_the_limit() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input.json");
        std::fs::write(&path, b"abcd").unwrap();

        assert_eq!(
            read_bounded_regular_bytes(&path, 4).unwrap(),
            b"abcd".to_vec()
        );
        assert!(read_bounded_regular_bytes(&path, 3).is_err());
        assert_eq!(read_bounded_regular_file(&path, 4).unwrap(), "abcd");
    }

    #[test]
    fn bounded_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        std::fs::write(&target, b"{}").unwrap();
        let link = root.path().join("input.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded_regular_bytes(&link, 16).is_err());

        let fifo = root.path().join("input.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded_regular_bytes(&fifo, 16).is_err());
    }

    #[test]
    fn bounded_reads_reject_directories() {
        let root = tempfile::tempdir().unwrap();
        assert!(read_bounded_regular_bytes(root.path(), 16).is_err());
    }
}
