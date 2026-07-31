use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::oracle::Verdict;
use chaoscontrol_protocol::assertion_catalog::token_for_descriptors;
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use chaoscontrol_protocol::assertion_wire::{
    encode_catalog_begin, encode_catalog_complete, encode_descriptor_frame, encode_event_frame,
    EventFrame, EVENT_KIND_OFFSET,
};
use chaoscontrol_protocol::{
    HypercallPage, CMD_ASSERT_ALWAYS, CMD_ASSERT_CATALOG_BEGIN, CMD_ASSERT_CATALOG_COMPLETE,
    CMD_ASSERT_CATALOG_DESCRIPTOR, PAYLOAD_MAX, STATUS_ASSERTION_EVENT_REJECTED,
    STATUS_ASSERTION_FAILED, STATUS_ASSERTION_IDENTITY_CONFLICT, STATUS_OK,
};

const COMPATIBILITY_ID: u32 = 101;
const SOURCE_LINE: u32 = 20;
const SOURCE_COLUMN: u32 = 7;
const TRUE_FLAG: u8 = 1;
const FALSE_FLAG: u8 = 0;
const EVENT_DETAILS: &[u8] = br#"{"node":1}"#;
const SPOOFED_ID: u32 = COMPATIBILITY_ID + 1;

fn descriptor() -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "build:test-guest:v1".to_string(),
        logical_key: AssertionLogicalKey::Automatic {
            source_site: "src/main.rs:20:7".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind: AssertionKind::Always,
        message: "state remains valid".to_string(),
        source_file: "src/main.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "test-guest".to_string(),
        category: "invariant".to_string(),
    }
}

fn page(command: u8, flags: u8, id: u32, payload: &[u8]) -> HypercallPage {
    assert!(payload.len() <= PAYLOAD_MAX);
    let mut page = HypercallPage::zeroed();
    page.command = command;
    page.flags = flags;
    page.id = id;
    page.payload_len = payload.len() as u16;
    page.payload[..payload.len()].copy_from_slice(payload);
    page
}

fn begin_page(count: u32) -> HypercallPage {
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_catalog_begin(&mut payload).expect("catalog begin frame");
    page(
        CMD_ASSERT_CATALOG_BEGIN,
        FALSE_FLAG,
        count,
        &payload[..length],
    )
}

fn descriptor_page(value: &AssertionDescriptor) -> HypercallPage {
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_descriptor_frame(value, &mut payload).expect("descriptor frame");
    page(
        CMD_ASSERT_CATALOG_DESCRIPTOR,
        FALSE_FLAG,
        value.compatibility_id.unwrap_or_default(),
        &payload[..length],
    )
}

fn complete_page(value: &AssertionDescriptor) -> HypercallPage {
    let token = token_for_descriptors(core::slice::from_ref(value)).expect("catalog token");
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_catalog_complete(token, &mut payload).expect("complete frame");
    page(
        CMD_ASSERT_CATALOG_COMPLETE,
        FALSE_FLAG,
        1,
        &payload[..length],
    )
}

fn event_page(
    value: &AssertionDescriptor,
    token: AssertionFingerprint,
    condition: bool,
) -> HypercallPage {
    let fingerprint = value.fingerprint().expect("fingerprint");
    let frame = EventFrame {
        catalog_token: token,
        fingerprint,
        kind: value.kind,
        details: EVENT_DETAILS.to_vec(),
    };
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_event_frame(&frame, &mut payload).expect("event frame");
    page(
        CMD_ASSERT_ALWAYS,
        u8::from(condition),
        COMPATIBILITY_ID,
        &payload[..length],
    )
}

fn admit(engine: &mut FaultEngine, value: &AssertionDescriptor) -> AssertionFingerprint {
    assert_eq!(engine.handle_hypercall(&begin_page(1)).1, STATUS_OK);
    assert_eq!(
        engine.handle_hypercall(&descriptor_page(value)).1,
        STATUS_OK
    );
    assert_eq!(engine.handle_hypercall(&complete_page(value)).1, STATUS_OK);
    token_for_descriptors(core::slice::from_ref(value)).expect("catalog token")
}

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
    const FINGERPRINT_PAYLOAD_OFFSET: usize = 1 + 32;
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
fn legacy_diagnostic_quarantine_cannot_promote() {
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.enable_legacy_assertion_diagnostics();
    engine.begin_run();
    let legacy = page(CMD_ASSERT_ALWAYS, TRUE_FLAG, COMPATIBILITY_ID, &[]);
    assert_eq!(
        engine.handle_hypercall(&legacy).1,
        STATUS_ASSERTION_EVENT_REJECTED
    );
    engine.end_run();
    let report = engine.oracle().report();
    assert!(!report.collision_safe_evidence);
    assert!(report.structured_assertions.is_empty());
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
