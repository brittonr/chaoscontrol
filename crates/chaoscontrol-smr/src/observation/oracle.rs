//! Consumer-owned oracle boundary. ChaosControl transports admitted cohort facts
//! and records typed results; it never assigns protocol meaning itself.
use super::{error, CohortResult, Completeness, ProtocolProfile};
use crate::{canonical_value_ref, validate_digest_ref, SmrResult};
use serde::{Deserialize, Serialize};

pub const ORACLE_RESULT_SCHEMA: &str = "chaoscontrol.protocol-oracle-result.v1";
pub const MAX_DIAGNOSTICS: usize = 32;
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OracleVerdict {
    Pass,
    Fail,
    Incomplete,
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleResult {
    pub schema: String,
    pub oracle_ref: String,
    pub cohort_ref: String,
    pub verdict: OracleVerdict,
    pub diagnostics: Vec<String>,
}

impl OracleResult {
    /// Canonical identity of one typed oracle result.
    ///
    /// # Errors
    ///
    /// Rejects a schema, cohort, or oracle identity that does not match the
    /// admitted profile and cohort, an unbounded diagnostic set, and an empty
    /// diagnostic.
    // r[impl chaoscontrol.protocol_observation.oracle_boundary]
    pub fn admit(&self, profile: &ProtocolProfile, cohort: &CohortResult) -> SmrResult<String> {
        profile.admit()?;
        if self.schema != ORACLE_RESULT_SCHEMA
            || self.oracle_ref != profile.independent_oracle_ref
            || self.cohort_ref != cohort.cohort_ref
        {
            return Err(error(
                "oracle",
                "result identity differs from the admitted oracle or cohort",
            ));
        }
        validate_digest_ref("consumer oracle", &self.oracle_ref)?;
        if self.diagnostics.len() > MAX_DIAGNOSTICS {
            return Err(error("bound", "oracle diagnostics exceed the bound"));
        }
        for diagnostic in &self.diagnostics {
            if diagnostic.is_empty() || diagnostic.len() > MAX_DIAGNOSTIC_BYTES {
                return Err(error("oracle", "oracle diagnostic outside bounds"));
            }
        }
        if self.verdict == OracleVerdict::Pass && cohort.completeness != Completeness::Complete {
            return Err(error(
                "incomplete",
                "a pass result requires a complete admitted cohort",
            ));
        }
        canonical_value_ref(b"chaoscontrol.protocol-oracle-result.v1\0", self)
    }
}

/// Pure consumer-owned evaluation over one admitted cohort.
///
/// Implementations must not read clocks, files, environment, or network state,
/// and must not consult the runtime under test as their only expected result.
pub trait ProtocolOracle {
    /// Exact immutable consumer oracle identity; must equal the profile's
    /// independent oracle reference.
    fn identity(&self) -> &str;

    /// Evaluate admitted cohort facts. The cohort is complete when this runs.
    fn evaluate(&self, cohort: &CohortResult) -> SmrResult<OracleResult>;
}

/// Run a consumer oracle over one admitted cohort.
///
/// An incomplete cohort is recorded as `incomplete` and the oracle is not
/// invoked, so a partial observation set can never produce a passing result.
///
/// # Errors
///
/// Rejects a self-declared runtime oracle, an oracle whose identity differs from
/// the profile's independent oracle, and any result that fails its own admission.
// r[impl chaoscontrol.protocol_observation.oracle_boundary]
pub fn evaluate_oracle(
    profile: &ProtocolProfile,
    cohort: &CohortResult,
    oracle: &impl ProtocolOracle,
) -> SmrResult<OracleResult> {
    profile.admit()?;
    let identity = oracle.identity();
    if identity == profile.oracle_adapter_ref || identity != profile.independent_oracle_ref {
        return Err(error(
            "unsupported",
            "runtime self-oracle is not independent of the workload",
        ));
    }
    if cohort.completeness != Completeness::Complete {
        let result = OracleResult {
            schema: ORACLE_RESULT_SCHEMA.into(),
            oracle_ref: identity.into(),
            cohort_ref: cohort.cohort_ref.clone(),
            verdict: OracleVerdict::Incomplete,
            diagnostics: vec!["cohort is not complete".into()],
        };
        result.admit(profile, cohort)?;
        return Ok(result);
    }
    let result = oracle.evaluate(cohort)?;
    result.admit(profile, cohort)?;
    Ok(result)
}
