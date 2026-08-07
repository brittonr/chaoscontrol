use chaoscontrol_evidence::validate_assertion_summary_for_promotion;
use chaoscontrol_explore::assertion_summary::AssertionSummaryV2;
use chaoscontrol_explore::assertion_summary_writer::{
    write_assertion_summary, AssertionSummaryWrite,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const COMPATIBILITY_ALIAS: u32 = 17;

fn legacy_summary() -> AssertionSummaryV2 {
    serde_json::from_value(json!({
        "schema": "chaoscontrol.assertion-summary.v2",
        "catalog_status": "legacy-ambiguous",
        "collision_safe_evidence": false,
        "assertions": [{
            "id": COMPATIBILITY_ALIAS,
            "message": "historical assertion",
            "kind": "always",
            "guest": "historical-guest",
            "category": "uncategorized",
            "verdict": "unexercised",
            "hit_count": 0,
            "true_count": 0,
            "false_count": 0
        }]
    }))
    .expect("valid summary")
}

fn strict_summary() -> AssertionSummaryV2 {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/evidence/fixtures/valid/assertions.identity.valid.json");
    let bytes = fs::read(path).expect("strict summary fixture");
    serde_json::from_slice(&bytes).expect("validated strict summary")
}

fn destination(root: &TempDir) -> PathBuf {
    root.path().join("assertions.json")
}

fn read_summary(path: &Path) -> AssertionSummaryV2 {
    let bytes = fs::read(path).expect("summary bytes");
    serde_json::from_slice(&bytes).expect("validated summary JSON")
}

#[test]
fn writes_valid_summary_atomically() {
    let root = TempDir::new().expect("temporary directory");
    let path = destination(&root);

    let result = write_assertion_summary(&path, || Ok(legacy_summary()));

    assert_eq!(result, Ok(AssertionSummaryWrite::Written));
    assert_eq!(read_summary(&path).assertions().len(), 1);
    assert_eq!(fs::read_dir(root.path()).expect("directory").count(), 1);
}

#[test]
fn writes_and_revalidates_accepted_strict_summary() {
    let root = TempDir::new().expect("temporary directory");
    let path = destination(&root);

    write_assertion_summary(&path, || Ok(strict_summary())).expect("strict summary write");

    let value = serde_json::from_slice(&fs::read(&path).expect("strict summary bytes"))
        .expect("strict summary JSON");
    validate_assertion_summary_for_promotion(&value).expect("promotable written summary");
}

#[test]
fn invalid_input_removes_stale_accepted_evidence() {
    let root = TempDir::new().expect("temporary directory");
    let path = destination(&root);
    fs::write(&path, br#"{"catalog_status":"accepted"}"#).expect("stale evidence");

    let result = write_assertion_summary(&path, || Err("invalid summary".to_string()));

    assert!(result.is_err());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn symlink_destination_is_replaced_without_following_target() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().expect("temporary directory");
    let path = destination(&root);
    let target = root.path().join("target.json");
    fs::write(&target, b"do not replace").expect("target");
    symlink(&target, &path).expect("destination symlink");

    write_assertion_summary(&path, || Ok(legacy_summary())).expect("safe write");

    assert_eq!(fs::read(&target).expect("target bytes"), b"do not replace");
    assert!(!fs::symlink_metadata(&path)
        .expect("destination metadata")
        .file_type()
        .is_symlink());
    assert_eq!(read_summary(&path).assertions().len(), 1);
}

#[test]
fn malformed_deserialized_summary_leaves_no_destination() {
    let root = TempDir::new().expect("temporary directory");
    let path = destination(&root);
    let invalid = json!({
        "schema": "chaoscontrol.assertion-summary.v2",
        "catalog_status": "pending",
        "collision_safe_evidence": false,
        "assertions": []
    });

    let result = write_assertion_summary(&path, || {
        serde_json::from_value(invalid).map_err(|error| error.to_string())
    });

    assert!(result.is_err());
    assert!(!path.exists());
}

#[test]
fn temporary_file_creation_failure_leaves_no_destination() {
    let root = TempDir::new().expect("temporary directory");
    let missing_parent = root.path().join("missing");
    let path = missing_parent.join("assertions.json");

    let result = write_assertion_summary(&path, || Ok(legacy_summary()));

    assert!(result.is_err());
    assert!(!path.exists());
}
