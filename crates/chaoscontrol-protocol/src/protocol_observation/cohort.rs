//! Deterministic bounded cohort assembly and replay validation.

#[path = "cohort/accounting.rs"]
mod accounting;
use super::*;

pub const MAX_COLLECTED_RECORD_BYTES: usize = 16_384;

pub fn assemble(
    profile: &AdmittedProfile,
    logical_boundary_ref: &str,
    collected: &[CollectedObservation],
    projection_support: ProjectionSupport,
) -> Result<CohortResult, ProtocolObservationError> {
    assemble_with_losses(
        profile,
        logical_boundary_ref,
        collected,
        projection_support,
        0,
    )
}

pub fn assemble_with_losses(
    profile: &AdmittedProfile,
    logical_boundary_ref: &str,
    collected: &[CollectedObservation],
    projection_support: ProjectionSupport,
    host_loss_count: u64,
) -> Result<CohortResult, ProtocolObservationError> {
    validate_profile_identity(profile)?;
    validate_exact_reference(logical_boundary_ref, "logical-boundary")?;
    if collected.len() > profile.profile.bounds.max_cohort_backlog as usize {
        return Err(ProtocolObservationError::BoundExceeded("cohort-backlog"));
    }
    let mut source_records = Vec::with_capacity(collected.len());
    let mut admitted = Vec::with_capacity(collected.len());
    let mut issues = Vec::new();
    for candidate in collected {
        bounded_record(candidate)?;
        source_records.push(candidate.clone());
        match admit_observation(profile, candidate.clone()) {
            Ok(record) => admitted.push(record),
            Err(error) => issues.push(accounting::issue_for_error(error)),
        }
    }
    source_records.sort_by_cached_key(record_digest);
    source_records.dedup();
    admitted.sort_by(|left, right| {
        let key = |record: &AdmittedObservation| {
            (
                record.collected.draft.source_sequence,
                record.record_identity.clone(),
            )
        };
        left.collected
            .draft
            .producer_ref
            .cmp(&right.collected.draft.producer_ref)
            .then_with(|| key(left).cmp(&key(right)))
    });
    admitted.dedup();
    accounting::validate(profile, &admitted, &mut issues);
    if host_loss_count > 0 {
        issues.push(accounting::issue(
            CohortIssueKind::LossObserved,
            "host-collection",
        ));
    }
    let records: Vec<_> = admitted
        .into_iter()
        .filter(|record| record.collected.draft.logical_boundary_ref == logical_boundary_ref)
        .collect();
    accounting::participants(profile, &records, &mut issues);
    if projection_support == ProjectionSupport::Unavailable {
        issues.push(accounting::issue(
            CohortIssueKind::UnsupportedProjection,
            "projection-schema",
        ));
    }
    issues.sort_by(|left, right| (left.kind, &left.subject).cmp(&(right.kind, &right.subject)));
    issues.dedup();
    let novelty_identities = records
        .iter()
        .map(|record| record.collected.draft.novelty_identity.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut result = CohortResult {
        cohort_identity: String::new(),
        profile_ref: profile.profile_ref.clone(),
        projection_support,
        source_records,
        host_loss_count,
        cohort_ref: profile.profile.cohort_ref.clone(),
        logical_boundary_ref: logical_boundary_ref.to_string(),
        classification: accounting::classify(&issues),
        records,
        issues,
        novelty_identities,
    };
    result.cohort_identity = identity(&result)?;
    Ok(result)
}

pub fn validate(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
) -> Result<(), ProtocolObservationError> {
    let rebuilt = assemble_with_losses(
        profile,
        &cohort.logical_boundary_ref,
        &cohort.source_records,
        cohort.projection_support,
        cohort.host_loss_count,
    )?;
    if rebuilt != *cohort {
        return Err(ProtocolObservationError::IdentityMismatch("cohort"));
    }
    Ok(())
}

fn identity(cohort: &CohortResult) -> Result<String, ProtocolObservationError> {
    let mut body = cohort.clone();
    body.cohort_identity.clear();
    let bytes = serde_json::to_vec(&body).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    Ok(digest_reference(
        "protocol-cohort",
        COHORT_IDENTITY_DOMAIN,
        &bytes,
    ))
}

fn record_digest(record: &CollectedObservation) -> String {
    // The bounded record contains only total Serde data types.
    let bytes = serde_json::to_vec(record).expect("bounded protocol record serializes");
    digest_reference("protocol-source", RECORD_IDENTITY_DOMAIN, &bytes)
}

/// Bound serialization before cloning an untrusted record.
pub fn bounded_record(record: &CollectedObservation) -> Result<(), ProtocolObservationError> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let total = self
                .0
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_COLLECTED_RECORD_BYTES)
                .ok_or_else(|| std::io::Error::other("protocol record exceeds byte limit"))?;
            self.0 = total;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), record)
        .map_err(|_| ProtocolObservationError::BoundExceeded("record-bytes"))
}
