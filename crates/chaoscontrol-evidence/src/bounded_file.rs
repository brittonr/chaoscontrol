use crate::{EvidenceError, EvidenceResult};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Parent directory for an output path, using the current directory when the
/// path has no non-empty parent component.
pub fn output_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

pub fn open_regular_write(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "output target is not a regular file",
        ));
    }
    Ok(file)
}

pub fn write_new_bytes(path: &Path, bytes: &[u8]) -> EvidenceResult<()> {
    let parent = output_parent(path);
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| EvidenceError::new(format!("{}: {}", path.display(), error.error)))?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn write_bytes_replace(path: &Path, bytes: &[u8]) -> EvidenceResult<()> {
    let parent = output_parent(path);
    std::fs::create_dir_all(parent)?;
    let directory = File::open(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    directory.sync_all()?;
    Ok(())
}

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
    let bounded_length = maximum_bytes.saturating_add(1);
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(usize::MAX)
        .min(usize::try_from(maximum_bytes).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(capacity);
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

pub(crate) fn open_regular_file(path: &Path) -> EvidenceResult<File> {
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
    fn output_parent_handles_relative_paths() {
        assert_eq!(output_parent(Path::new("report.json")), Path::new("."));
        assert_eq!(
            output_parent(Path::new("out/report.json")),
            Path::new("out")
        );
    }

    #[test]
    fn regular_write_opens_reject_fifos() {
        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("output.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let _reader = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)
            .unwrap();
        assert_eq!(
            open_regular_write(&fifo).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn replacement_publication_is_atomic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested/report.txt");
        write_bytes_replace(&path, b"first").unwrap();
        write_bytes_replace(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn no_replace_publication_is_atomic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested/report.json");
        write_new_bytes(&path, b"{}\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");
        assert!(write_new_bytes(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");
    }

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
        assert_eq!(
            read_bounded_regular_bytes(&path, u64::MAX).unwrap(),
            b"abcd".to_vec()
        );
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
