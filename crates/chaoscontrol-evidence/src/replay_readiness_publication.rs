//! Publication shell for already-rendered replay-readiness artifacts.

// r[impl chaoscontrol.architecture_modules.evidence]

pub(crate) fn write_bytes(path: &std::path::Path, bytes: &[u8]) -> crate::EvidenceResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
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
    fn publication_rejects_directory_as_output_file() {
        let root = tempfile::tempdir().expect("temporary publication root");
        let error =
            write_bytes(root.path(), b"not a directory").expect_err("directory output must fail");
        assert!(!error.to_string().is_empty());
    }
}
