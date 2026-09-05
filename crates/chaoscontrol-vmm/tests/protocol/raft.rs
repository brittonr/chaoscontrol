//! Consumer fixture: one leader identity per admitted term boundary.
//! This is not a Raft implementation or a universal Raft proof.

use chaoscontrol_protocol::protocol_observation::*;
use serde::Deserialize;

pub struct RaftOracle {
    pub adapter: String,
    pub schema: String,
    pub authority: OracleAuthority,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Projection {
    leader: String,
    term: u64,
    runtime_pass: bool,
}

impl RaftOracle {
    pub fn profile() -> AdmittedProfile {
        let bytes =
            include_bytes!("../../../../contracts/protocol-observation/fixtures/valid.json");
        let mut raw: ProtocolObservationProfile = serde_json::from_slice(bytes).unwrap();
        raw.oracle.adapter_ref = format!(
            "oracle-adapter:{}",
            blake3::hash(include_bytes!("raft.rs")).to_hex()
        );
        admit_profile(raw).unwrap()
    }
    pub fn new(profile: &AdmittedProfile) -> Self {
        Self {
            adapter: profile.profile.oracle.adapter_ref.clone(),
            schema: profile.profile.projection_schema_ref.clone(),
            authority: OracleAuthority::ConsumerIndependent,
        }
    }
}

impl ProtocolOracle for RaftOracle {
    fn adapter_ref(&self) -> &str {
        &self.adapter
    }
    fn projection_schema_ref(&self) -> &str {
        &self.schema
    }
    fn authority(&self) -> OracleAuthority {
        self.authority
    }
    fn evaluate(
        &self,
        cohort: &CohortResult,
        work_limit: u32,
    ) -> Result<OracleDecision, ProtocolObservationError> {
        let mut expected = None;
        let mut verdict = ProtocolVerdict::Pass;
        let mut work_items: u32 = 0;
        for record in &cohort.records {
            if work_items >= work_limit {
                return Err(ProtocolObservationError::OracleWorkExceeded);
            }
            work_items += 1;
            let bytes = record
                .collected
                .draft
                .projection_bytes
                .as_deref()
                .ok_or(ProtocolObservationError::OracleMismatch)?;
            let projection: Projection = serde_json::from_slice(bytes)
                .map_err(|_| ProtocolObservationError::OracleMismatch)?;
            // The runtime pass bit grants no oracle authority.
            let _runtime_claim = projection.runtime_pass;
            let actual = (projection.term, projection.leader);
            if expected.as_ref().is_some_and(|value| *value != actual) {
                verdict = ProtocolVerdict::Fail;
            }
            expected = Some(actual);
        }
        Ok(OracleDecision {
            verdict,
            diagnostic_refs: Vec::new(),
            work_items,
        })
    }
}
