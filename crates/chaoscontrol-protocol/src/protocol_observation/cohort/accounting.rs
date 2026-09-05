use super::*;

pub(super) fn validate(
    profile: &AdmittedProfile,
    records: &[AdmittedObservation],
    issues: &mut Vec<CohortIssue>,
) {
    let bounds = &profile.profile.bounds;
    let boundaries = records
        .iter()
        .map(|record| &record.collected.draft.logical_boundary_ref)
        .collect::<std::collections::BTreeSet<_>>();
    if boundaries.len() > bounds.max_logical_boundaries as usize {
        issues.push(issue(CohortIssueKind::BoundExceeded, "logical-boundaries"));
    }
    let total = records.iter().try_fold(0_u64, |total, record| {
        let length = record
            .collected
            .draft
            .projection_bytes
            .as_ref()
            .map_or(0, Vec::len);
        total.checked_add(u64::try_from(length).ok()?)
    });
    if total.is_none_or(|total| total > bounds.max_total_projection_bytes) {
        issues.push(issue(
            CohortIssueKind::BoundExceeded,
            "total-projection-bytes",
        ));
    }
    for producer in &profile.profile.producers {
        let selected: Vec<_> = records
            .iter()
            .filter(|record| record.collected.draft.producer_ref == producer.producer_ref)
            .collect();
        if selected.len() > bounds.max_records_per_producer as usize {
            issues.push(issue(
                CohortIssueKind::BoundExceeded,
                &producer.producer_ref,
            ));
        }
        sequence(&selected, &producer.producer_ref, issues);
    }
}

fn sequence(records: &[&AdmittedObservation], producer: &str, issues: &mut Vec<CohortIssue>) {
    let mut expected = 0;
    let mut previous: Option<&AdmittedObservation> = None;
    let mut final_seen = false;
    for record in records {
        let draft = &record.collected.draft;
        if draft.source_loss_count > 0 {
            issues.push(issue(CohortIssueKind::LossObserved, producer));
        }
        if let Some(prior) = previous {
            if prior.collected.draft.source_sequence == draft.source_sequence {
                // Exact duplicates were removed. Any different field is a conflict.
                issues.push(issue(CohortIssueKind::DuplicateSequence, producer));
                continue;
            }
            if final_seen {
                issues.push(issue(CohortIssueKind::PostFinalRecord, producer));
            }
            if prior.collected.draft.source_loss_count > draft.source_loss_count
                || prior.collected.scheduler_position.guest_exit_sequence
                    > record.collected.scheduler_position.guest_exit_sequence
            {
                issues.push(issue(CohortIssueKind::IdentityDrift, producer));
            }
        }
        if draft.source_sequence != expected {
            issues.push(issue(CohortIssueKind::SequenceGap, producer));
        }
        match draft.source_sequence.checked_add(1) {
            Some(next) => expected = next,
            None => issues.push(issue(CohortIssueKind::BoundExceeded, producer)),
        }
        final_seen |= draft.drain_state == DrainState::Final;
        previous = Some(record);
    }
    if previous.is_none_or(|last| last.collected.draft.drain_state != DrainState::Final) {
        issues.push(issue(CohortIssueKind::FailedFinalDrain, producer));
    }
}

pub(super) fn participants(
    profile: &AdmittedProfile,
    records: &[AdmittedObservation],
    issues: &mut Vec<CohortIssue>,
) {
    for participant in &profile.profile.required_participants {
        let projections = records
            .iter()
            .filter(|record| record.collected.draft.participant_ref == *participant)
            .map(|record| &record.collected.draft.projection_ref)
            .collect::<std::collections::BTreeSet<_>>();
        if projections.is_empty() {
            issues.push(issue(CohortIssueKind::MissingParticipant, participant));
        } else if projections.len() > 1 {
            issues.push(issue(CohortIssueKind::ConflictingProjection, participant));
        }
    }
}

pub(super) fn classify(issues: &[CohortIssue]) -> CohortClassification {
    if issues.iter().any(|item| {
        matches!(
            item.kind,
            CohortIssueKind::ConflictingProjection
                | CohortIssueKind::DuplicateSequence
                | CohortIssueKind::PostFinalRecord
                | CohortIssueKind::GenerationDrift
                | CohortIssueKind::IdentityDrift
        )
    }) {
        return CohortClassification::Conflicting;
    }
    if issues
        .iter()
        .any(|item| item.kind != CohortIssueKind::UnsupportedProjection)
    {
        return CohortClassification::Incomplete;
    }
    if issues.is_empty() {
        CohortClassification::Complete
    } else {
        CohortClassification::Unsupported
    }
}

pub(super) fn issue_for_error(error: ProtocolObservationError) -> CohortIssue {
    let kind = match error {
        ProtocolObservationError::BoundExceeded(_)
        | ProtocolObservationError::CardinalityOverflow => CohortIssueKind::BoundExceeded,
        ProtocolObservationError::IdentityMismatch("generation") => {
            CohortIssueKind::GenerationDrift
        }
        _ => CohortIssueKind::IdentityDrift,
    };
    issue(kind, "rejected-source-record")
}

pub(super) fn issue(kind: CohortIssueKind, subject: &str) -> CohortIssue {
    CohortIssue {
        kind,
        subject: subject.to_string(),
    }
}
