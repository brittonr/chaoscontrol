//! Publication shell for already-rendered replay-readiness artifacts.

// r[impl chaoscontrol.architecture_modules.evidence]

use std::path::Path;

use crate::EvidenceResult;

pub(crate) fn write_bytes(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> EvidenceResult<()> {
    use std::io::Write;

    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let directory = std::fs::File::open(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes.as_ref())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    directory.sync_all()?;
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
    fn publication_replaces_symlink_without_following() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let target = root.path().join("target.txt");
        std::fs::write(&target, b"original").unwrap();
        let link = root.path().join("report.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_bytes(&link, b"replacement").expect("publish over link");
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert_eq!(std::fs::read(&link).unwrap(), b"replacement");
    }

    #[test]
    fn publication_rejects_directory_as_output_file() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let error =
            write_bytes(root.path(), b"not a directory").expect_err("directory output must fail");
        assert!(!error.to_string().is_empty());
    }
}
