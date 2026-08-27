use crate::{run_parity_corpus, validate_parity_report};

// r[verify chaoscontrol.vm_cohort.parity]
#[test]
fn legacy_and_shared_overlay_corpus_agrees_and_rejects_overclaim() {
    let report = run_parity_corpus().expect("parity corpus");
    assert!(validate_parity_report(&report));

    let mut overclaim = report.clone();
    overclaim.product_authority_granted = true;
    assert!(!validate_parity_report(&overclaim));

    let mut divergent = report;
    divergent.rows[0].agrees = false;
    assert!(!validate_parity_report(&divergent));
}
