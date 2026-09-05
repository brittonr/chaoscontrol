//! Exploration evidence DTOs and claim vocabulary.
//!
//! The report binds the profile identity, comparisons, shrink ledgers, and
//! bound skips to the six non-claims. Host-side Wasm exploration evidence
//! stays separate from VM snapshot-backed replay proof, assertion readiness,
//! package trust, semantic equivalence, sandbox proof, and release
//! eligibility; claims outside the bounded vocabulary are rejected as
//! overclaims.

use serde::{Deserialize, Serialize};

use crate::{
    blake3_hex,
    bounds::BoundSkip,
    canonical_json,
    compare::StrategyComparison,
    is_identifier,
    profile::{profile_identity, proof_eligible, ExplorationProfile, REQUIRED_NON_CLAIMS},
    shrink::ShrinkLedger,
};

pub const REPORT_SCHEMA: &str = "chaoscontrol.wasm-component-exploration-report.v1";
pub const BOUNDED_EVIDENCE_CLAIM: &str = "bounded-host-side-exploration-evidence";
pub const DIAGNOSTIC_ONLY_CLAIM: &str = "diagnostic-only";

/// The complete claim vocabulary a report may assert. Anything else is an
/// overclaim and must be rejected before a report is emitted.
pub fn check_claim(claim: &str) -> Result<(), String> {
    let mut allowed = REQUIRED_NON_CLAIMS
        .iter()
        .copied()
        .chain([BOUNDED_EVIDENCE_CLAIM, DIAGNOSTIC_ONLY_CLAIM]);
    if allowed.any(|allowed| allowed == claim) {
        Ok(())
    } else {
        Err(format!(
            "claim {claim:?} is outside the bounded exploration evidence vocabulary"
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationReport {
    pub schema: String,
    pub profile_id: String,
    pub profile_identity_blake3: String,
    pub proof_eligible: bool,
    pub comparisons: Vec<StrategyComparison>,
    pub shrink_ledgers: Vec<ShrinkLedger>,
    pub bound_skips: Vec<BoundSkip>,
    pub non_claims: Vec<String>,
    pub report_identity_blake3: String,
}

/// Builds the report DTO from admitted facts: validates the schema, binds the
/// six non-claims, and seals the BLAKE3 report identity. The identity covers
/// every comparison, ledger, and skip; identical facts seal identically.
pub fn exploration_report(
    profile: &ExplorationProfile,
    comparisons: Vec<StrategyComparison>,
    shrink_ledgers: Vec<ShrinkLedger>,
    bound_skips: Vec<BoundSkip>,
) -> Result<ExplorationReport, String> {
    let profile_identity_blake3 = profile_identity(profile)?;
    let mut report = ExplorationReport {
        schema: REPORT_SCHEMA.to_string(),
        profile_id: profile.profile_id.clone(),
        profile_identity_blake3,
        proof_eligible: proof_eligible(profile),
        comparisons,
        shrink_ledgers,
        bound_skips,
        non_claims: REQUIRED_NON_CLAIMS
            .iter()
            .map(|claim| claim.to_string())
            .collect(),
        report_identity_blake3: String::new(),
    };
    if !is_identifier(&report.profile_id) {
        return Err(format!(
            "report profile_id {:?} is not a bounded identifier",
            report.profile_id
        ));
    }
    let bytes = canonical_json(&report)?;
    report.report_identity_blake3 = blake3_hex(&bytes);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{compare_strategies, completed_observation, Strategy, StrategyRun};
    use crate::profile::example_profile;

    fn diverging_comparison() -> StrategyComparison {
        let mut pulley = completed_observation("1");
        pulley.result_values = vec!["2".to_string()];
        compare_strategies(
            "component-case-0001",
            "identity-blake3-value",
            StrategyRun {
                strategy: Strategy::Cranelift,
                observation: completed_observation("1"),
            },
            StrategyRun {
                strategy: Strategy::Pulley,
                observation: pulley,
            },
        )
        .expect("comparison")
    }

    #[test]
    fn reports_bind_non_claims_and_seal_a_deterministic_identity() {
        let profile = example_profile();
        let first = exploration_report(&profile, vec![diverging_comparison()], vec![], vec![])
            .expect("report");
        let second = exploration_report(&profile, vec![diverging_comparison()], vec![], vec![])
            .expect("report");
        assert_eq!(first, second);
        assert_eq!(first.schema, REPORT_SCHEMA);
        assert_eq!(
            first.non_claims,
            REQUIRED_NON_CLAIMS
                .iter()
                .map(|claim| claim.to_string())
                .collect::<Vec<_>>()
        );
        assert!(!first.proof_eligible);
        assert_eq!(first.report_identity_blake3.len(), crate::BLAKE3_HEX_LENGTH);

        let mut different = diverging_comparison();
        different.case_id = String::from("component-case-0002");
        let changed =
            exploration_report(&profile, vec![different], vec![], vec![]).expect("report");
        assert_ne!(changed.report_identity_blake3, first.report_identity_blake3);
    }

    #[test]
    fn overclaims_are_rejected_while_vocabulary_claims_pass() {
        for claim in REQUIRED_NON_CLAIMS {
            check_claim(claim).expect("non-claims are allowed");
        }
        check_claim(BOUNDED_EVIDENCE_CLAIM).expect("bounded evidence claim is allowed");
        check_claim(DIAGNOSTIC_ONLY_CLAIM).expect("diagnostic-only claim is allowed");

        for overclaim in [
            "proves-vm-replay",
            "universal-wasm-correctness",
            "sandbox-proof",
            "release-eligible",
            "assertion-ready",
            "semantically-equivalent",
            "package-trusted",
        ] {
            let error = check_claim(overclaim).expect_err("overclaim must be rejected");
            assert!(error.contains("bounded exploration evidence vocabulary"));
        }
    }
}
