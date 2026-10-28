//! Deterministic storage-recovery campaign request and receipt projection.
//!
//! Parsing, cohort construction, and receipt identity are pure. The CLI owns
//! file effects.

use serde::{Deserialize, Serialize};

use crate::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_smr::persistence::{PersistentNodeCohort, PersistentNodeState};
use chaoscontrol_smr::storage_recovery::{
    RecoveryProgressInput, RecoveryProgressReport, RecoveryProgressVerdict,
    StorageRecoveryObservation, StorageSafetyReport, STORAGE_RECOVERY_NON_CLAIMS,
};
use chaoscontrol_smr::storage_schedule::{
    run_storage_recovery_campaign, ScheduleObservation, StorageRecoveryCampaignInput,
    StorageRecoveryCohortIdentity, StorageRecoverySchedule,
};

pub const STORAGE_CAMPAIGN_REQUEST_SCHEMA: &str =
    "chaoscontrol.storage-recovery-campaign-request.v1";
pub const STORAGE_CAMPAIGN_RECEIPT_SCHEMA: &str =
    "chaoscontrol.storage-recovery-campaign-receipt.v1";
pub const MAX_CAMPAIGN_INPUT_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_CAMPAIGN_RECEIPT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageCampaignRequest {
    pub schema: String,
    pub node_count: usize,
    pub capacity_bytes: usize,
    pub initial_states: Vec<Option<PersistentNodeState>>,
    pub cohort: StorageRecoveryCohortIdentity,
    pub schedules: Vec<StorageRecoverySchedule>,
    pub safety: Vec<StorageRecoveryObservation>,
    pub progress: Vec<RecoveryProgressInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageCampaignReceipt {
    pub schema: String,
    pub input_blake3: String,
    pub receipt_ref: String,
    pub verdict: RecoveryProgressVerdict,
    pub schedule_observations: Vec<Vec<ScheduleObservation>>,
    pub safety: StorageSafetyReport,
    pub progress: Vec<RecoveryProgressReport>,
    pub non_claims: Vec<String>,
}

/// Run one campaign from bounded request bytes.
pub fn run_storage_campaign(input: &[u8]) -> Result<StorageCampaignReceipt, String> {
    validate_byte_length(
        "storage campaign request",
        u64::try_from(input.len()).map_err(|_| String::from("request length"))?,
        MAX_CAMPAIGN_INPUT_BYTES,
    )?;
    let request: StorageCampaignRequest = serde_json::from_slice(input)
        .map_err(|error| format!("storage campaign request: invalid JSON: {error}"))?;
    if request.schema != STORAGE_CAMPAIGN_REQUEST_SCHEMA {
        return Err(String::from(
            "storage campaign request: schema is not current",
        ));
    }
    if request.initial_states.len() != request.node_count {
        return Err(String::from(
            "storage campaign request: initial state count differs from node count",
        ));
    }
    let mut cohort = PersistentNodeCohort::new(request.node_count, request.capacity_bytes)
        .map_err(|error| error.to_string())?;
    for (node, state) in request.initial_states.iter().enumerate() {
        if let Some(state) = state {
            cohort
                .node_mut(node)
                .map_err(|error| error.to_string())?
                .write(state)
                .map_err(|error| error.to_string())?;
        }
    }
    let report = run_storage_recovery_campaign(
        cohort,
        StorageRecoveryCampaignInput {
            cohort: request.cohort,
            schedules: request.schedules,
            safety: request.safety,
            progress: request.progress,
        },
    )
    .map_err(|error| error.to_string())?;
    let receipt = StorageCampaignReceipt {
        schema: STORAGE_CAMPAIGN_RECEIPT_SCHEMA.to_string(),
        input_blake3: format!("blake3:{}", blake3::hash(input).to_hex()),
        receipt_ref: report.receipt_ref,
        verdict: report.verdict,
        schedule_observations: report.schedule_observations,
        safety: report.safety,
        progress: report.progress,
        non_claims: STORAGE_RECOVERY_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    };
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|error| format!("storage campaign receipt: {error}"))?;
    if encoded.len() > MAX_CAMPAIGN_RECEIPT_BYTES {
        return Err(String::from(
            "storage campaign receipt: byte bound exceeded",
        ));
    }
    Ok(receipt)
}

/// Render one validated receipt as canonical JSON bytes.
pub fn encode_storage_campaign_receipt(
    receipt: &StorageCampaignReceipt,
) -> Result<Vec<u8>, String> {
    if receipt.schema != STORAGE_CAMPAIGN_RECEIPT_SCHEMA {
        return Err(String::from(
            "storage campaign receipt: schema is not current",
        ));
    }
    if receipt.non_claims.len() != STORAGE_RECOVERY_NON_CLAIMS.len() {
        return Err(String::from(
            "storage campaign receipt: non-claims are incomplete",
        ));
    }
    serde_json::to_vec(receipt).map_err(|error| format!("storage campaign receipt: {error}"))
}

/// True when the campaign verdict admits the declared recovery behaviour.
pub fn is_storage_campaign_pass(verdict: RecoveryProgressVerdict) -> bool {
    verdict == RecoveryProgressVerdict::Pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_smr::storage_schedule::RecoveryScheduleStep;

    fn identity() -> StorageRecoveryCohortIdentity {
        StorageRecoveryCohortIdentity {
            candidate_blake3: "blake3:aa".to_string(),
            guest_image_blake3: "blake3:bb".to_string(),
            kernel_blake3: "blake3:cc".to_string(),
            filesystem: "ext4".to_string(),
            device_profile: "virtio-blk-flush-v1".to_string(),
            workload: "raft-storage-recovery".to_string(),
            schedule_blake3: "blake3:dd".to_string(),
            oracle_version: 1,
        }
    }

    fn state() -> PersistentNodeState {
        PersistentNodeState {
            term: 4,
            voted_for: Some(0),
            snapshot: None,
            log: vec![chaoscontrol_smr::persistence::PersistentLogEntry {
                index: 1,
                term: 4,
                command: b"set".to_vec(),
            }],
            application: b"applied".to_vec(),
        }
    }

    fn request() -> StorageCampaignRequest {
        StorageCampaignRequest {
            schema: STORAGE_CAMPAIGN_REQUEST_SCHEMA.to_string(),
            node_count: 3,
            capacity_bytes: 4096,
            initial_states: vec![Some(state()), Some(state()), Some(state())],
            cohort: identity(),
            schedules: vec![StorageRecoverySchedule {
                steps: vec![RecoveryScheduleStep::Restart { node: 0 }],
            }],
            safety: vec![StorageRecoveryObservation {
                participant: "node-0".to_string(),
                term: 4,
                durable_last_index: 1,
                acknowledged_last_index: 1,
                committed_last_index: 1,
                committed_term: 4,
                is_voting: true,
                is_local_sufficient: true,
                is_repair_completed: false,
                is_commitment_known: true,
            }],
            progress: vec![RecoveryProgressInput {
                participant: "node-0".to_string(),
                required_index: 1,
                is_local_sufficient: true,
                admitted_peers: vec!["node-1".to_string()],
                peer_available: Vec::new(),
                repair_source: None,
                is_repair_completed: false,
                is_fault_disrupted: false,
                source_sequence: 1,
                loss_count: 0,
                is_final_drain_complete: true,
                virtual_horizon: 100,
                is_horizon_reached: true,
                is_commitment_known: true,
            }],
        }
    }

    #[test]
    fn request_runs_and_projects_a_receipt() {
        let bytes = serde_json::to_vec(&request()).expect("encode");
        let receipt = run_storage_campaign(&bytes).expect("campaign");
        assert!(is_storage_campaign_pass(receipt.verdict));
        assert!(receipt.receipt_ref.starts_with("blake3:"));
        assert!(receipt.input_blake3.starts_with("blake3:"));
        assert_eq!(receipt.non_claims.len(), STORAGE_RECOVERY_NON_CLAIMS.len());
        let encoded = encode_storage_campaign_receipt(&receipt).expect("encode receipt");
        let decoded: StorageCampaignReceipt = serde_json::from_slice(&encoded).expect("decode");
        assert_eq!(decoded, receipt);
    }

    #[test]
    fn non_pass_verdict_is_reported_not_hidden() {
        let mut request = request();
        request.progress[0].is_local_sufficient = true;
        request.progress[0].repair_source = Some("node-1".to_string());
        let receipt =
            run_storage_campaign(&serde_json::to_vec(&request).expect("encode")).expect("campaign");
        assert!(!is_storage_campaign_pass(receipt.verdict));
    }

    #[test]
    fn stale_schema_over_bound_input_and_seed_mismatch_are_denied() {
        let mut stale = request();
        stale.schema = "chaoscontrol.storage-recovery-campaign-request.v0".to_string();
        assert!(
            run_storage_campaign(&serde_json::to_vec(&stale).expect("encode"))
                .expect_err("schema")
                .contains("schema is not current")
        );

        let mut mismatch = request();
        mismatch.initial_states.pop();
        assert!(
            run_storage_campaign(&serde_json::to_vec(&mismatch).expect("encode"))
                .expect_err("count")
                .contains("initial state count")
        );

        let oversized = vec![b'{'; usize::try_from(MAX_CAMPAIGN_INPUT_BYTES).expect("bound") + 1];
        assert!(run_storage_campaign(&oversized)
            .expect_err("bound")
            .contains("byte bound exceeded"));
    }
}
