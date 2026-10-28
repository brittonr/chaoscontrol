//! Publication shell for already-rendered replay-readiness artifacts.

// r[impl chaoscontrol.architecture_modules.evidence]

use std::path::Path;

use crate::EvidenceResult;

pub(crate) fn write_bytes(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> EvidenceResult<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    file.write_all(bytes.as_ref())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_creates_parent_and_writes_exact_bytes() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let path = root.path().join("nested/report.txt");
        write_bytes(&path, b"bounded report").expect("publish report");
        assert_eq!(std::fs::read(path).expect("read report"), b"bounded report");
    }

    #[test]
    fn publication_rejects_symlink_output() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let target = root.path().join("target.txt");
        std::fs::write(&target, b"original").unwrap();
        let link = root.path().join("report.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_bytes(&link, b"replacement").expect_err("symlink output must fail");
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    #[test]
    fn publication_rejects_directory_as_output_file() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let error =
            write_bytes(root.path(), b"not a directory").expect_err("directory output must fail");
        assert!(!error.to_string().is_empty());
    }
}
