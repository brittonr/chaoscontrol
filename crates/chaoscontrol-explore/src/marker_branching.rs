//! Pure marker projection, prioritization, coverage, and evidence binding.

use chaoscontrol_protocol::branch_marker::{marker_identity, BRANCH_MARKER_EVENT};

pub const MARKER_NOVELTY_BONUS: f64 = 32.0;
pub const MARKER_RARITY_NUMERATOR: f64 = 16.0;
pub const MARKER_REPLAY_SCHEMA: &str = "chaoscontrol.marker-replay-binding.v1";

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerObservation {
    pub run_id: u32,
    pub marker: ::chaoscontrol_protocol::branch_marker::BranchMarker,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerFrontierMetadata {
    pub marker_identity: String,
    pub owner: String,
    pub state_ref: Option<String>,
    pub logical_position_ref: Option<String>,
    pub run_id: u32,
    pub observed_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerCoverageReport {
    pub declared: Vec<String>,
    pub reached: Vec<String>,
    pub gaps: Vec<String>,
    pub limit_events: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerReplayBinding {
    pub schema: String,
    pub marker: MarkerFrontierMetadata,
    pub parent_snapshot_ref: String,
    pub replay_verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerBindingError {
    EmptySnapshotRef,
    IdentityDrift,
    InvalidMarker,
    InvalidSchema,
    UnsupportedReplayVerdict,
}

pub fn observations(
    report: &::chaoscontrol_fault::oracle::OracleReport,
) -> Result<Vec<MarkerObservation>, MarkerBindingError> {
    report
        .events
        .iter()
        .filter(|event| event.name == BRANCH_MARKER_EVENT)
        .map(|event| {
            ::chaoscontrol_protocol::branch_marker::BranchMarker::from_value(&event.details)
                .map(|marker| MarkerObservation {
                    run_id: event.run_id,
                    marker,
                })
                .map_err(|_| MarkerBindingError::InvalidMarker)
        })
        .collect()
}

pub fn frontier_metadata(
    observation: &MarkerObservation,
    observed_tick: u64,
) -> MarkerFrontierMetadata {
    MarkerFrontierMetadata {
        marker_identity: observation.marker.identity.clone(),
        owner: observation.marker.owner.clone(),
        state_ref: observation.marker.state_ref.clone(),
        logical_position_ref: observation.marker.logical_position_ref.clone(),
        run_id: observation.run_id,
        observed_tick,
    }
}

pub fn marker_score(base_score: f64, prior_hits: u32) -> f64 {
    let denominator = f64::from(prior_hits.saturating_add(1));
    let novelty = if prior_hits == 0 {
        MARKER_NOVELTY_BONUS
    } else {
        0.0
    };
    base_score + novelty + (MARKER_RARITY_NUMERATOR / denominator)
}

pub fn oracle_coverage_report(
    report: &::chaoscontrol_fault::oracle::OracleReport,
) -> Result<MarkerCoverageReport, MarkerBindingError> {
    let declared = report.structured_assertions.values().filter_map(|record| {
        let admitted = record.identity.as_ref()?;
        if admitted.descriptor.category
            != ::chaoscontrol_protocol::branch_marker::BRANCH_MARKER_ASSERTION_CATEGORY
        {
            return None;
        }
        let chaoscontrol_protocol::identity::AssertionLogicalKey::Stable { key } =
            &admitted.descriptor.logical_key
        else {
            return None;
        };
        Some(marker_identity(&admitted.descriptor.namespace, key))
    });
    coverage_report(declared, report)
}

pub fn coverage_report(
    declared: impl IntoIterator<Item = String>,
    report: &::chaoscontrol_fault::oracle::OracleReport,
) -> Result<MarkerCoverageReport, MarkerBindingError> {
    let declared = declared
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let reached = observations(report)?
        .into_iter()
        .map(|observation| observation.marker.identity)
        .collect::<std::collections::BTreeSet<_>>();
    let gaps = declared.difference(&reached).cloned().collect::<Vec<_>>();
    let limit_events = report
        .events
        .iter()
        .filter(|event| {
            event.name == ::chaoscontrol_protocol::branch_marker::BRANCH_MARKER_LIMIT_EVENT
        })
        .count();
    Ok(MarkerCoverageReport {
        declared: declared.into_iter().collect(),
        reached: reached.into_iter().collect(),
        gaps,
        limit_events,
    })
}

pub fn bind_replay(
    marker: MarkerFrontierMetadata,
    expected_marker_identity: &str,
    parent_snapshot_ref: &str,
    replay_verdict: &str,
) -> Result<MarkerReplayBinding, MarkerBindingError> {
    if marker.marker_identity != expected_marker_identity {
        return Err(MarkerBindingError::IdentityDrift);
    }
    if parent_snapshot_ref.is_empty() {
        return Err(MarkerBindingError::EmptySnapshotRef);
    }
    if !matches!(
        replay_verdict,
        "snapshot_backed_reproduced" | "snapshot_backed_not_reproduced"
    ) {
        return Err(MarkerBindingError::UnsupportedReplayVerdict);
    }
    Ok(MarkerReplayBinding {
        schema: MARKER_REPLAY_SCHEMA.to_string(),
        marker,
        parent_snapshot_ref: parent_snapshot_ref.to_string(),
        replay_verdict: replay_verdict.to_string(),
    })
}

pub fn validate_replay_binding(binding: &MarkerReplayBinding) -> Result<(), MarkerBindingError> {
    if binding.schema != MARKER_REPLAY_SCHEMA {
        return Err(MarkerBindingError::InvalidSchema);
    }
    let rebound = bind_replay(
        binding.marker.clone(),
        &binding.marker.marker_identity,
        &binding.parent_snapshot_ref,
        &binding.replay_verdict,
    )?;
    if rebound != *binding {
        return Err(MarkerBindingError::IdentityDrift);
    }
    Ok(())
}

pub fn update_hit_counts(
    counts: &mut std::collections::BTreeMap<String, u32>,
    marker_identity: &str,
) -> u32 {
    let prior = counts.get(marker_identity).copied().unwrap_or(0);
    counts.insert(marker_identity.to_string(), prior.saturating_add(1));
    prior
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_fault::oracle::OracleEvent;

    const OBSERVED_TICK: u64 = 41;
    const TEST_ALIAS: u64 = 17;
    const FIRST_PRIOR_HITS: u32 = 0;
    const REPEATED_PRIOR_HITS: u32 = 3;
    const MARKER_TERM: u64 = 3;
    const TEST_DIGEST_HEX_BYTES: usize = 64;

    fn marker() -> ::chaoscontrol_protocol::branch_marker::BranchMarker {
        ::chaoscontrol_protocol::branch_marker::BranchMarker::new(
            "raft",
            "leader-elected",
            "guest-0",
            serde_json::json!({"term": MARKER_TERM}),
            Some(format!("b3:{}", "a".repeat(TEST_DIGEST_HEX_BYTES))),
            Some(format!("term:{MARKER_TERM}")),
        )
        .unwrap()
    }

    fn report_with_marker() -> ::chaoscontrol_fault::oracle::OracleReport {
        let marker = marker();
        ::chaoscontrol_fault::oracle::OracleReport {
            events: vec![OracleEvent {
                run_id: 1,
                name: BRANCH_MARKER_EVENT.to_string(),
                details: serde_json::to_value(marker).unwrap(),
            }],
            ..::chaoscontrol_fault::oracle::OracleReport::empty()
        }
    }

    #[test]
    fn marker_observation_scores_and_binds_replay() {
        let report = report_with_marker();
        let observed = observations(&report).unwrap();
        assert_eq!(observed.len(), 1);
        let metadata = frontier_metadata(&observed[0], OBSERVED_TICK);
        assert!(marker_score(1.0, FIRST_PRIOR_HITS) > marker_score(1.0, REPEATED_PRIOR_HITS));
        let binding = bind_replay(
            metadata.clone(),
            &metadata.marker_identity,
            "snapshots/parent.cbor.zst",
            "snapshot_backed_reproduced",
        )
        .unwrap();
        validate_replay_binding(&binding).unwrap();
    }

    #[test]
    fn catalog_declaration_without_event_is_a_gap() {
        use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogBuilder};
        use chaoscontrol_protocol::identity::{AssertionKind, AssertionLogicalKey};

        let mut descriptor = crate::test_support::assertion_identity(TEST_ALIAS).descriptor;
        descriptor.namespace = "raft".to_string();
        descriptor.logical_key = AssertionLogicalKey::Stable {
            key: "unreached".to_string(),
        };
        descriptor.kind = AssertionKind::Reachable;
        descriptor.category =
            ::chaoscontrol_protocol::branch_marker::BRANCH_MARKER_ASSERTION_CATEGORY.to_string();
        let token = token_for_descriptors(std::slice::from_ref(&descriptor)).unwrap();
        let mut builder = CatalogBuilder::begin(1).unwrap();
        builder.insert(descriptor).unwrap();
        let catalog = builder.complete(token).unwrap();
        let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
        oracle.activate_catalog(catalog).unwrap();
        oracle.begin_run();
        oracle.end_run();

        let coverage = oracle_coverage_report(&oracle.report()).unwrap();
        assert!(coverage.reached.is_empty());
        assert_eq!(coverage.gaps, vec![marker_identity("raft", "unreached")]);
    }

    #[test]
    fn coverage_and_identity_drift_fail_closed() {
        let report = report_with_marker();
        let observed_identity = observations(&report).unwrap()[0].marker.identity.clone();
        let missing = format!("b3:{}", "b".repeat(TEST_DIGEST_HEX_BYTES));
        let coverage =
            coverage_report([observed_identity.clone(), missing.clone()], &report).unwrap();
        assert_eq!(coverage.reached, vec![observed_identity]);
        assert_eq!(coverage.gaps, vec![missing]);
        let metadata = frontier_metadata(&observations(&report).unwrap()[0], OBSERVED_TICK);
        assert_eq!(
            bind_replay(
                metadata,
                "b3:wrong",
                "snapshots/parent.cbor.zst",
                "snapshot_backed_reproduced",
            ),
            Err(MarkerBindingError::IdentityDrift)
        );
    }
}
