//! Deterministic storage-recovery schedules and cohort-bound receipts.
//!
//! Schedules mutate only the supplied in-memory disks. Receipts bind declared
//! cohort identities; neither grants commitment, repair, or release authority.
//!
//! r[impl chaoscontrol.smr_storage_recovery.persistent_nodes]
//! r[impl chaoscontrol.smr_storage_recovery.evidence]

use serde::{Deserialize, Serialize};

use crate::persistence::{PersistentNodeCohort, PersistentNodeState};
use crate::storage_recovery::{StorageSafetyReport, STORAGE_RECOVERY_NON_CLAIMS};
use crate::{SmrError, SmrResult};

/// Maximum steps in one storage-recovery schedule.
pub const MAX_SCHEDULE_STEPS: usize = 64;
/// Receipt schema.
pub const STORAGE_RECOVERY_RECEIPT_SCHEMA: &str = "chaoscontrol.storage-recovery-receipt.v1";
/// Oracle version bound into receipts.
pub const STORAGE_RECOVERY_ORACLE_VERSION: u32 = 1;

fn deny(class: &'static str, detail: impl Into<String>) -> SmrError {
    SmrError::new(class, detail)
}

/// One deterministic schedule step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum RecoveryScheduleStep {
    /// Restart one application while retaining its published disk state.
    Restart { node: usize },
    /// Mark one replica as lagging without mutating its durable record.
    LagReplica { node: usize, behind_by: u64 },
    /// Remove one admitted peer from the observation set.
    MakePeerUnavailable { peer: String },
    /// Corrupt one published byte range on one node.
    Corrupt { node: usize, offset: usize },
    /// Attempt a local repair by reloading the published record.
    LocalRepair { node: usize },
}

/// Ordered schedule for one declared cohort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoverySchedule {
    pub steps: Vec<RecoveryScheduleStep>,
}

/// One observed schedule effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum ScheduleObservation {
    Restarted {
        node: usize,
        published_index: Option<u64>,
    },
    Lagged {
        node: usize,
        behind_by: u64,
    },
    PeerUnavailable {
        peer: String,
    },
    Corrupted {
        node: usize,
        offset: usize,
    },
    LocalRepairDenied {
        node: usize,
    },
    LocalRepaired {
        node: usize,
    },
    Denied {
        class: String,
    },
}

impl StorageRecoverySchedule {
    /// Validate every step against the declared cohort size.
    pub fn validate(&self, node_count: usize) -> SmrResult<()> {
        if self.steps.is_empty() {
            return Err(deny("storage-schedule-empty", "schedule carries no steps"));
        }
        if self.steps.len() > MAX_SCHEDULE_STEPS {
            return Err(deny(
                "storage-schedule-over-bound",
                format!(
                    "schedule of {} steps exceeds {MAX_SCHEDULE_STEPS}",
                    self.steps.len()
                ),
            ));
        }
        for step in &self.steps {
            match step {
                RecoveryScheduleStep::Restart { node }
                | RecoveryScheduleStep::LagReplica { node, .. }
                | RecoveryScheduleStep::Corrupt { node, .. }
                | RecoveryScheduleStep::LocalRepair { node }
                    if *node >= node_count =>
                {
                    return Err(deny(
                        "storage-schedule-node",
                        format!("node {node} is outside the declared cohort"),
                    ));
                }
                RecoveryScheduleStep::MakePeerUnavailable { peer } if peer.is_empty() => {
                    return Err(deny("storage-schedule-peer", "peer identity is empty"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Apply one schedule to the supplied disks and record every effect.
pub fn apply_schedule(
    cohort: &mut PersistentNodeCohort,
    schedule: &StorageRecoverySchedule,
) -> SmrResult<Vec<ScheduleObservation>> {
    schedule.validate(cohort.len())?;
    let mut observations = Vec::with_capacity(schedule.steps.len());
    for step in &schedule.steps {
        match step {
            RecoveryScheduleStep::Restart { node } => match cohort.node(*node)?.load() {
                Ok(state) => {
                    observations.push(ScheduleObservation::Restarted {
                        node: *node,
                        published_index: state.as_ref().map(last_index),
                    });
                }
                Err(error) => {
                    // A corrupt record is an observed restart denial, not a
                    // campaign abort; the workload reports it to the oracle.
                    observations.push(ScheduleObservation::Denied {
                        class: error.class.to_string(),
                    });
                }
            },
            RecoveryScheduleStep::LagReplica { node, behind_by } => {
                observations.push(ScheduleObservation::Lagged {
                    node: *node,
                    behind_by: *behind_by,
                });
            }
            RecoveryScheduleStep::MakePeerUnavailable { peer } => {
                observations.push(ScheduleObservation::PeerUnavailable { peer: peer.clone() });
            }
            RecoveryScheduleStep::Corrupt { node, offset } => {
                cohort.node_mut(*node)?.corrupt_published(*offset, 0xFF)?;
                observations.push(ScheduleObservation::Corrupted {
                    node: *node,
                    offset: *offset,
                });
            }
            RecoveryScheduleStep::LocalRepair { node } => match cohort.node(*node)?.load() {
                Ok(_) => observations.push(ScheduleObservation::LocalRepaired { node: *node }),
                Err(_) => observations.push(ScheduleObservation::LocalRepairDenied { node: *node }),
            },
        }
    }
    Ok(observations)
}

fn last_index(state: &PersistentNodeState) -> u64 {
    state.log.last().map_or_else(
        || {
            state
                .snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.last_index)
        },
        |entry| entry.index,
    )
}

/// Declared cohort identities bound by one receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoveryCohortIdentity {
    pub candidate_blake3: String,
    pub guest_image_blake3: String,
    pub kernel_blake3: String,
    pub filesystem: String,
    pub device_profile: String,
    pub workload: String,
    pub schedule_blake3: String,
    pub oracle_version: u32,
}

/// Cohort-bound storage-recovery receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoveryReceipt {
    pub schema: String,
    pub cohort: StorageRecoveryCohortIdentity,
    pub participant_identities: Vec<Option<String>>,
    pub safety: StorageSafetyReport,
    pub non_claims: Vec<String>,
}

/// Build one receipt from declared identities and observed participant state.
pub fn identify_storage_recovery_receipt(
    cohort: StorageRecoveryCohortIdentity,
    disks: &PersistentNodeCohort,
    safety: StorageSafetyReport,
) -> SmrResult<(String, String)> {
    let receipt = StorageRecoveryReceipt {
        schema: STORAGE_RECOVERY_RECEIPT_SCHEMA.to_string(),
        cohort,
        participant_identities: disks.identities(),
        safety,
        non_claims: STORAGE_RECOVERY_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    };
    validate_storage_recovery_receipt(&receipt)?;
    let encoded = serde_json::to_vec(&receipt)
        .map_err(|error| deny("storage-receipt-encoding", error.to_string()))?;
    Ok((
        format!("blake3:{}", blake3::hash(&encoded).to_hex()),
        String::from_utf8(encoded)
            .map_err(|error| deny("storage-receipt-encoding", error.to_string()))?,
    ))
}

/// Validate one receipt without trusting its rendered summary.
pub fn validate_storage_recovery_receipt(receipt: &StorageRecoveryReceipt) -> SmrResult<()> {
    if receipt.schema != STORAGE_RECOVERY_RECEIPT_SCHEMA {
        return Err(deny(
            "storage-receipt-schema",
            "receipt schema is not current",
        ));
    }
    if receipt.cohort.oracle_version != STORAGE_RECOVERY_ORACLE_VERSION {
        return Err(deny(
            "storage-receipt-oracle",
            "oracle version is not current",
        ));
    }
    for (name, value) in [
        ("candidate", &receipt.cohort.candidate_blake3),
        ("guest", &receipt.cohort.guest_image_blake3),
        ("kernel", &receipt.cohort.kernel_blake3),
        ("filesystem", &receipt.cohort.filesystem),
        ("device", &receipt.cohort.device_profile),
        ("workload", &receipt.cohort.workload),
        ("schedule", &receipt.cohort.schedule_blake3),
    ] {
        if value.is_empty() {
            return Err(deny(
                "storage-receipt-identity",
                format!("{name} identity is empty"),
            ));
        }
    }
    if !receipt.cohort.candidate_blake3.starts_with("blake3:")
        || !receipt.cohort.guest_image_blake3.starts_with("blake3:")
        || !receipt.cohort.kernel_blake3.starts_with("blake3:")
        || !receipt.cohort.schedule_blake3.starts_with("blake3:")
    {
        return Err(deny(
            "storage-receipt-identity",
            "byte identities must be BLAKE3 references",
        ));
    }
    if receipt.participant_identities.is_empty() {
        return Err(deny(
            "storage-receipt-participants",
            "receipt carries no participants",
        ));
    }
    if receipt.non_claims.is_empty() {
        return Err(deny(
            "storage-receipt-nonclaims",
            "receipt omits its non-claims",
        ));
    }
    if receipt.safety.is_pass && !receipt.safety.findings.is_empty() {
        return Err(deny(
            "storage-receipt-decision",
            "passing safety decision carries findings",
        ));
    }
    for finding in &receipt.safety.findings {
        if finding.participant.is_empty() {
            return Err(deny(
                "storage-receipt-finding",
                "finding omits its participant",
            ));
        }
    }
    Ok(())
}

/// Declared inputs for one deterministic storage-recovery campaign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoveryCampaignInput {
    pub cohort: StorageRecoveryCohortIdentity,
    pub schedules: Vec<StorageRecoverySchedule>,
    pub safety: Vec<crate::storage_recovery::StorageRecoveryObservation>,
    pub progress: Vec<crate::storage_recovery::RecoveryProgressInput>,
    /// Optional staged writes executed before the schedules.
    #[serde(default)]
    pub writes: Vec<StorageRecoveryWrite>,
}

/// One armed disk fault for a campaign write attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageWriteFault {
    WriteError,
    FlushError,
}

/// One campaign write attempt bound to one node and optional fault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoveryWrite {
    pub node: usize,
    pub state: crate::persistence::PersistentNodeState,
    pub fault: Option<StorageWriteFault>,
}

/// Observed effect of one campaign write attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum StorageWriteObservation {
    Published { node: usize, identity: String },
    Refused { node: usize, class: String },
}

/// Campaign result binding schedules, safety, progress, and one receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoveryCampaignReport {
    pub receipt_ref: String,
    pub write_observations: Vec<StorageWriteObservation>,
    pub schedule_observations: Vec<Vec<ScheduleObservation>>,
    pub safety: StorageSafetyReport,
    pub progress: Vec<crate::storage_recovery::RecoveryProgressReport>,
    pub verdict: crate::storage_recovery::RecoveryProgressVerdict,
}

fn campaign_verdict(
    safety: &StorageSafetyReport,
    progress: &[crate::storage_recovery::RecoveryProgressReport],
) -> crate::storage_recovery::RecoveryProgressVerdict {
    use crate::storage_recovery::RecoveryProgressVerdict;
    if !safety.is_pass
        || progress
            .iter()
            .any(|report| report.verdict == RecoveryProgressVerdict::Fail)
    {
        return RecoveryProgressVerdict::Fail;
    }
    if progress
        .iter()
        .any(|report| report.verdict == RecoveryProgressVerdict::NotEvaluated)
    {
        return RecoveryProgressVerdict::NotEvaluated;
    }
    if progress
        .iter()
        .any(|report| report.verdict == RecoveryProgressVerdict::Incomplete)
    {
        return RecoveryProgressVerdict::Incomplete;
    }
    RecoveryProgressVerdict::Pass
}

fn apply_write(
    cohort: &mut PersistentNodeCohort,
    write: &StorageRecoveryWrite,
) -> SmrResult<StorageWriteObservation> {
    let disk = cohort.node_mut(write.node)?;
    match write.fault {
        Some(StorageWriteFault::WriteError) => disk.fail_next_write(),
        Some(StorageWriteFault::FlushError) => disk.fail_next_flush(),
        None => {}
    }
    match disk.write(&write.state) {
        Ok(publication) => Ok(StorageWriteObservation::Published {
            node: write.node,
            identity: publication.identity,
        }),
        Err(error) => Ok(StorageWriteObservation::Refused {
            node: write.node,
            class: error.class.to_string(),
        }),
    }
}

/// Apply every schedule, evaluate safety and progress, and bind one receipt.
pub fn run_storage_recovery_campaign(
    mut cohort: PersistentNodeCohort,
    input: StorageRecoveryCampaignInput,
) -> SmrResult<StorageRecoveryCampaignReport> {
    if input.schedules.is_empty() || input.safety.is_empty() || input.progress.is_empty() {
        return Err(deny(
            "storage-campaign-input",
            "campaign requires schedules, safety observations, and progress inputs",
        ));
    }
    let mut write_observations = Vec::with_capacity(input.writes.len());
    for write in &input.writes {
        write_observations.push(apply_write(&mut cohort, write)?);
    }
    let mut schedule_observations = Vec::with_capacity(input.schedules.len());
    for schedule in &input.schedules {
        schedule_observations.push(apply_schedule(&mut cohort, schedule)?);
    }
    let safety = crate::storage_recovery::validate_storage_recovery(&input.safety)?;
    let mut progress = Vec::with_capacity(input.progress.len());
    for facts in &input.progress {
        progress.push(crate::storage_recovery::evaluate_recovery_progress(facts)?);
    }
    let verdict = campaign_verdict(&safety, &progress);
    let (receipt_ref, _) =
        identify_storage_recovery_receipt(input.cohort, &cohort, safety.clone())?;
    Ok(StorageRecoveryCampaignReport {
        receipt_ref,
        write_observations,
        schedule_observations,
        safety,
        progress,
        verdict,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistentNodeState;

    fn cohort() -> PersistentNodeCohort {
        let mut cohort = PersistentNodeCohort::new(3, 4096).expect("cohort");
        for node in 0..3 {
            cohort
                .node_mut(node)
                .expect("node")
                .write(&PersistentNodeState {
                    term: 4,
                    voted_for: Some(0),
                    snapshot: None,
                    log: vec![crate::persistence::PersistentLogEntry {
                        index: 1,
                        term: 4,
                        command: b"set".to_vec(),
                    }],
                    application: b"applied".to_vec(),
                })
                .expect("write");
        }
        cohort
    }

    fn identity() -> StorageRecoveryCohortIdentity {
        StorageRecoveryCohortIdentity {
            candidate_blake3: "blake3:aa".to_string(),
            guest_image_blake3: "blake3:bb".to_string(),
            kernel_blake3: "blake3:cc".to_string(),
            filesystem: "ext4".to_string(),
            device_profile: "virtio-blk-flush-v1".to_string(),
            workload: "raft-storage-recovery".to_string(),
            schedule_blake3: "blake3:dd".to_string(),
            oracle_version: STORAGE_RECOVERY_ORACLE_VERSION,
        }
    }

    #[test]
    fn schedule_restart_lag_corruption_and_repair_are_observed() {
        let mut cohort = cohort();
        let schedule = StorageRecoverySchedule {
            steps: vec![
                RecoveryScheduleStep::Restart { node: 0 },
                RecoveryScheduleStep::LagReplica {
                    node: 1,
                    behind_by: 2,
                },
                RecoveryScheduleStep::MakePeerUnavailable {
                    peer: "node-2".to_string(),
                },
                RecoveryScheduleStep::Corrupt { node: 2, offset: 0 },
                RecoveryScheduleStep::LocalRepair { node: 2 },
            ],
        };
        let observations = apply_schedule(&mut cohort, &schedule).expect("schedule");
        assert_eq!(observations.len(), 5);
        assert_eq!(
            observations[0],
            ScheduleObservation::Restarted {
                node: 0,
                published_index: Some(1)
            }
        );
        assert_eq!(
            observations[4],
            ScheduleObservation::LocalRepairDenied { node: 2 }
        );
    }

    #[test]
    fn local_repair_succeeds_on_an_intact_record() {
        let mut cohort = cohort();
        let schedule = StorageRecoverySchedule {
            steps: vec![RecoveryScheduleStep::LocalRepair { node: 1 }],
        };
        assert_eq!(
            apply_schedule(&mut cohort, &schedule).expect("schedule"),
            vec![ScheduleObservation::LocalRepaired { node: 1 }]
        );
    }

    #[test]
    fn empty_out_of_range_and_over_bound_schedules_are_denied() {
        let mut cohort = cohort();
        for (schedule, class) in [
            (
                StorageRecoverySchedule { steps: Vec::new() },
                "storage-schedule-empty",
            ),
            (
                StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::Restart { node: 3 }],
                },
                "storage-schedule-node",
            ),
            (
                StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::Restart { node: 0 }; MAX_SCHEDULE_STEPS + 1],
                },
                "storage-schedule-over-bound",
            ),
        ] {
            assert_eq!(
                apply_schedule(&mut cohort, &schedule)
                    .expect_err("denied")
                    .class,
                class
            );
        }
    }

    #[test]
    fn receipt_binds_identities_and_rejects_cohort_drift() {
        let cohort = cohort();
        let safety = StorageSafetyReport {
            findings: Vec::new(),
            is_pass: true,
        };
        let (receipt_ref, encoded) =
            identify_storage_recovery_receipt(identity(), &cohort, safety.clone())
                .expect("receipt");
        assert!(receipt_ref.starts_with("blake3:"));
        let receipt: StorageRecoveryReceipt = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(receipt.participant_identities.len(), 3);
        assert_eq!(receipt.non_claims.len(), STORAGE_RECOVERY_NON_CLAIMS.len());

        let mut drifted = identity();
        drifted.oracle_version = 0;
        assert_eq!(
            identify_storage_recovery_receipt(drifted, &cohort, safety.clone())
                .expect_err("oracle")
                .class,
            "storage-receipt-oracle"
        );

        let mut overclaim = receipt;
        overclaim
            .safety
            .findings
            .push(crate::storage_recovery::StorageSafetyFinding {
                participant: "node-0".to_string(),
                class: crate::storage_recovery::StorageSafetyFindingClass::AcknowledgedDataLoss,
            });
        assert_eq!(
            validate_storage_recovery_receipt(&overclaim)
                .expect_err("overclaim")
                .class,
            "storage-receipt-decision"
        );
    }

    #[test]
    fn receipt_rejects_empty_identities_and_missing_participants() {
        let cohort = cohort();
        let mut receipt = StorageRecoveryReceipt {
            schema: STORAGE_RECOVERY_RECEIPT_SCHEMA.to_string(),
            cohort: identity(),
            participant_identities: cohort.identities(),
            safety: StorageSafetyReport {
                findings: Vec::new(),
                is_pass: true,
            },
            non_claims: vec!["bounded".to_string()],
        };
        receipt.cohort.kernel_blake3.clear();
        assert_eq!(
            validate_storage_recovery_receipt(&receipt)
                .expect_err("empty")
                .class,
            "storage-receipt-identity"
        );
        receipt.cohort.kernel_blake3 = "blake3:cc".to_string();
        receipt.participant_identities.clear();
        assert_eq!(
            validate_storage_recovery_receipt(&receipt)
                .expect_err("participants")
                .class,
            "storage-receipt-participants"
        );
    }

    use crate::storage_recovery::{
        RecoveryProgressInput, RecoveryProgressIssue, RecoveryProgressVerdict,
        StorageRecoveryObservation,
    };

    fn progress_for(participant: &str, is_local_sufficient: bool) -> RecoveryProgressInput {
        RecoveryProgressInput {
            participant: participant.to_string(),
            required_index: 1,
            is_local_sufficient,
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
        }
    }

    fn observation(participant: &str) -> StorageRecoveryObservation {
        StorageRecoveryObservation {
            participant: participant.to_string(),
            term: 4,
            durable_last_index: 1,
            acknowledged_last_index: 1,
            committed_last_index: 1,
            committed_term: 4,
            is_voting: true,
            is_local_sufficient: true,
            is_repair_completed: false,
            is_commitment_known: true,
        }
    }

    fn campaign(
        participants: &[&str],
        progress: Vec<RecoveryProgressInput>,
        schedules: Vec<StorageRecoverySchedule>,
    ) -> StorageRecoveryCampaignInput {
        StorageRecoveryCampaignInput {
            cohort: identity(),
            schedules,
            safety: participants.iter().map(|name| observation(name)).collect(),
            progress,
            writes: Vec::new(),
        }
    }

    #[test]
    fn local_only_campaign_passes_and_binds_a_receipt() {
        let report = run_storage_recovery_campaign(
            cohort(),
            campaign(
                &["node-0", "node-1", "node-2"],
                vec![progress_for("node-0", true)],
                vec![StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::Restart { node: 0 }],
                }],
            ),
        )
        .expect("campaign");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
        assert_eq!(report.progress[0].issue, RecoveryProgressIssue::LocalRepair);
        assert!(report.receipt_ref.starts_with("blake3:"));
        assert_eq!(report.schedule_observations.len(), 1);
    }

    #[test]
    fn peer_repair_campaign_passes() {
        let mut facts = progress_for("node-0", false);
        facts.repair_source = Some("node-1".to_string());
        facts.is_repair_completed = true;
        let report = run_storage_recovery_campaign(
            cohort(),
            campaign(
                &["node-0", "node-1", "node-2"],
                vec![facts],
                vec![StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::LocalRepair { node: 0 }],
                }],
            ),
        )
        .expect("campaign");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
        assert_eq!(report.progress[0].issue, RecoveryProgressIssue::PeerRepair);
    }

    #[test]
    fn unnecessary_and_missed_repair_campaigns_fail() {
        let mut unnecessary = progress_for("node-0", true);
        unnecessary.repair_source = Some("node-1".to_string());
        let report = run_storage_recovery_campaign(
            cohort(),
            campaign(
                &["node-0", "node-1", "node-2"],
                vec![unnecessary],
                vec![StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::Restart { node: 0 }],
                }],
            ),
        )
        .expect("campaign");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Fail);
        assert_eq!(
            report.progress[0].issue,
            RecoveryProgressIssue::UnnecessaryRemoteRepair
        );

        let mut missed = progress_for("node-0", false);
        missed.peer_available = vec!["node-1".to_string()];
        let report = run_storage_recovery_campaign(
            cohort(),
            campaign(
                &["node-0", "node-1", "node-2"],
                vec![missed],
                vec![StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::MakePeerUnavailable {
                        peer: "node-2".to_string(),
                    }],
                }],
            ),
        )
        .expect("campaign");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Fail);
        assert_eq!(
            report.progress[0].issue,
            RecoveryProgressIssue::MissedAvailableRepair
        );
    }

    #[test]
    fn incomplete_observation_campaign_is_incomplete() {
        let mut facts = progress_for("node-0", false);
        facts.is_final_drain_complete = false;
        let report = run_storage_recovery_campaign(
            cohort(),
            campaign(
                &["node-0", "node-1", "node-2"],
                vec![facts],
                vec![StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::LagReplica {
                        node: 1,
                        behind_by: 3,
                    }],
                }],
            ),
        )
        .expect("campaign");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Incomplete);
    }

    #[test]
    fn quorum_intersection_storage_loss_is_rejected() {
        let mut lossy = campaign(
            &["node-0", "node-1"],
            vec![progress_for("node-0", false)],
            vec![StorageRecoverySchedule {
                steps: vec![
                    RecoveryScheduleStep::Corrupt { node: 0, offset: 0 },
                    RecoveryScheduleStep::Corrupt { node: 1, offset: 0 },
                ],
            }],
        );
        for observation in &mut lossy.safety {
            observation.durable_last_index = 0;
        }
        let report = run_storage_recovery_campaign(cohort(), lossy).expect("campaign");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Fail);
        assert!(!report.safety.is_pass);
        assert!(report.safety.findings.iter().any(|finding| finding.class
            == crate::storage_recovery::StorageSafetyFindingClass::CommittedEntryTruncation));
        assert_eq!(report.schedule_observations[0].len(), 2);
    }

    #[test]
    fn write_and_flush_fault_campaigns_refuse_without_advancing_state() {
        let mut state = crate::persistence::PersistentNodeState {
            term: 5,
            voted_for: None,
            snapshot: None,
            log: vec![crate::persistence::PersistentLogEntry {
                index: 1,
                term: 5,
                command: b"set".to_vec(),
            }],
            application: b"applied".to_vec(),
        };
        let mut input = campaign(
            &["node-0", "node-1", "node-2"],
            vec![progress_for("node-0", true)],
            vec![StorageRecoverySchedule {
                steps: vec![RecoveryScheduleStep::Restart { node: 0 }],
            }],
        );
        input.writes = vec![
            StorageRecoveryWrite {
                node: 0,
                state: state.clone(),
                fault: Some(StorageWriteFault::FlushError),
            },
            StorageRecoveryWrite {
                node: 0,
                state: state.clone(),
                fault: Some(StorageWriteFault::WriteError),
            },
            StorageRecoveryWrite {
                node: 0,
                state: state.clone(),
                fault: None,
            },
        ];
        let report = run_storage_recovery_campaign(cohort(), input).expect("campaign");
        assert_eq!(
            report.write_observations[0],
            StorageWriteObservation::Refused {
                node: 0,
                class: String::from("storage-flush-failed"),
            }
        );
        assert_eq!(
            report.write_observations[1],
            StorageWriteObservation::Refused {
                node: 0,
                class: String::from("storage-write-failed"),
            }
        );
        assert!(matches!(
            report.write_observations[2],
            StorageWriteObservation::Published { node: 0, .. }
        ));
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
        state.term = 5;
    }

    #[test]
    fn corruption_and_cache_retention_campaigns_keep_prior_published_state() {
        let mut input = campaign(
            &["node-0", "node-1", "node-2"],
            vec![progress_for("node-0", false)],
            vec![
                StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::Corrupt { node: 0, offset: 0 }],
                },
                StorageRecoverySchedule {
                    steps: vec![RecoveryScheduleStep::Restart { node: 0 }],
                },
            ],
        );
        input.progress[0].repair_source = Some("node-1".to_string());
        input.progress[0].is_repair_completed = true;
        let report = run_storage_recovery_campaign(cohort(), input).expect("campaign");
        assert_eq!(
            report.schedule_observations[1][0].clone(),
            ScheduleObservation::Denied {
                class: String::from("storage-record-schema"),
            }
        );
        assert_eq!(report.progress[0].issue, RecoveryProgressIssue::PeerRepair);
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
    }
}
