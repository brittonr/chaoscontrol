//! Promotion admission for newly discovered exploration cases.
//!
//! A live case enters the stable regression corpus only after it has been
//! regenerated through Mantle with the exact same BLAKE3 identity. While the
//! materialization bundle is not admitted (pinned for v1), every promotion is
//! denied and the case stays exploration-only.

use serde::{Deserialize, Serialize};

use crate::profile::{proof_eligible, ExplorationProfile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromotionDenial {
    BundleNotMaterialized,
    IdentityMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionDecision {
    pub case_identity_blake3: String,
    pub outcome: PromotionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum PromotionOutcome {
    Admitted,
    Denied { denial: PromotionDenial },
}

/// Decides one promotion request. Fail-closed order: an unmaterialized bundle
/// denies before identities are compared, so no v1 case can be promoted even
/// with a matching digest.
pub fn admit_promotion(
    profile: &ExplorationProfile,
    case_identity_blake3: &str,
    materialized_blake3: Option<&str>,
) -> PromotionDecision {
    let denial = if !proof_eligible(profile) {
        PromotionDenial::BundleNotMaterialized
    } else if materialized_blake3 != Some(case_identity_blake3) {
        PromotionDenial::IdentityMismatch
    } else {
        return PromotionDecision {
            case_identity_blake3: case_identity_blake3.to_string(),
            outcome: PromotionOutcome::Admitted,
        };
    };
    PromotionDecision {
        case_identity_blake3: case_identity_blake3.to_string(),
        outcome: PromotionOutcome::Denied { denial },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{example_profile, BUNDLE_STATUS_MATERIALIZED};

    #[test]
    fn v1_cases_stay_exploration_only_even_with_matching_digests() {
        let profile = example_profile();
        let case_identity = crate::blake3_hex(b"case-identity");
        let decision = admit_promotion(&profile, &case_identity, Some(&case_identity));
        assert_eq!(
            decision,
            PromotionDecision {
                case_identity_blake3: case_identity.clone(),
                outcome: PromotionOutcome::Denied {
                    denial: PromotionDenial::BundleNotMaterialized
                },
            }
        );
        let decision = admit_promotion(&profile, &case_identity, None);
        assert_eq!(
            decision.outcome,
            PromotionOutcome::Denied {
                denial: PromotionDenial::BundleNotMaterialized
            }
        );
    }

    #[test]
    fn materialized_bundles_admit_only_exact_identity_matches() {
        let mut profile = example_profile();
        profile.materialization.bundle_status = BUNDLE_STATUS_MATERIALIZED.to_string();
        let case_identity = crate::blake3_hex(b"case-identity");

        let decision = admit_promotion(&profile, &case_identity, Some(&case_identity));
        assert_eq!(decision.outcome, PromotionOutcome::Admitted);

        let regenerated = crate::blake3_hex(b"regenerated-bytes");
        let decision = admit_promotion(&profile, &case_identity, Some(&regenerated));
        assert_eq!(
            decision.outcome,
            PromotionOutcome::Denied {
                denial: PromotionDenial::IdentityMismatch
            }
        );

        let decision = admit_promotion(&profile, &case_identity, None);
        assert_eq!(
            decision.outcome,
            PromotionOutcome::Denied {
                denial: PromotionDenial::IdentityMismatch
            }
        );
    }
}
