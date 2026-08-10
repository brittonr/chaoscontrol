//! Filesystem loading shell for replay-readiness evidence.

// r[impl chaoscontrol.architecture_modules.evidence]

use std::path::Path;

use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

pub(crate) fn load_json(path: &Path) -> EvidenceResult<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| EvidenceError::new(format!("read {}: {error}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|error| EvidenceError::new(format!("parse {}: {error}", path.display())))
}
