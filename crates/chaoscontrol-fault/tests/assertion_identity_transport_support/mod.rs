use chaoscontrol_fault::engine::FaultEngine;
use chaoscontrol_protocol::admission::token_for_descriptors;
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use chaoscontrol_protocol::transport::{
    encode_catalog_begin, encode_catalog_complete, encode_descriptor_frame, encode_event_frame,
    EventFrame,
};
use chaoscontrol_protocol::{
    HypercallPage, CMD_ASSERT_ALWAYS, CMD_ASSERT_CATALOG_BEGIN, CMD_ASSERT_CATALOG_COMPLETE,
    CMD_ASSERT_CATALOG_DESCRIPTOR, PAYLOAD_MAX, STATUS_OK,
};

pub(super) const COMPATIBILITY_ID: u32 = 101;
const SOURCE_LINE: u32 = 20;
const SOURCE_COLUMN: u32 = 7;
pub(super) const TRUE_FLAG: u8 = 1;
const FALSE_FLAG: u8 = 0;
const EVENT_DETAILS: &[u8] = br#"{"node":1}"#;
pub(super) const SPOOFED_ID: u32 = COMPATIBILITY_ID + 1;

pub(super) fn descriptor() -> AssertionDescriptor {
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

pub(super) fn page(command: u8, flags: u8, id: u32, payload: &[u8]) -> HypercallPage {
    assert!(payload.len() <= PAYLOAD_MAX);
    let mut page = HypercallPage::zeroed();
    page.command = command;
    page.flags = flags;
    page.id = id;
    page.payload_len = payload.len() as u16;
    page.payload[..payload.len()].copy_from_slice(payload);
    page
}

pub(super) fn begin_page(count: u32) -> HypercallPage {
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_catalog_begin(&mut payload).expect("catalog begin frame");
    page(
        CMD_ASSERT_CATALOG_BEGIN,
        FALSE_FLAG,
        count,
        &payload[..length],
    )
}

pub(super) fn descriptor_page(value: &AssertionDescriptor) -> HypercallPage {
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_descriptor_frame(value, &mut payload).expect("descriptor frame");
    page(
        CMD_ASSERT_CATALOG_DESCRIPTOR,
        FALSE_FLAG,
        value.compatibility_id.unwrap_or_default(),
        &payload[..length],
    )
}

pub(super) fn complete_page(value: &AssertionDescriptor) -> HypercallPage {
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

pub(super) fn event_page(
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

pub(super) fn admit(engine: &mut FaultEngine, value: &AssertionDescriptor) -> AssertionFingerprint {
    assert_eq!(engine.handle_hypercall(&begin_page(1)).1, STATUS_OK);
    assert_eq!(
        engine.handle_hypercall(&descriptor_page(value)).1,
        STATUS_OK
    );
    assert_eq!(engine.handle_hypercall(&complete_page(value)).1, STATUS_OK);
    token_for_descriptors(core::slice::from_ref(value)).expect("catalog token")
}
