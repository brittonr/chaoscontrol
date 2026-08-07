use chaoscontrol_evidence::validate_assertion_summary;
use serde_json::{json, Value};
use std::path::Path;

fn strict_summary_fixture() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/evidence/fixtures/valid/assertions.identity.valid.json");
    let bytes = std::fs::read(path).expect("strict summary fixture");
    serde_json::from_slice(&bytes).expect("strict summary JSON")
}

#[test]
fn fatal_structured_metadata_spoof_is_rejected() {
    let mut summary = strict_summary_fixture();
    summary["catalog_status"] = json!("fatal-conflict");
    summary["collision_safe_evidence"] = json!(false);
    summary["assertions"][0]["message"] = json!("spoofed top-level message");

    assert!(validate_assertion_summary(&summary).is_err());
}
