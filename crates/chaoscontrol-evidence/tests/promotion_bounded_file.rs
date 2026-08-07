use chaoscontrol_evidence::validate_readiness_promotion_files;

const MANIFEST_PATH: &str = "../../dogfood-results/accepted-workload-proofs.json";
const REPORT_PATH: &str = "../../docs/replay-readiness-status.md";
const MAX_EVIDENCE_JSON_BYTES: usize = 16 * 1024 * 1024;
const OVERSIZED_EVIDENCE_JSON_BYTES: usize = MAX_EVIDENCE_JSON_BYTES + 1;

#[test]
fn regular_promotion_inputs_validate() {
    validate_readiness_promotion_files(MANIFEST_PATH, REPORT_PATH)
        .expect("regular historical inputs remain quarantined");
}

#[cfg(unix)]
#[test]
fn symlinked_promotion_manifest_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = std::fs::canonicalize(MANIFEST_PATH).expect("canonical manifest");
    let link = temp.path().join("manifest.json");
    symlink(manifest, &link).expect("create manifest symlink");

    let error = validate_readiness_promotion_files(&link, REPORT_PATH)
        .expect_err("symlinked manifest must fail closed");
    assert!(error.message().contains("manifest.json"));
}

#[test]
fn oversized_promotion_manifest_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = temp.path().join("manifest.json");
    std::fs::write(&manifest, vec![b' '; OVERSIZED_EVIDENCE_JSON_BYTES])
        .expect("write oversized manifest");

    let error = validate_readiness_promotion_files(&manifest, REPORT_PATH)
        .expect_err("oversized manifest must fail closed");
    assert!(error
        .message()
        .contains("file exceeds the input byte limit"));
}
