#[path = "protocol/replay.rs"]
mod replay;

use chaoscontrol_explore::coverage::CoverageBitmap;
use chaoscontrol_explore::protocol_observation::{
    collect_cohort, enrich_coverage, validate_replay,
};
use chaoscontrol_fault::protocol_collection::Collection;
use chaoscontrol_protocol::protocol_observation::*;

struct Oracle;
impl ProtocolOracle for Oracle {
    fn adapter_ref(&self) -> &str {
        "oracle-adapter:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    }
    fn projection_schema_ref(&self) -> &str {
        "projection-schema:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
    fn authority(&self) -> OracleAuthority {
        OracleAuthority::ConsumerIndependent
    }
    fn evaluate(
        &self,
        _: &CohortResult,
        _: u32,
    ) -> Result<OracleDecision, ProtocolObservationError> {
        Ok(OracleDecision {
            verdict: ProtocolVerdict::Pass,
            diagnostic_refs: Vec::new(),
            work_items: 1,
        })
    }
}

fn profile() -> AdmittedProfile {
    decode_profile(include_bytes!(
        "../../../contracts/protocol-observation/fixtures/valid.json"
    ))
    .unwrap()
}

fn record(profile: &AdmittedProfile, index: usize) -> CollectedObservation {
    let producer = &profile.profile.producers[index];
    let projection = b"{}".to_vec();
    let boundary = format!("logical-boundary:{}", "a".repeat(BLAKE3_HEX_BYTES));
    let projection_ref = projection_identity(&projection);
    let novelty_identity = novelty_identity(profile, &projection_ref, &boundary, "final");
    bind_scheduler_position(
        ObservationDraft {
            schema: DRAFT_SCHEMA.into(),
            profile_ref: profile.profile_ref.clone(),
            protocol_ref: profile.profile.protocol_ref.clone(),
            cohort_ref: profile.profile.cohort_ref.clone(),
            producer_ref: producer.producer_ref.clone(),
            participant_ref: producer.participant_ref.clone(),
            process_ref: producer.process_ref.clone(),
            execution_ref: profile.profile.execution_ref.clone(),
            generation: producer.admitted_generation,
            source_sequence: 0,
            source_loss_count: 0,
            drain_state: DrainState::Final,
            transition_class: "final".into(),
            logical_boundary_ref: boundary,
            projection_schema_ref: profile.profile.projection_schema_ref.clone(),
            projection_ref,
            projection_bytes: Some(projection),
            novelty_identity,
            marker_identity: None,
            parent_snapshot_ref: None,
        },
        SchedulerPosition {
            vm_id: producer.vm_id,
            active_vcpu: 0,
            guest_exit_sequence: 1,
            schedule_state_ref: format!("schedule-state:{}", "b".repeat(BLAKE3_HEX_BYTES)),
        },
    )
    .unwrap()
}

#[test]
fn cross_vm_collection_preserves_full_novelty_and_rejects_replay_drift() {
    let profile = profile();
    let mut first = Collection::default();
    let mut second = Collection::default();
    first.configure(profile.clone(), &Oracle).unwrap();
    second.configure(profile.clone(), &Oracle).unwrap();
    let boundary = record(&profile, 0).draft.logical_boundary_ref;
    first.collect(record(&profile, 0)).unwrap();
    second.collect(record(&profile, 1)).unwrap();
    let expected = collect_cohort(&profile, &boundary, &[&first, &second]).unwrap();
    let reversed = collect_cohort(&profile, &boundary, &[&second, &first]).unwrap();
    assert_eq!(expected, reversed);
    let mut bitmap = CoverageBitmap::new();
    let identities = enrich_coverage(&profile, &expected, &mut bitmap).unwrap();
    assert_eq!(identities, expected.novelty_identities);
    assert!(bitmap.count_bits() > 0);
    validate_replay(&profile, &expected, &reversed).unwrap();
    second.reject();
    let missing = collect_cohort(&profile, &boundary, &[&first, &second]).unwrap();
    assert!(validate_replay(&profile, &expected, &missing).is_err());
    let before = bitmap.as_slice().to_vec();
    assert!(enrich_coverage(&profile, &missing, &mut bitmap).is_err());
    assert_eq!(bitmap.as_slice(), before);
}

#[test]
fn forged_collection_and_incomplete_marker_never_create_guidance() {
    let profile = profile();
    let empty = Collection::default();
    let boundary = record(&profile, 0).draft.logical_boundary_ref;
    assert!(collect_cohort(&profile, &boundary, &[&empty]).is_err());
    let mut configured = Collection::default();
    configured.configure(profile.clone(), &Oracle).unwrap();
    configured.collect(record(&profile, 0)).unwrap();
    let cohort = collect_cohort(&profile, &boundary, &[&configured]).unwrap();
    assert_eq!(cohort.classification, CohortClassification::Incomplete);
    let mut bitmap = CoverageBitmap::new();
    assert!(enrich_coverage(&profile, &cohort, &mut bitmap).is_err());
    assert_eq!(bitmap.count_bits(), 0);
    let mut changed = serde_json::to_value(&configured).unwrap();
    changed["profile"]["oracle"]["authority"] = "runtime-self-report".into();
    let forged: Collection = serde_json::from_value(changed).unwrap();
    assert!(configured.admit_snapshot(&forged).is_err());
}
