//! Bounded protocol-observation receipt. Missing, conflicting, unsupported, or
//! partial facts stay distinct from pass, and promotions are refused.
use super::marker::MarkerBinding;
use super::oracle::{OracleResult, OracleVerdict};
use super::{error, CohortResult, Completeness, ProtocolProfile, NON_CLAIMS};
use crate::{canonical_value_ref, validate_digest_ref, SmrResult};
use serde::{Deserialize, Serialize};

pub const RECEIPT_SCHEMA: &str = "chaoscontrol.protocol-observation-receipt.v1";
pub const MAX_REFERENCE_SET: usize = 4096;

/// One bounded campaign receipt for admitted protocol observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolReceipt {
    pub schema: String,
    pub profile_ref: String,
    pub bounds_ref: String,
    pub boundary_ref: String,
    pub cohort_ref: String,
    pub completeness: Completeness,
    pub record_refs: Vec<String>,
    pub novelty_ref: String,
    pub oracle_adapter_ref: String,
    pub oracle_ref: String,
    pub oracle_verdict: OracleVerdict,
    pub oracle_result_ref: String,
    pub marker_ref: Option<String>,
    pub parent_snapshot_ref: Option<String>,
    pub scheduler_refs: Vec<String>,
    pub fault_refs: Vec<String>,
    pub replay_refs: Vec<String>,
    pub non_claims: Vec<String>,
}

impl ProtocolReceipt {
    /// Bind one campaign's admitted facts into a canonical receipt.
    ///
    /// # Errors
    ///
    /// Rejects an unadmitted profile, a cohort or oracle result from another
    /// campaign, a completeness upgrade, a marker bound to a different cohort,
    /// unbounded reference sets, and any missing required non-claim.
    // r[impl chaoscontrol.protocol_observation.evidence]
    pub fn build(
        profile: &ProtocolProfile,
        cohort: &CohortResult,
        oracle: &OracleResult,
        marker: Option<&MarkerBinding>,
        scheduler_refs: Vec<String>,
        fault_refs: Vec<String>,
        replay_refs: Vec<String>,
    ) -> SmrResult<Self> {
        let profile_ref = profile.admit()?;
        oracle.admit(profile, cohort)?;
        let bounds_ref =
            canonical_value_ref(b"chaoscontrol.protocol-bounds.v1\0", &profile.bounds)?;
        for refs in [&scheduler_refs, &fault_refs, &replay_refs] {
            if refs.len() > MAX_REFERENCE_SET {
                return Err(error("bound", "receipt reference set exceeds the bound"));
            }
            for reference in refs {
                validate_digest_ref("receipt reference", reference)?;
            }
        }
        let (marker_ref, parent_snapshot_ref) = match marker {
            Some(binding) => {
                if binding.cohort_ref != cohort.cohort_ref
                    || binding.cohort_completeness != cohort.completeness
                {
                    return Err(error(
                        "conflicting",
                        "marker binding belongs to a different cohort state",
                    ));
                }
                (
                    Some(binding.marker_ref.clone()),
                    Some(binding.parent_snapshot_ref.clone()),
                )
            }
            None => (None, None),
        };
        let oracle_result_ref = oracle.admit(profile, cohort)?;
        Ok(Self {
            schema: RECEIPT_SCHEMA.into(),
            profile_ref,
            bounds_ref,
            boundary_ref: cohort.boundary_ref.clone(),
            cohort_ref: cohort.cohort_ref.clone(),
            completeness: cohort.completeness,
            record_refs: cohort.record_refs.clone(),
            novelty_ref: cohort.novelty_ref.clone(),
            oracle_adapter_ref: profile.oracle_adapter_ref.clone(),
            oracle_ref: oracle.oracle_ref.clone(),
            oracle_verdict: oracle.verdict,
            oracle_result_ref,
            marker_ref,
            parent_snapshot_ref,
            scheduler_refs,
            fault_refs,
            replay_refs,
            non_claims: profile.non_claims.clone(),
        })
    }

    /// Revalidate a retained receipt against the same profile and cohort facts.
    ///
    /// # Errors
    ///
    /// Rejects schema or identity drift, a completeness upgrade, a pass verdict
    /// on an incomplete cohort, and a missing required non-claim.
    // r[impl chaoscontrol.protocol_observation.evidence]
    pub fn admit(&self, profile: &ProtocolProfile, cohort: &CohortResult) -> SmrResult<String> {
        let profile_ref = profile.admit()?;
        if self.schema != RECEIPT_SCHEMA
            || self.profile_ref != profile_ref
            || self.cohort_ref != cohort.cohort_ref
            || self.boundary_ref != cohort.boundary_ref
            || self.record_refs != cohort.record_refs
            || self.novelty_ref != cohort.novelty_ref
            || self.completeness != cohort.completeness
            || self.oracle_adapter_ref != profile.oracle_adapter_ref
            || self.oracle_ref != profile.independent_oracle_ref
        {
            return Err(error(
                "conflicting",
                "receipt identities differ from the admitted campaign",
            ));
        }
        let bounds_ref =
            canonical_value_ref(b"chaoscontrol.protocol-bounds.v1\0", &profile.bounds)?;
        if self.bounds_ref != bounds_ref {
            return Err(error("conflicting", "receipt bounds identity drifted"));
        }
        if self.completeness != Completeness::Complete && self.oracle_verdict == OracleVerdict::Pass
        {
            return Err(error(
                "incomplete",
                "an incomplete cohort cannot carry a pass result",
            ));
        }
        if NON_CLAIMS
            .iter()
            .any(|claim| !self.non_claims.iter().any(|value| value == claim))
        {
            return Err(error("claim", "required non-claim missing from receipt"));
        }
        canonical_value_ref(b"chaoscontrol.protocol-observation-receipt.v1\0", self)
    }

    /// Whether the receipt records a passing consumer result for a complete cohort.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.completeness == Completeness::Complete && self.oracle_verdict == OracleVerdict::Pass
    }
}
