#![allow(dead_code)]

mod assertion_identity_transport_support;

use assertion_identity_transport_support::*;
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine, MAX_ASSERTION_HIT_EVENTS};
use chaoscontrol_protocol::identity::{AssertionKind, ASSERTION_IDENTITY_VERSION};
use chaoscontrol_protocol::transport::{encode_event_frame, EventFrame};
use chaoscontrol_protocol::{
    HypercallPage, CMD_ASSERT_SOMETIMES, PAYLOAD_MAX, STATUS_ASSERTION_EVENT_REJECTED, STATUS_OK,
};

fn sometimes_descriptor() -> chaoscontrol_protocol::identity::AssertionDescriptor {
    let mut value = descriptor();
    value.kind = AssertionKind::Sometimes;
    value.identity_version = ASSERTION_IDENTITY_VERSION;
    value
}

fn sometimes_page(
    value: &chaoscontrol_protocol::identity::AssertionDescriptor,
    token: chaoscontrol_protocol::identity::AssertionFingerprint,
    condition: bool,
) -> HypercallPage {
    let frame = EventFrame {
        catalog_token: token,
        fingerprint: value.fingerprint().expect("fingerprint"),
        kind: AssertionKind::Sometimes,
        details: br#"{"chaoscontrol_process_identity":"process:v1:fixture"}"#.to_vec(),
    };
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_event_frame(&frame, &mut payload).expect("event frame");
    page(
        CMD_ASSERT_SOMETIMES,
        u8::from(condition),
        value.compatibility_id.expect("compatibility id"),
        &payload[..length],
    )
}

#[test]
fn accepted_sometimes_hit_is_queued_then_drained() {
    let value = sometimes_descriptor();
    let fingerprint = value.fingerprint().expect("fingerprint");
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.begin_run();
    let token = admit(&mut engine, &value);

    assert_eq!(
        engine
            .handle_hypercall(&sometimes_page(&value, token, true))
            .1,
        STATUS_OK
    );

    let hits = engine.take_assertion_hits();
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.fingerprint, fingerprint);
    assert_eq!(hit.kind, AssertionKind::Sometimes);
    assert!(hit.condition);
    assert_eq!(hit.guest, "test-guest");
    assert_eq!(hit.category, "invariant");
    assert_eq!(hit.compatibility_id, value.compatibility_id);
    assert_eq!(hit.run_sequence, 1);
    assert!(hit.run_id.0.iter().any(|byte| *byte != 0));
    assert_eq!(hit.process_identity.as_deref(), Some("process:v1:fixture"));
    assert!(engine.take_assertion_hits().is_empty());
}

#[test]
fn rejected_event_does_not_queue_a_hit() {
    let value = sometimes_descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.begin_run();
    let token = admit(&mut engine, &value);
    let mut page = sometimes_page(&value, token, true);
    page.payload[1] ^= u8::MAX;

    assert_eq!(
        engine.handle_hypercall(&page).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    assert!(engine.take_assertion_hits().is_empty());
    assert_eq!(engine.assertion_hit_overflow(), 0);
}

#[test]
fn hit_queue_is_bounded_and_reports_overflow() {
    let value = sometimes_descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.begin_run();
    let token = admit(&mut engine, &value);
    let page = sometimes_page(&value, token, true);

    for _ in 0..=MAX_ASSERTION_HIT_EVENTS {
        assert_eq!(engine.handle_hypercall(&page).1, STATUS_OK);
    }
    let hits = engine.take_assertion_hits();
    assert_eq!(hits.len(), MAX_ASSERTION_HIT_EVENTS);
    assert_eq!(engine.assertion_hit_overflow(), 1);
}

#[test]
fn failure_status_hits_are_queued_for_boundary_drain() {
    let value = descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.begin_run();
    let token = admit(&mut engine, &value);

    assert_eq!(
        engine.handle_hypercall(&event_page(&value, token, false)).1,
        chaoscontrol_protocol::STATUS_ASSERTION_FAILED
    );
    let hits = engine.take_assertion_hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, AssertionKind::Always);
    assert!(!hits[0].condition);
}

#[test]
fn snapshot_restore_does_not_replay_queued_hits() {
    let value = sometimes_descriptor();
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.begin_run();
    let token = admit(&mut engine, &value);
    assert_eq!(
        engine
            .handle_hypercall(&sometimes_page(&value, token, true))
            .1,
        STATUS_OK
    );
    let snapshot = engine.snapshot();
    engine.restore(&snapshot).expect("restore");

    assert!(engine.take_assertion_hits().is_empty());
    assert_eq!(engine.assertion_hit_overflow(), 0);
}
