//! Filesystem loading shell for replay-readiness evidence.

// r[impl chaoscontrol.architecture_modules.evidence]

use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

pub(crate) fn load_json(path: &std::path::Path) -> EvidenceResult<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| EvidenceError::new(format!("{}: {error}", path.display())))?;
    serde_json::from_str(&text).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_reads_valid_json() {
        let root = tempfile::tempdir().expect("temporary loader root");
        let path = root.path().join("receipt.json");
        std::fs::write(&path, br#"{"status":"passed"}"#).expect("write valid JSON");
        let loaded = load_json(&path).expect("load valid JSON");
        assert_eq!(loaded["status"], "passed");
    }

    #[test]
    fn loader_rejects_missing_and_malformed_inputs() {
        let root = tempfile::tempdir().expect("temporary loader root");
        let missing = root.path().join("missing.json");
        assert!(load_json(&missing)
            .expect_err("reject missing input")
            .to_string()
            .contains("missing.json"));

        let malformed = root.path().join("malformed.json");
        std::fs::write(&malformed, b"{").expect("write malformed JSON");
        assert!(!load_json(&malformed)
            .expect_err("reject malformed input")
            .to_string()
            .is_empty());
    }
}
