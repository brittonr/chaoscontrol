//! Exact receipt projection and bounded status.

use super::*;
const RECEIPT_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.receipt.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    BoundedObservation,
    ProtocolSemantics,
    UniversalCorrectness,
    CrossParticipantWallClockInstant,
    ProductionReadiness,
    ReleaseEligibility,
}

pub fn validate_claim(claim: Claim) -> Result<(), ProtocolObservationError> {
    if claim == Claim::BoundedObservation {
        Ok(())
    } else {
        Err(ProtocolObservationError::ClaimOverreach)
    }
}

pub fn build_receipt(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    oracle_result: Option<ProtocolOracleResult>,
    context: ProtocolEvidenceContext,
) -> Result<ProtocolObservationReceipt, ProtocolObservationError> {
    validate_inputs(profile, cohort, oracle_result.as_ref(), &context)?;
    let mut receipt = ProtocolObservationReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        receipt_ref: String::new(),
        profile_ref: profile.profile_ref.clone(),
        bounds_ref: profile.bounds_ref.clone(),
        protocol_ref: profile.profile.protocol_ref.clone(),
        projection_schema_ref: profile.profile.projection_schema_ref.clone(),
        producer_refs: profile
            .profile
            .producers
            .iter()
            .map(|producer| producer.producer_ref.clone())
            .collect(),
        participant_refs: profile.profile.required_participants.clone(),
        record_refs: cohort
            .records
            .iter()
            .map(|record| record.record_identity.clone())
            .collect(),
        cohort_identity: cohort.cohort_identity.clone(),
        logical_boundary_ref: cohort.logical_boundary_ref.clone(),
        classification: cohort.classification,
        oracle_result,
        novelty_identities: cohort.novelty_identities.clone(),
        marker_binding: context.marker_binding,
        scheduler_state_refs: cohort
            .source_records
            .iter()
            .map(|record| record.scheduler_position.schedule_state_ref.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        fault_refs: sorted_unique(context.fault_refs),
        replay_refs: sorted_unique(context.replay_refs),
        non_claims: profile.profile.non_claims.clone(),
    };
    let bytes =
        serde_json::to_vec(&receipt).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    receipt.receipt_ref = digest_reference("protocol-observation-receipt", RECEIPT_DOMAIN, &bytes);
    Ok(receipt)
}

fn validate_inputs(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    oracle: Option<&ProtocolOracleResult>,
    context: &ProtocolEvidenceContext,
) -> Result<(), ProtocolObservationError> {
    validate_cohort(profile, cohort)?;
    match (cohort.classification, oracle) {
        (CohortClassification::Complete, Some(result)) => {
            validate_oracle_result(profile, cohort, result)?
        }
        (CohortClassification::Complete, None) => {
            return Err(ProtocolObservationError::OracleMismatch)
        }
        (_, Some(_)) => return Err(ProtocolObservationError::CohortNotComplete),
        (_, None) => {}
    }
    if let Some(binding) = &context.marker_binding {
        validate_marker_binding(profile, cohort, binding)?;
    }
    for (refs, prefix) in [
        (&context.fault_refs, "fault"),
        (&context.replay_refs, "replay"),
    ] {
        if refs.len() > profile.profile.bounds.max_diagnostic_refs as usize {
            return Err(ProtocolObservationError::BoundExceeded("receipt-context"));
        }
        for value in refs {
            validate_exact_reference(value, prefix)?;
        }
    }
    Ok(())
}

pub fn validate_receipt(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    receipt: &ProtocolObservationReceipt,
) -> Result<(), ProtocolObservationError> {
    let expected = build_receipt(
        profile,
        cohort,
        receipt.oracle_result.clone(),
        ProtocolEvidenceContext {
            marker_binding: receipt.marker_binding.clone(),
            fault_refs: receipt.fault_refs.clone(),
            replay_refs: receipt.replay_refs.clone(),
        },
    )?;
    if *receipt != expected {
        return Err(ProtocolObservationError::IdentityMismatch("receipt"));
    }
    Ok(())
}

pub fn build_status(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    oracle: Option<&ProtocolOracleResult>,
    marker: Option<&MarkerSnapshotBinding>,
) -> Result<ProtocolObservationStatus, ProtocolObservationError> {
    validate_cohort(profile, cohort)?;
    if let Some(result) = oracle {
        validate_oracle_result(profile, cohort, result)?;
    }
    if let Some(binding) = marker {
        validate_marker_binding(profile, cohort, binding)?;
    }
    let observed = cohort
        .records
        .iter()
        .map(|record| record.collected.draft.participant_ref.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let missing_participants = profile
        .profile
        .required_participants
        .iter()
        .filter(|participant| !observed.contains(participant.as_str()))
        .cloned()
        .collect();
    Ok(ProtocolObservationStatus {
        classification: cohort.classification,
        required_participants: profile.profile.required_participants.len(),
        observed_participants: observed.len(),
        missing_participants,
        sequence_gap_count: count_issue(cohort, CohortIssueKind::SequenceGap),
        loss_count: count_issue(cohort, CohortIssueKind::LossObserved),
        conflict_count: cohort
            .issues
            .iter()
            .filter(|issue| {
                matches!(
                    issue.kind,
                    CohortIssueKind::ConflictingProjection
                        | CohortIssueKind::DuplicateSequence
                        | CohortIssueKind::PostFinalRecord
                        | CohortIssueKind::GenerationDrift
                        | CohortIssueKind::IdentityDrift
                )
            })
            .count(),
        oracle_verdict: oracle.map(|result| result.decision.verdict),
        novelty_count: cohort.novelty_identities.len(),
        marker_reachability: if marker.is_some() {
            MarkerReachability::IdentityLinked
        } else {
            MarkerReachability::NotBound
        },
        blocker_kinds: cohort
            .issues
            .iter()
            .map(|issue| issue.kind)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
    })
}

fn count_issue(cohort: &CohortResult, kind: CohortIssueKind) -> usize {
    cohort
        .issues
        .iter()
        .filter(|item| item.kind == kind)
        .count()
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
