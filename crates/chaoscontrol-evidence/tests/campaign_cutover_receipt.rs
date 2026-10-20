//! Committed Campaign cutover receipt contract.

use chaoscontrol_evidence::campaign_policy_readiness::{admit_cutover_receipt, CutoverReceipt};
use std::path::Path;

fn receipt() -> CutoverReceipt {
    serde_json::from_str(include_str!(
        "../../../contracts/campaign-cutover/receipt.json"
    ))
    .expect("cutover receipt")
}

// r[verify chaoscontrol.campaign_policy.cutover]
// r[verify chaoscontrol.campaign_policy.cutover.invalid]
#[test]
fn committed_receipt_admits_and_mutations_fail() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let receipt = receipt();
    admit_cutover_receipt(&root, &receipt, &receipt.source_revision)
        .expect("committed cutover receipt");

    let mut failed_conformance = receipt.clone();
    failed_conformance.conformance = false;
    assert!(admit_cutover_receipt(
        &root,
        &failed_conformance,
        &failed_conformance.source_revision
    )
    .is_err());

    let mut drifted_parity = receipt.clone();
    drifted_parity.kvm_parity.campaign_digest = format!("b3:{}", "0".repeat(64));
    assert!(
        admit_cutover_receipt(&root, &drifted_parity, &drifted_parity.source_revision).is_err()
    );

    let mut leaked = receipt;
    leaked.architecture[0]
        .product_types
        .push("Snapshot".to_string());
    assert!(admit_cutover_receipt(&root, &leaked, &leaked.source_revision).is_err());
}
