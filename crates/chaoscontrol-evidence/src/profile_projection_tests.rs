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
    const PROFILE_ID: &str = "test-profile";
    const PROJECTION_PATH: &str = "projection.json";
    const RECEIPT_PATH: &str = "receipt.json";
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path();
    let source = write_bound(root, "source.ncl", b"{ value = true }");
    let contract = write_bound(root, "contract.ncl", b"{ value | Bool }");
    let import = write_bound(root, "import.ncl", b"{ helper = true }");
    let canonical = canonical_pretty_json(br#"{"value":"original"}"#).expect("canonical");
    std::fs::write(root.join(PROJECTION_PATH), &canonical).expect("write projection");
    let receipt = ProjectionReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        profile_id: PROFILE_ID.to_string(),
        source,
        contract,
        imports: vec![import],
        evaluator: BoundIdentity {
            name: EVALUATOR_IDENTITY.to_string(),
            identity: blake3_identity(EVALUATOR_IDENTITY.as_bytes()),
        },
        projection: BoundArtifact {
            path: PROJECTION_PATH.to_string(),
            identity: blake3_identity(&canonical),
        },
        non_claims: NON_CLAIMS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    };
    std::fs::write(
        root.join(RECEIPT_PATH),
        serde_json::to_vec_pretty(&receipt).expect("serialize receipt"),
    )
    .expect("write receipt");
    verify_profile_projection(
        root,
        std::path::Path::new(PROJECTION_PATH),
        std::path::Path::new(RECEIPT_PATH),
        PROFILE_ID,
    )
    .expect("fixture verifies");
    std::fs::write(root.join(PROJECTION_PATH), br#"{"value":"substituted"}"#)
        .expect("substitute projection");
    assert!(verify_profile_projection(
        root,
        std::path::Path::new(PROJECTION_PATH),
        std::path::Path::new(RECEIPT_PATH),
        PROFILE_ID,
    )
    .is_err());
}

fn write_bound(root: &std::path::Path, path: &str, bytes: &[u8]) -> BoundArtifact {
    std::fs::write(root.join(path), bytes).expect("write bound fixture");
    BoundArtifact {
        path: path.to_string(),
        identity: blake3_identity(bytes),
    }
}
