#![cfg(feature = "std")]

use chaoscontrol_protocol::assertion_catalog::{
    token_for_descriptors, validate_legacy_descriptors, BoundAssertionEvent, CatalogBuilder,
    CatalogConflict, CatalogInsert, MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey, IdentityError,
    ASSERTION_DESCRIPTOR_DOMAIN, ASSERTION_FINGERPRINT_BYTES, ASSERTION_IDENTITY_VERSION,
    ASSERTION_KIND_ALWAYS_DISCRIMINANT, ASSERTION_KIND_REACHABLE_DISCRIMINANT,
    ASSERTION_KIND_SOMETIMES_DISCRIMINANT, ASSERTION_KIND_UNREACHABLE_DISCRIMINANT,
    MAX_ASSERTION_MESSAGE_BYTES,
};
use chaoscontrol_protocol::assertion_wire::{
    decode_descriptor_frame, decode_event_frame, encode_descriptor_frame, encode_event_frame,
    EventFrame,
};
use chaoscontrol_protocol::PAYLOAD_MAX;

const SOURCE_LINE: u32 = 41;
const SOURCE_COLUMN: u32 = 9;
const LEGACY_ID: u32 = 77;
const EVENT_DETAILS: &[u8] = br#"{"term":3}"#;

fn descriptor(key: AssertionLogicalKey) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "build:raft-guest:v1".to_string(),
        logical_key: key,
        compatibility_id: Some(LEGACY_ID),
        kind: AssertionKind::Always,
        message: "leader is unique".to_string(),
        source_file: "src/raft/assertions.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "raft".to_string(),
        category: "invariant".to_string(),
    }
}

fn automatic() -> AssertionDescriptor {
    descriptor(AssertionLogicalKey::Automatic {
        source_site: "src/raft/assertions.rs:41:9".to_string(),
    })
}

#[test]
fn assertion_kind_discriminants_are_stable() {
    const EXPECTED_ALWAYS: u8 = 0;
    const EXPECTED_SOMETIMES: u8 = 1;
    const EXPECTED_REACHABLE: u8 = 2;
    const EXPECTED_UNREACHABLE: u8 = 3;
    assert_eq!(ASSERTION_KIND_ALWAYS_DISCRIMINANT, EXPECTED_ALWAYS);
    assert_eq!(ASSERTION_KIND_SOMETIMES_DISCRIMINANT, EXPECTED_SOMETIMES);
    assert_eq!(ASSERTION_KIND_REACHABLE_DISCRIMINANT, EXPECTED_REACHABLE);
    assert_eq!(
        ASSERTION_KIND_UNREACHABLE_DISCRIMINANT,
        EXPECTED_UNREACHABLE
    );
    assert_eq!(AssertionKind::Always as u8, EXPECTED_ALWAYS);
    assert_eq!(AssertionKind::Sometimes as u8, EXPECTED_SOMETIMES);
    assert_eq!(AssertionKind::Reachable as u8, EXPECTED_REACHABLE);
    assert_eq!(AssertionKind::Unreachable as u8, EXPECTED_UNREACHABLE);
}

#[test]
fn canonical_descriptor_and_wire_round_trip_are_stable() {
    let descriptor = automatic();
    let canonical = descriptor.canonical_bytes().expect("canonical descriptor");
    let fingerprint = descriptor.fingerprint().expect("descriptor fingerprint");
    assert_eq!(
        canonical,
        descriptor.canonical_bytes().expect("repeat canonical")
    );
    assert_eq!(
        fingerprint,
        descriptor.fingerprint().expect("repeat fingerprint")
    );

    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_descriptor_frame(&descriptor, &mut payload).expect("encode descriptor");
    let decoded = decode_descriptor_frame(&payload[..length]).expect("decode descriptor");
    assert_eq!(decoded.descriptor, descriptor);
    assert_eq!(decoded.canonical_bytes, canonical);
    assert_eq!(decoded.fingerprint, fingerprint);
}

#[test]
fn exact_duplicate_is_idempotent() {
    let descriptor = automatic();
    let token =
        token_for_descriptors(&[descriptor.clone(), descriptor.clone()]).expect("catalog token");
    let mut builder = CatalogBuilder::begin(2).expect("catalog begin");
    assert_eq!(
        builder.insert(descriptor.clone()),
        Ok(CatalogInsert::Inserted)
    );
    assert_eq!(
        builder.insert(descriptor),
        Ok(CatalogInsert::ExactDuplicate)
    );
    let catalog = builder.complete(token).expect("catalog complete");
    assert_eq!(catalog.assertions.len(), 1);
}

#[test]
fn forced_fingerprint_collision_fails_closed() {
    let first = automatic();
    let mut second = first.clone();
    second.logical_key = AssertionLogicalKey::Stable {
        key: "stable-leader".to_string(),
    };
    second.message = "term never decreases".to_string();
    const INJECTED_FINGERPRINT_BYTE: u8 = 0xab;
    let injected = AssertionFingerprint([INJECTED_FINGERPRINT_BYTE; ASSERTION_FINGERPRINT_BYTES]);
    let mut builder = CatalogBuilder::begin(2).expect("catalog begin");
    assert_eq!(
        builder.insert_with_fingerprint(first, injected),
        Ok(CatalogInsert::Inserted)
    );
    assert_eq!(
        builder.insert_with_fingerprint(second, injected),
        Err(CatalogConflict::FingerprintCollision)
    );
}

#[test]
fn logical_key_metadata_conflicts_are_typed() {
    let base = automatic();
    let cases = [
        ("kind", {
            let mut value = base.clone();
            value.kind = AssertionKind::Sometimes;
            (value, CatalogConflict::KindConflict)
        }),
        ("message", {
            let mut value = base.clone();
            value.message = "different".to_string();
            (value, CatalogConflict::MessageConflict)
        }),
        ("source", {
            let mut value = base.clone();
            value.source_line = SOURCE_LINE + 1;
            (
                value,
                CatalogConflict::Descriptor(IdentityError::InvalidAutomaticSourceSite),
            )
        }),
        ("guest", {
            let mut value = base.clone();
            value.guest = "other".to_string();
            (value, CatalogConflict::GuestConflict)
        }),
        ("category", {
            let mut value = base.clone();
            value.category = "recovery".to_string();
            (value, CatalogConflict::CategoryConflict)
        }),
    ];
    for (name, (candidate, expected)) in cases {
        let mut builder = CatalogBuilder::begin(2).expect("catalog begin");
        builder.insert(base.clone()).expect("first descriptor");
        assert_eq!(builder.insert(candidate), Err(expected), "case {name}");
    }
}

#[test]
fn legacy_alias_conflict_is_diagnostic_and_never_admitted() {
    let first = descriptor(AssertionLogicalKey::LegacyU32 { id: LEGACY_ID });
    let mut conflict = first.clone();
    conflict.message = "different legacy assertion".to_string();
    assert_eq!(
        validate_legacy_descriptors(&[first.clone(), conflict]),
        Err(CatalogConflict::LegacyAliasConflict)
    );

    let mut other = descriptor(AssertionLogicalKey::LegacyU32 { id: LEGACY_ID });
    other.namespace = "build:redb-guest:v1".to_string();
    other.guest = "redb".to_string();
    validate_legacy_descriptors(&[first.clone(), other]).expect("separate diagnostic namespaces");

    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    assert_eq!(
        builder.insert(first),
        Err(CatalogConflict::LegacyIdentityForbidden)
    );
}

#[test]
fn automatic_compatibility_alias_collision_is_fatal() {
    let first = automatic();
    let mut second = first.clone();
    second.logical_key = AssertionLogicalKey::Automatic {
        source_site: "src/raft/other.rs:1:1".to_string(),
    };
    second.source_file = "src/raft/other.rs".to_string();
    second.source_line = 1;
    second.source_column = 1;
    second.message = "different automatic assertion".to_string();
    let mut builder = CatalogBuilder::begin(2).expect("catalog begin");
    builder.insert(first).expect("first automatic descriptor");
    assert_eq!(
        builder.insert(second),
        Err(CatalogConflict::CompatibilityAliasConflict)
    );
}

#[test]
fn events_require_the_accepted_token_fingerprint_and_kind() {
    let descriptor = automatic();
    let token = token_for_descriptors(core::slice::from_ref(&descriptor)).expect("token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let event = BoundAssertionEvent {
        catalog_token: token,
        fingerprint,
        kind: descriptor.kind,
    };
    assert_eq!(
        catalog
            .resolve_event(&event)
            .expect("bound event")
            .descriptor,
        descriptor
    );

    let wrong_kind = BoundAssertionEvent {
        kind: AssertionKind::Unreachable,
        ..event.clone()
    };
    assert_eq!(
        catalog.resolve_event(&wrong_kind),
        Err(CatalogConflict::EventKindMismatch)
    );
    let unknown = BoundAssertionEvent {
        fingerprint: AssertionFingerprint::ZERO,
        ..event
    };
    assert_eq!(
        catalog.resolve_event(&unknown),
        Err(CatalogConflict::UnknownFingerprint)
    );
}

#[test]
fn compatibility_id_none_serializes_as_missing_and_rejects_explicit_null() {
    let mut value = automatic();
    value.compatibility_id = None;
    let json = serde_json::to_value(&value).expect("descriptor JSON");
    assert!(json.get("compatibility_id").is_none());
    let decoded: AssertionDescriptor =
        serde_json::from_value(json.clone()).expect("missing alias decodes as None");
    assert_eq!(decoded.compatibility_id, None);

    let mut null = json;
    null["compatibility_id"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<AssertionDescriptor>(null).is_err());
}

#[test]
fn malformed_and_over_limit_inputs_fail_before_admission() {
    let mut too_long = automatic();
    too_long.message = "x".repeat(MAX_ASSERTION_MESSAGE_BYTES + 1);
    assert_eq!(
        too_long.validate(),
        Err(IdentityError::FieldTooLong("message"))
    );
    assert!(matches!(
        CatalogBuilder::begin(MAX_ASSERTION_CATALOG_ENTRIES + 1),
        Err(CatalogConflict::CardinalityOverflow)
    ));

    let valid = automatic();
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_descriptor_frame(&valid, &mut payload).expect("descriptor frame");
    let first_tag_offset = ASSERTION_FINGERPRINT_BYTES + ASSERTION_DESCRIPTOR_DOMAIN.len() + 2;
    payload[first_tag_offset] = u8::MAX;
    assert!(decode_descriptor_frame(&payload[..length]).is_err());
}

#[test]
fn event_wire_round_trip_retains_binding_and_details() {
    let descriptor = automatic();
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let frame = EventFrame {
        catalog_token: fingerprint,
        fingerprint,
        kind: descriptor.kind,
        details: EVENT_DETAILS.to_vec(),
    };
    let mut payload = [0_u8; PAYLOAD_MAX];
    let length = encode_event_frame(&frame, &mut payload).expect("event frame");
    assert_eq!(
        decode_event_frame(&payload[..length], descriptor.kind).expect("decode event"),
        frame
    );
}
