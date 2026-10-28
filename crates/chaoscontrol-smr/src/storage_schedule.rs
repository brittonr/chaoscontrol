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
            RecoveryScheduleStep::Restart { node } => {
                let published_index = cohort.node(*node)?.load()?.map(|state| last_index(&state));
                observations.push(ScheduleObservation::Restarted {
                    node: *node,
                    published_index,
                });
            }
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
}
