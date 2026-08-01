mod assertion_identity_transport_support;

use assertion_identity_transport_support::*;
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::oracle::Verdict;
use chaoscontrol_protocol::identity::{
    AssertionFingerprint, AssertionKind, ASSERTION_FINGERPRINT_BYTES,
};
use chaoscontrol_protocol::transport::EVENT_KIND_OFFSET;
use chaoscontrol_protocol::{
    CMD_ASSERT_ALWAYS, STATUS_ASSERTION_EVENT_REJECTED, STATUS_ASSERTION_FAILED,
    STATUS_ASSERTION_IDENTITY_CONFLICT, STATUS_OK,
};

#[test]
fn accepted_catalog_binds_event_and_snapshot_state() {
    let value = descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    let token = admit(&mut engine, &value);
    engine.begin_run();
    assert_eq!(
        engine.handle_hypercall(&event_page(&value, token, true)).1,
        STATUS_OK
    );
    engine.end_run();
    let snapshot = engine.snapshot();
    let fingerprint = value.fingerprint().expect("fingerprint");
    let report = engine.oracle().report();
    assert!(report.collision_safe_evidence);
    assert_eq!(
        report.structured_assertions[&fingerprint].verdict(),
        Verdict::Passed
    );

    engine.restore(&snapshot).expect("restore snapshot");
    let restored = engine.oracle().report();
    assert!(restored.collision_safe_evidence);
    assert_eq!(restored.structured_assertions[&fingerprint].hit_count, 1);
}

#[test]
fn accepted_event_without_active_run_is_rejected_before_counter_mutation() {
    let value = descriptor();
    let fingerprint = value.fingerprint().expect("fingerprint");
    let mut engine = FaultEngine::new(EngineConfig::default());
    let token = admit(&mut engine, &value);
    let before = engine.oracle().structured_assertions()[&fingerprint].clone();

    assert_eq!(
        engine.handle_hypercall(&event_page(&value, token, true)).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    assert_eq!(
        engine.oracle().structured_assertions()[&fingerprint],
        before
    );
}

#[test]
fn missing_completion_and_unknown_identity_reject_events() {
    let value = descriptor();
    let mut incomplete = FaultEngine::new(EngineConfig::default());
    assert_eq!(incomplete.handle_hypercall(&begin_page(1)).1, STATUS_OK);
    assert_eq!(
        incomplete
            .handle_hypercall(&event_page(&value, AssertionFingerprint::ZERO, true))
            .1,
        STATUS_ASSERTION_EVENT_REJECTED
    );

    let mut unknown = FaultEngine::new(EngineConfig::default());
    let token = admit(&mut unknown, &value);
    let mut event = event_page(&value, token, true);
    const FINGERPRINT_PAYLOAD_OFFSET: usize = 1 + ASSERTION_FINGERPRINT_BYTES;
    event.payload[FINGERPRINT_PAYLOAD_OFFSET] ^= u8::MAX;
    assert_eq!(
        unknown.handle_hypercall(&event).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    assert!(!unknown.oracle().report().collision_safe_evidence);
}

#[test]
fn mismatched_token_kind_spoof_and_post_conflict_events_fail_closed() {
    let value = descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    let token = admit(&mut engine, &value);
    let mut mismatch = event_page(&value, token, true);
    const CATALOG_TOKEN_PAYLOAD_OFFSET: usize = 1;
    mismatch.payload[CATALOG_TOKEN_PAYLOAD_OFFSET] ^= u8::MAX;
    assert_eq!(
        engine.handle_hypercall(&mismatch).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );

    let mut spoof = event_page(&value, token, true);
    spoof.payload[EVENT_KIND_OFFSET] = AssertionKind::Unreachable as u8;
    assert_eq!(
        engine.handle_hypercall(&spoof).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    assert_eq!(
        engine.handle_hypercall(&event_page(&value, token, false)).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
}

#[test]
fn catalog_metadata_conflict_is_fatal_before_runtime() {
    let value = descriptor();
    let mut conflict = value.clone();
    conflict.message = "different property".to_string();
    let mut engine = FaultEngine::new(EngineConfig::default());
    assert_eq!(engine.handle_hypercall(&begin_page(2)).1, STATUS_OK);
    assert_eq!(
        engine.handle_hypercall(&descriptor_page(&value)).1,
        STATUS_OK
    );
    assert_eq!(
        engine.handle_hypercall(&descriptor_page(&conflict)).1,
        STATUS_ASSERTION_IDENTITY_CONFLICT
    );
    assert!(!engine.oracle().report().collision_safe_evidence);
}

#[test]
fn redundant_transport_identity_fields_must_match() {
    let value = descriptor();
    let mut descriptor_spoof = FaultEngine::new(EngineConfig::default());
    assert_eq!(
        descriptor_spoof.handle_hypercall(&begin_page(1)).1,
        STATUS_OK
    );
    let mut wrong_descriptor_id = descriptor_page(&value);
    wrong_descriptor_id.id = SPOOFED_ID;
    assert_eq!(
        descriptor_spoof.handle_hypercall(&wrong_descriptor_id).1,
        STATUS_ASSERTION_IDENTITY_CONFLICT
    );

    let mut missing_id_descriptor = value.clone();
    missing_id_descriptor.compatibility_id = None;
    let mut missing_id_engine = FaultEngine::new(EngineConfig::default());
    assert_eq!(
        missing_id_engine.handle_hypercall(&begin_page(1)).1,
        STATUS_OK
    );
    let missing_id_status = missing_id_engine
        .handle_hypercall(&descriptor_page(&missing_id_descriptor))
        .1;
    assert_eq!(missing_id_status, STATUS_ASSERTION_IDENTITY_CONFLICT);

    let mut completion_spoof = FaultEngine::new(EngineConfig::default());
    assert_eq!(
        completion_spoof.handle_hypercall(&begin_page(1)).1,
        STATUS_OK
    );
    assert_eq!(
        completion_spoof
            .handle_hypercall(&descriptor_page(&value))
            .1,
        STATUS_OK
    );
    let mut wrong_completion_count = complete_page(&value);
    wrong_completion_count.id = SPOOFED_ID;
    assert_eq!(
        completion_spoof.handle_hypercall(&wrong_completion_count).1,
        STATUS_ASSERTION_IDENTITY_CONFLICT
    );

    let mut event_spoof = FaultEngine::new(EngineConfig::default());
    let token = admit(&mut event_spoof, &value);
    let mut wrong_event_id = event_page(&value, token, true);
    wrong_event_id.id = SPOOFED_ID;
    assert_eq!(
        event_spoof.handle_hypercall(&wrong_event_id).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    assert!(!event_spoof.oracle().report().collision_safe_evidence);
}

#[test]
fn unbound_strict_event_fails_closed_without_legacy_fallback() {
    let mut engine = FaultEngine::new(EngineConfig::default());
    let unbound = page(CMD_ASSERT_ALWAYS, TRUE_FLAG, COMPATIBILITY_ID, &[]);
    assert_eq!(
        engine.handle_hypercall(&unbound).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    assert!(!engine.oracle().report().collision_safe_evidence);
}

#[test]
fn bound_false_always_event_keeps_failure_semantics() {
    let value = descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    let token = admit(&mut engine, &value);
    engine.begin_run();
    assert_eq!(
        engine.handle_hypercall(&event_page(&value, token, false)).1,
        STATUS_ASSERTION_FAILED
    );
    assert!(engine.has_assertion_failure());
}
