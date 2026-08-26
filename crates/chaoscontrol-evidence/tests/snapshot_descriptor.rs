use chaoscontrol_evidence::snapshot_descriptor::fixture::{
    example_descriptor, fixture_paths, write_fixture_bundle,
};
use chaoscontrol_evidence::snapshot_descriptor::{
    chunked_closure_from_manifest, envelope, read_envelope, write_envelope,
};

const MAX_FIXTURE_BYTES: u64 = 1024 * 1024;

// r[verify chaoscontrol.snapshot_descriptor.verification]
#[test]
fn fixture_emits_monolithic_chunked_restore_and_consumer_artifacts() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let bundle = write_fixture_bundle(temp.path()).expect("write descriptor fixture bundle");
    assert_ne!(
        bundle.monolithic_descriptor, bundle.chunked_descriptor,
        "closure class must affect descriptor identity"
    );
    assert!(bundle.restore_completed);
    assert_eq!(bundle.consumer_claim_count, 0);
    for path in fixture_paths(temp.path()) {
        assert!(
            path.is_file(),
            "missing fixture artifact {}",
            path.display()
        );
    }
}

#[test]
fn descriptor_readback_rejects_field_substitution() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let path = temp.path().join("descriptor.json");
    let descriptor = example_descriptor().expect("example descriptor");
    write_envelope(&path, &envelope(descriptor).expect("descriptor envelope"))
        .expect("write descriptor envelope");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read descriptor"))
            .expect("parse descriptor JSON");
    value["descriptor"]["architecture"] = serde_json::Value::String("aarch64".to_string());
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("tampered JSON"),
    )
    .expect("write tampered descriptor");
    let error = read_envelope(&path).expect_err("tampered descriptor must fail readback");
    assert!(error.message().contains("architecture"));
}

#[test]
fn chunked_shell_rejects_missing_payload_member() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let manifest_path =
        chaoscontrol_evidence::write_snapshot_chunk_fixture(temp.path()).expect("chunk fixture");
    let manifest: chaoscontrol_evidence::SnapshotChunkManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    let missing = temp.path().join(&manifest.chunks[0].path);
    std::fs::remove_file(missing).expect("remove one chunk");
    let error = chunked_closure_from_manifest(&manifest_path, temp.path(), MAX_FIXTURE_BYTES)
        .expect_err("missing chunk must fail");
    assert!(error.message().contains("snapshot chunk missing"));
}

#[test]
fn chunked_shell_rejects_payload_digest_mismatch() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let manifest_path =
        chaoscontrol_evidence::write_snapshot_chunk_fixture(temp.path()).expect("chunk fixture");
    let manifest: chaoscontrol_evidence::SnapshotChunkManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    let first = temp.path().join(&manifest.chunks[0].path);
    let mut bytes = std::fs::read(&first).expect("read first chunk");
    bytes[0] ^= 1;
    std::fs::write(first, bytes).expect("write tampered chunk");
    let error = chunked_closure_from_manifest(&manifest_path, temp.path(), MAX_FIXTURE_BYTES)
        .expect_err("tampered chunk must fail");
    assert!(error.message().contains("snapshot chunk digest mismatch"));
}

#[test]
fn vmm_and_public_descriptor_profile_constants_match() {
    let descriptor = example_descriptor().expect("example descriptor");
    assert_eq!(
        descriptor.completeness_profile,
        chaoscontrol_vmm::snapshot::SNAPSHOT_PROFILE_EXACT_X86_KVM_V1
    );
    assert_eq!(
        descriptor.state_schema_version,
        chaoscontrol_vmm::snapshot::SNAPSHOT_STATE_SCHEMA_VERSION
    );
    assert_eq!(
        descriptor.architecture,
        chaoscontrol_snapshot_descriptor::EXACT_ARCHITECTURE
    );
}
