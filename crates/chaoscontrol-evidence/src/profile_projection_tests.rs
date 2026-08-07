use super::*;
use crate::profile_projection_spec::{ArtifactSpec, ProjectionSpec};
use crate::profile_projection_verification::verify_profile_projection_for_spec;

const PROFILE_ID: &str = "test-profile";
const SOURCE_PATH: &str = "source.ncl";
const CONTRACT_PATH: &str = "contract.ncl";
const FIRST_IMPORT_PATH: &str = "import-a.ncl";
const SECOND_IMPORT_PATH: &str = "import-b.ncl";
const PROJECTION_PATH: &str = "projection.json";
const RECEIPT_PATH: &str = "receipt.json";

struct TestFixture {
    directory: tempfile::TempDir,
    spec: ProjectionSpec,
    receipt: ProjectionReceipt,
}

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
fn verifier_accepts_only_the_trusted_fixture() {
    let fixture = test_fixture();
    verify_fixture(&fixture).expect("trusted fixture verifies");
}

#[test]
fn verifier_rejects_projection_substitution() {
    let fixture = test_fixture();
    std::fs::write(
        fixture.directory.path().join(PROJECTION_PATH),
        br#"{"value":"substituted"}"#,
    )
    .expect("substitute projection");
    assert!(verify_fixture(&fixture).is_err());
}

#[test]
fn verifier_rejects_joint_projection_and_receipt_substitution() {
    let mut fixture = test_fixture();
    let replacement = canonical_pretty_json(br#"{"value":"substituted"}"#).expect("replacement");
    std::fs::write(fixture.directory.path().join(PROJECTION_PATH), &replacement)
        .expect("substitute projection");
    fixture.receipt.projection.identity = blake3_identity(&replacement);
    write_receipt(&fixture);
    assert_trusted_spec_rejection(verify_fixture(&fixture));
}

#[test]
fn verifier_rejects_joint_source_and_receipt_substitution() {
    let mut fixture = test_fixture();
    let replacement = b"{ value = false }";
    std::fs::write(fixture.directory.path().join(SOURCE_PATH), replacement)
        .expect("substitute source");
    fixture.receipt.source.identity = blake3_identity(replacement);
    write_receipt(&fixture);
    assert_trusted_spec_rejection(verify_fixture(&fixture));
}

#[test]
fn verifier_rejects_bound_artifact_path_substitution() {
    assert_receipt_mutation_rejected(|receipt| {
        receipt.source.path = "alternate-source.ncl".to_string();
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.contract.path = "alternate-contract.ncl".to_string();
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.imports[0].path = "alternate-import.ncl".to_string();
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.projection.path = "alternate-projection.json".to_string();
    });
}

#[test]
fn verifier_rejects_missing_extra_duplicate_and_reordered_imports() {
    assert_receipt_mutation_rejected(|receipt| receipt.imports.clear());
    assert_receipt_mutation_rejected(|receipt| {
        receipt.imports.push(BoundArtifact {
            path: "extra.ncl".to_string(),
            identity: blake3_identity(b"extra"),
        });
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.imports.push(receipt.imports[0].clone());
    });
    assert_receipt_mutation_rejected(|receipt| receipt.imports.swap(0, 1));
}

#[test]
fn verifier_rejects_header_and_non_claim_substitution() {
    assert_receipt_mutation_rejected(|receipt| {
        receipt.schema = "chaoscontrol.profile-projection-receipt.v2".to_string();
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.profile_id = "other-profile".to_string();
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.evaluator.name = "nickel attacker".to_string();
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.evaluator.identity = blake3_identity(b"nickel attacker");
    });
    assert_receipt_mutation_rejected(|receipt| {
        receipt.non_claims.pop();
    });
    assert_receipt_mutation_rejected(|receipt| receipt.non_claims.swap(0, 1));
}

#[test]
fn verifier_rejects_unknown_and_missing_receipt_fields() {
    let fixture = test_fixture();
    let mut unknown = serde_json::to_value(&fixture.receipt).expect("receipt value");
    unknown
        .as_object_mut()
        .expect("receipt object")
        .insert("authority".to_string(), serde_json::json!(true));
    write_receipt_value(&fixture, &unknown);
    assert!(verify_fixture(&fixture).is_err());

    let mut missing = serde_json::to_value(&fixture.receipt).expect("receipt value");
    missing
        .as_object_mut()
        .expect("receipt object")
        .remove("profile_id");
    write_receipt_value(&fixture, &missing);
    assert!(verify_fixture(&fixture).is_err());
}

#[test]
fn public_verifier_rejects_unknown_profiles_before_file_access() {
    let directory = tempfile::tempdir().expect("tempdir");
    let error = verify_profile_projection(
        directory.path(),
        std::path::Path::new(PROJECTION_PATH),
        std::path::Path::new(RECEIPT_PATH),
        PROFILE_ID,
    )
    .expect_err("unknown profile must fail closed");
    assert!(error.message().contains("unknown trusted profile ID"));
}

#[test]
fn verifier_rejects_requested_path_substitution() {
    let fixture = test_fixture();
    assert!(verify_profile_projection_for_spec(
        fixture.directory.path(),
        std::path::Path::new("alternate-projection.json"),
        std::path::Path::new(RECEIPT_PATH),
        &fixture.spec,
    )
    .is_err());
    assert!(verify_profile_projection_for_spec(
        fixture.directory.path(),
        std::path::Path::new(PROJECTION_PATH),
        std::path::Path::new("alternate-receipt.json"),
        &fixture.spec,
    )
    .is_err());
}

fn test_fixture() -> TestFixture {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path();
    let source = write_bound(root, SOURCE_PATH, b"{ value = true }");
    let contract = write_bound(root, CONTRACT_PATH, b"{ value | Bool }");
    let first_import = write_bound(root, FIRST_IMPORT_PATH, b"{ helper_a = true }");
    let second_import = write_bound(root, SECOND_IMPORT_PATH, b"{ helper_b = true }");
    let canonical = canonical_pretty_json(br#"{"value":"original"}"#).expect("canonical");
    std::fs::write(root.join(PROJECTION_PATH), &canonical).expect("write projection");
    let receipt = ProjectionReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        profile_id: PROFILE_ID.to_string(),
        source: source.clone(),
        contract: contract.clone(),
        imports: vec![first_import.clone(), second_import.clone()],
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
    let imports = Box::leak(
        vec![
            ArtifactSpec {
                path: FIRST_IMPORT_PATH,
                identity: leak(first_import.identity),
            },
            ArtifactSpec {
                path: SECOND_IMPORT_PATH,
                identity: leak(second_import.identity),
            },
        ]
        .into_boxed_slice(),
    );
    let spec = ProjectionSpec {
        profile_id: PROFILE_ID,
        source: ArtifactSpec {
            path: SOURCE_PATH,
            identity: leak(source.identity),
        },
        contract: ArtifactSpec {
            path: CONTRACT_PATH,
            identity: leak(contract.identity),
        },
        imports,
        projection: ArtifactSpec {
            path: PROJECTION_PATH,
            identity: leak(receipt.projection.identity.clone()),
        },
        receipt: RECEIPT_PATH,
    };
    let fixture = TestFixture {
        directory,
        spec,
        receipt,
    };
    write_receipt(&fixture);
    fixture
}

fn verify_fixture(fixture: &TestFixture) -> EvidenceResult<String> {
    verify_profile_projection_for_spec(
        fixture.directory.path(),
        std::path::Path::new(PROJECTION_PATH),
        std::path::Path::new(RECEIPT_PATH),
        &fixture.spec,
    )
}

fn assert_receipt_mutation_rejected(mutate: impl FnOnce(&mut ProjectionReceipt)) {
    let mut fixture = test_fixture();
    mutate(&mut fixture.receipt);
    write_receipt(&fixture);
    assert!(verify_fixture(&fixture).is_err());
}

fn assert_trusted_spec_rejection(result: EvidenceResult<String>) {
    let error = result.expect_err("substitution must fail closed");
    assert!(error.message().contains("trusted specification"));
}

fn write_receipt(fixture: &TestFixture) {
    let value = serde_json::to_value(&fixture.receipt).expect("receipt value");
    write_receipt_value(fixture, &value);
}

fn write_receipt_value(fixture: &TestFixture, value: &serde_json::Value) {
    std::fs::write(
        fixture.directory.path().join(RECEIPT_PATH),
        serde_json::to_vec_pretty(value).expect("serialize receipt"),
    )
    .expect("write receipt");
}

fn write_bound(root: &std::path::Path, path: &str, bytes: &[u8]) -> BoundArtifact {
    std::fs::write(root.join(path), bytes).expect("write bound fixture");
    BoundArtifact {
        path: path.to_string(),
        identity: blake3_identity(bytes),
    }
}

fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
