use super::*;

#[test]
fn canonical_json_ignores_input_field_order() {
    let left = canonical_pretty_json(br#"{"b":2,"a":1}"#).expect("left");
    let right = canonical_pretty_json(br#"{"a":1,"b":2}"#).expect("right");
    assert_eq!(left, right);
    assert_eq!(blake3_identity(&left), blake3_identity(&right));
}

#[test]
fn malformed_projection_is_rejected() {
    assert!(canonical_pretty_json(b"not-json").is_err());
}

#[test]
fn verifier_rejects_projection_substitution() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let valid = verify_profile_projection(
        &root,
        std::path::Path::new("contracts/evidence/fixtures/valid/run-profile.valid.json"),
        std::path::Path::new(
            "contracts/evidence/fixtures/valid/run-profile.projection-receipt.json",
        ),
        "vm-run",
    )
    .expect("committed projection verifies");
    let directory = tempfile::tempdir().expect("tempdir");
    let substituted = directory.path().join("run-profile.valid.json");
    std::fs::write(&substituted, valid.replace("\"seed\": 42", "\"seed\": 43"))
        .expect("write substituted projection");
    assert!(verify_profile_projection(
        &root,
        &substituted,
        std::path::Path::new(
            "contracts/evidence/fixtures/valid/run-profile.projection-receipt.json",
        ),
        "vm-run",
    )
    .is_err());
}
