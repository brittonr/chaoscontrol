//! Positive and negative checks for the WalTier reference posture.

// r[impl chaoscontrol.waltier_dst.verification]
pub fn reference_posture_is_bounded(text: &str) -> bool {
    text.contains("creates no new ChaosControl gate or product requirement")
        && text.contains("does not prove")
        && text.contains("parity")
        && !text.contains("requires WalTier parity")
        && !text.contains("WalTier is a ChaosControl requirement")
}

// r[verify chaoscontrol.waltier_dst.verification]
#[test]
fn reference_document_preserves_bounded_posture_and_negative_copy_is_rejected() {
    let document = include_str!("../../../docs/references/waltier-dst.md");
    assert!(reference_posture_is_bounded(document));
    assert!(!reference_posture_is_bounded(
        "# WalTier\n\nWalTier is a ChaosControl requirement. It requires WalTier parity."
    ));
}
