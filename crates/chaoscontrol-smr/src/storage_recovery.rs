//! Pure storage-recovery safety and progress decisions for the Raft workload.
//!
//! Facts are supplied by the workload adapter; this module performs no I/O and
//! does not decide Raft commitment or repair authority.
//!
//! r[impl chaoscontrol.smr_storage_recovery.protocol_oracles]
//! r[impl chaoscontrol.smr_storage_recovery.recovery_progress]

use serde::{Deserialize, Serialize};

use crate::{SmrError, SmrResult};

fn deny(class: &'static str, detail: impl Into<String>) -> SmrError {
    SmrError::new(class, detail)
}

/// One participant's durable and observed protocol facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageRecoveryObservation {
    pub participant: String,
    pub term: u64,
    pub durable_last_index: u64,
    pub acknowledged_last_index: u64,
    pub committed_last_index: u64,
    pub committed_term: u64,
    pub is_voting: bool,
    pub is_local_sufficient: bool,
    pub is_repair_completed: bool,
    pub is_commitment_known: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageSafetyFindingClass {
    AcknowledgedDataLoss,
    CommittedEntryTruncation,
    UnsafePostRepairVoting,
    ConflictingCommittedHistory,
    UnknownCommitmentProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageSafetyFinding {
    pub participant: String,
    pub class: StorageSafetyFindingClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageSafetyReport {
    pub findings: Vec<StorageSafetyFinding>,
    pub is_pass: bool,
}

/// Reject unsafe storage observations. Fail-closed unavailability is not a finding.
pub fn validate_storage_recovery(
    observations: &[StorageRecoveryObservation],
) -> SmrResult<StorageSafetyReport> {
    if observations.is_empty() {
        return Err(deny(
            "storage-recovery-empty",
            "no participant observations were supplied",
        ));
    }
    let mut findings = Vec::new();
    for observation in observations {
        let mut record = |class| {
            findings.push(StorageSafetyFinding {
                participant: observation.participant.clone(),
                class,
            });
        };
        if observation.durable_last_index < observation.committed_last_index {
            record(StorageSafetyFindingClass::CommittedEntryTruncation);
        }
        if observation.durable_last_index < observation.acknowledged_last_index {
            record(StorageSafetyFindingClass::AcknowledgedDataLoss);
        }
        if !observation.is_commitment_known && observation.is_repair_completed {
            record(StorageSafetyFindingClass::UnknownCommitmentProgress);
        }
        if observation.is_voting
            && !observation.is_local_sufficient
            && !observation.is_repair_completed
        {
            record(StorageSafetyFindingClass::UnsafePostRepairVoting);
        }
    }
    for (index, left) in observations.iter().enumerate() {
        for right in &observations[index + 1..] {
            let is_same_index = left.committed_last_index == right.committed_last_index;
            let is_conflicting = left.committed_term != right.committed_term;
            if is_same_index && is_conflicting && left.committed_last_index > 0 {
                for participant in [&left.participant, &right.participant] {
                    findings.push(StorageSafetyFinding {
                        participant: participant.clone(),
                        class: StorageSafetyFindingClass::ConflictingCommittedHistory,
                    });
                }
            }
        }
    }
    let is_pass = findings.is_empty();
    Ok(StorageSafetyReport { findings, is_pass })
}

/// Participant-scoped recovery progress facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryProgressInput {
    pub participant: String,
    pub required_index: u64,
    pub is_local_sufficient: bool,
    pub admitted_peers: Vec<String>,
    pub peer_available: Vec<String>,
    pub repair_source: Option<String>,
    pub is_repair_completed: bool,
    pub is_fault_disrupted: bool,
    pub source_sequence: u64,
    pub loss_count: u64,
    pub is_final_drain_complete: bool,
    pub virtual_horizon: u64,
    pub is_horizon_reached: bool,
    pub is_commitment_known: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryProgressVerdict {
    Pass,
    Fail,
    NotEvaluated,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryProgressIssue {
    LocalRepair,
    PeerRepair,
    UnnecessaryRemoteRepair,
    MissedAvailableRepair,
    GlobalAbsence,
    IncompleteObservation,
    HorizonNotReached,
    UnknownCommitment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryProgressReport {
    pub participant: String,
    pub verdict: RecoveryProgressVerdict,
    pub issue: RecoveryProgressIssue,
}

/// Evaluate one participant without granting repair or commitment authority.
pub fn evaluate_recovery_progress(
    input: &RecoveryProgressInput,
) -> SmrResult<RecoveryProgressReport> {
    if input.participant.is_empty() || input.required_index == 0 {
        return Err(deny(
            "storage-recovery-input",
            "participant and required index must be supplied",
        ));
    }
    if !input.is_commitment_known {
        return Ok(RecoveryProgressReport {
            participant: input.participant.clone(),
            verdict: RecoveryProgressVerdict::NotEvaluated,
            issue: RecoveryProgressIssue::UnknownCommitment,
        });
    }
    let report = |verdict, issue| RecoveryProgressReport {
        participant: input.participant.clone(),
        verdict,
        issue,
    };
    let repair_from_admitted = input
        .repair_source
        .as_ref()
        .is_some_and(|source| input.admitted_peers.contains(source));
    if input.is_local_sufficient {
        if input.repair_source.is_some() {
            return Ok(report(
                RecoveryProgressVerdict::Fail,
                RecoveryProgressIssue::UnnecessaryRemoteRepair,
            ));
        }
        return Ok(report(
            RecoveryProgressVerdict::Pass,
            RecoveryProgressIssue::LocalRepair,
        ));
    }
    if input.is_repair_completed && repair_from_admitted {
        return Ok(report(
            RecoveryProgressVerdict::Pass,
            RecoveryProgressIssue::PeerRepair,
        ));
    }
    if !input.peer_available.is_empty()
        && !(input.is_final_drain_complete && input.is_horizon_reached)
    {
        return Ok(report(
            RecoveryProgressVerdict::Incomplete,
            RecoveryProgressIssue::HorizonNotReached,
        ));
    }
    if input.peer_available.is_empty()
        && input.is_final_drain_complete
        && input.is_horizon_reached
        && !input.admitted_peers.is_empty()
    {
        return Ok(report(
            RecoveryProgressVerdict::Pass,
            RecoveryProgressIssue::GlobalAbsence,
        ));
    }
    if !input.peer_available.is_empty() {
        return Ok(report(
            RecoveryProgressVerdict::Fail,
            RecoveryProgressIssue::MissedAvailableRepair,
        ));
    }
    if !input.is_final_drain_complete {
        return Ok(report(
            RecoveryProgressVerdict::Incomplete,
            RecoveryProgressIssue::IncompleteObservation,
        ));
    }
    Ok(report(
        RecoveryProgressVerdict::Fail,
        RecoveryProgressIssue::MissedAvailableRepair,
    ))
}

/// Stable non-claim boundary for storage-recovery evidence.
pub const STORAGE_RECOVERY_NON_CLAIMS: [&str; 4] = [
    "not proof of arbitrary device or filesystem behavior",
    "not proof of Raft correctness",
    "not permission to rejoin or vote",
    "not release eligibility",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(index: u64) -> StorageRecoveryObservation {
        StorageRecoveryObservation {
            participant: format!("node-{index}"),
            term: 2,
            durable_last_index: 5,
            acknowledged_last_index: 5,
            committed_last_index: 5,
            committed_term: 2,
            is_voting: true,
            is_local_sufficient: true,
            is_repair_completed: false,
            is_commitment_known: true,
        }
    }

    fn progress() -> RecoveryProgressInput {
        RecoveryProgressInput {
            participant: "node-0".to_string(),
            required_index: 5,
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
        }
    }

    #[test]
    fn consistent_durable_state_passes_the_safety_oracle() {
        let report = validate_storage_recovery(&[observation(0), observation(1)]).expect("report");
        assert!(report.is_pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn truncation_loss_and_unsafe_voting_are_each_reported() {
        let mut truncated = observation(0);
        truncated.durable_last_index = 4;
        let report = validate_storage_recovery(&[truncated]).expect("report");
        assert_eq!(
            report.findings[0].class,
            StorageSafetyFindingClass::CommittedEntryTruncation
        );

        let mut acknowledged = observation(0);
        acknowledged.acknowledged_last_index = 6;
        acknowledged.committed_last_index = 5;
        acknowledged.durable_last_index = 5;
        let report = validate_storage_recovery(&[acknowledged]).expect("report");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.class == StorageSafetyFindingClass::AcknowledgedDataLoss));

        let mut unsafe_vote = observation(0);
        unsafe_vote.is_local_sufficient = false;
        let report = validate_storage_recovery(&[unsafe_vote]).expect("report");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.class == StorageSafetyFindingClass::UnsafePostRepairVoting));
    }

    #[test]
    fn conflicting_committed_histories_are_rejected() {
        let mut left = observation(0);
        left.committed_term = 3;
        let right = observation(1);
        let report = validate_storage_recovery(&[left, right]).expect("report");
        assert!(!report.is_pass);
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.class
                    == StorageSafetyFindingClass::ConflictingCommittedHistory)
        );
    }

    #[test]
    fn unknown_commitment_is_not_evaluated() {
        let mut input = progress();
        input.is_commitment_known = false;
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::NotEvaluated);
        assert_eq!(report.issue, RecoveryProgressIssue::UnknownCommitment);
    }

    #[test]
    fn sufficient_local_state_never_requires_remote_repair() {
        let report = evaluate_recovery_progress(&progress()).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
        assert_eq!(report.issue, RecoveryProgressIssue::LocalRepair);

        let mut input = progress();
        input.repair_source = Some("node-1".to_string());
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Fail);
        assert_eq!(report.issue, RecoveryProgressIssue::UnnecessaryRemoteRepair);
    }

    #[test]
    fn admitted_peer_repair_passes_and_missed_repair_fails() {
        let mut input = progress();
        input.is_local_sufficient = false;
        input.repair_source = Some("node-1".to_string());
        input.is_repair_completed = true;
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
        assert_eq!(report.issue, RecoveryProgressIssue::PeerRepair);

        let mut input = progress();
        input.is_local_sufficient = false;
        input.peer_available = vec!["node-1".to_string()];
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Fail);
        assert_eq!(report.issue, RecoveryProgressIssue::MissedAvailableRepair);
    }

    #[test]
    fn incomplete_observation_and_unreached_horizon_are_incomplete() {
        let mut input = progress();
        input.is_local_sufficient = false;
        input.peer_available = vec!["node-1".to_string()];
        input.is_horizon_reached = false;
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Incomplete);
        assert_eq!(report.issue, RecoveryProgressIssue::HorizonNotReached);

        let mut input = progress();
        input.is_local_sufficient = false;
        input.is_final_drain_complete = false;
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Incomplete);
        assert_eq!(report.issue, RecoveryProgressIssue::IncompleteObservation);
    }

    #[test]
    fn complete_global_absence_is_fail_closed_unavailability() {
        let mut input = progress();
        input.is_local_sufficient = false;
        let report = evaluate_recovery_progress(&input).expect("report");
        assert_eq!(report.verdict, RecoveryProgressVerdict::Pass);
        assert_eq!(report.issue, RecoveryProgressIssue::GlobalAbsence);
    }
}
