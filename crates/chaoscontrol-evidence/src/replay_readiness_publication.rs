//! Publication shell for already-rendered replay-readiness artifacts.

// r[impl chaoscontrol.architecture_modules.evidence]

use std::path::Path;

use crate::EvidenceResult;

pub(crate) fn write_bytes(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> EvidenceResult<()> {
    crate::bounded_file::write_bytes_replace(path.as_ref(), bytes.as_ref())
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

    #[test]
    fn failed_publication_leaves_no_temporary_files() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let directory_target = root.path().join("target");
        std::fs::create_dir(&directory_target).unwrap();
        let before = std::fs::read_dir(root.path()).unwrap().count();

        write_bytes(&directory_target, b"replacement").expect_err("directory target must fail");
        let after = std::fs::read_dir(root.path()).unwrap().count();
        assert_eq!(before, after);
    }
}
