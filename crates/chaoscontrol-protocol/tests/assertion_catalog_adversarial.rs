#![cfg(feature = "std")]

use chaoscontrol_protocol::admission::{
    catalog_token, token_for_descriptors, validate_accepted_catalog, validate_legacy_descriptors,
    AcceptedCatalog, AdmittedAssertion, BoundAssertionEvent, CatalogBuilder, CatalogConflict,
    CatalogValidationStatus, ASSERTION_CATALOG_VERSION,
};
use std::collections::BTreeMap;

use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionError, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_FINGERPRINT_BYTES, ASSERTION_IDENTITY_VERSION, MAX_ASSERTION_MESSAGE_BYTES,
};

const SOURCE_LINE: u32 = 17;
const SOURCE_COLUMN: u32 = 5;
const LEGACY_ID: u32 = 43;
const OTHER_LEGACY_ID: u32 = 47;
const INJECTED_FINGERPRINT_BYTE: u8 = 0xa5;

fn descriptor() -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.guest".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "stable-key".to_string(),
        },
        compatibility_id: Some(LEGACY_ID),
        kind: AssertionKind::Always,
        message: "stable assertion".to_string(),
        source_file: "src/main.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
    }
}

fn legacy_descriptor() -> AssertionDescriptor {
    let mut legacy = descriptor();
    legacy.namespace = "legacy:guest".to_string();
    legacy.logical_key = AssertionLogicalKey::LegacyU32 { id: LEGACY_ID };
    legacy
}

fn accepted_catalog() -> chaoscontrol_protocol::admission::AcceptedCatalog {
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor).expect("descriptor");
    builder.complete(token).expect("catalog complete")
}

#[test]
fn descriptor_error_poisons_builder() {
    let mut invalid = descriptor();
    invalid.message = "x".repeat(MAX_ASSERTION_MESSAGE_BYTES + 1);
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");

    assert!(matches!(
        builder.insert(invalid),
        Err(CatalogConflict::Descriptor(_))
    ));
    assert_eq!(
        builder.insert(descriptor()),
        Err(CatalogConflict::PostConflict)
    );
}

#[test]
fn every_admission_conflict_poisons_builder() {
    let first = descriptor();
    let mut conflicting = first.clone();
    conflicting.message = "conflicting message".to_string();
    let mut builder = CatalogBuilder::begin(2).expect("catalog begin");
    builder.insert(first).expect("first descriptor");
    assert_eq!(
        builder.insert(conflicting),
        Err(CatalogConflict::MessageConflict)
    );
    assert_eq!(
        builder.insert(descriptor()),
        Err(CatalogConflict::PostConflict)
    );

    let mut overflow = CatalogBuilder::begin(1).expect("bounded catalog");
    overflow.insert(descriptor()).expect("first descriptor");
    assert_eq!(
        overflow.insert(descriptor()),
        Err(CatalogConflict::UnexpectedDescriptorCount)
    );
    assert_eq!(
        overflow.insert(descriptor()),
        Err(CatalogConflict::PostConflict)
    );
}

#[test]
fn fingerprint_collision_poisons_builder() {
    let first = descriptor();
    let mut second = first.clone();
    second.logical_key = AssertionLogicalKey::Stable {
        key: "other-key".to_string(),
    };
    let injected = AssertionFingerprint([INJECTED_FINGERPRINT_BYTE; ASSERTION_FINGERPRINT_BYTES]);
    let mut builder = CatalogBuilder::begin(2).expect("catalog begin");
    builder
        .insert_with_fingerprint(first, injected)
        .expect("first descriptor");
    assert_eq!(
        builder.insert_with_fingerprint(second, injected),
        Err(CatalogConflict::FingerprintCollision)
    );
    assert_eq!(
        builder.insert(descriptor()),
        Err(CatalogConflict::PostConflict)
    );
}

#[test]
fn completion_rejects_a_single_injected_fingerprint() {
    let injected = AssertionFingerprint([INJECTED_FINGERPRINT_BYTE; ASSERTION_FINGERPRINT_BYTES]);
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder
        .insert_with_fingerprint(descriptor(), injected)
        .expect("injected descriptor");
    assert_eq!(
        builder.complete(AssertionFingerprint::ZERO),
        Err(CatalogConflict::Descriptor(
            AssertionError::InvalidFingerprint
        ))
    );
}

#[test]
fn strict_descriptor_rejects_legacy_alias_and_zero_source_position() {
    let mut legacy = descriptor();
    legacy.logical_key = AssertionLogicalKey::LegacyU32 { id: LEGACY_ID };
    legacy.compatibility_id = Some(OTHER_LEGACY_ID);
    assert_eq!(legacy.validate(), Err(AssertionError::InvalidLegacyAlias));

    let mut zero_line = descriptor();
    zero_line.source_line = 0;
    assert_eq!(
        zero_line.validate(),
        Err(AssertionError::InvalidSourcePosition)
    );
    let mut zero_column = descriptor();
    zero_column.source_column = 0;
    assert_eq!(
        zero_column.validate(),
        Err(AssertionError::InvalidSourcePosition)
    );
}

#[test]
fn automatic_identity_must_match_normalized_source_site() {
    let mut automatic = descriptor();
    automatic.logical_key = AssertionLogicalKey::Automatic {
        source_site: format!("src/main.rs:{SOURCE_LINE}:{SOURCE_COLUMN}"),
    };
    automatic.validate().expect("matching source site");
    if let AssertionLogicalKey::Automatic { source_site } = &mut automatic.logical_key {
        *source_site = "src/other.rs:17:5".to_string();
    }
    assert_eq!(
        automatic.validate(),
        Err(AssertionError::InvalidAutomaticSourceSite)
    );
}

#[test]
fn bounded_category_tokens_include_shipped_template_vocabulary() {
    for category in ["workload-driver", "service-invariant"] {
        let mut candidate = descriptor();
        candidate.category = category.to_string();
        candidate.validate().expect("shipped category");
    }
    let mut invalid = descriptor();
    invalid.category = "Service Invariant".to_string();
    assert_eq!(invalid.validate(), Err(AssertionError::InvalidCategory));
}

#[test]
fn accepted_catalog_validation_recomputes_all_identity_fields() {
    let catalog = accepted_catalog();
    validate_accepted_catalog(&catalog).expect("valid accepted catalog");

    let mut wrong_version = catalog.clone();
    wrong_version.catalog_version = ASSERTION_CATALOG_VERSION + 1;
    assert_eq!(
        validate_accepted_catalog(&wrong_version),
        Err(CatalogConflict::CatalogVersionMismatch)
    );

    let mut wrong_status = catalog.clone();
    wrong_status.status = CatalogValidationStatus::Pending;
    assert_eq!(
        validate_accepted_catalog(&wrong_status),
        Err(CatalogConflict::CatalogStatusMismatch)
    );

    let mut wrong_token = catalog.clone();
    wrong_token.token = AssertionFingerprint::ZERO;
    assert_eq!(
        validate_accepted_catalog(&wrong_token),
        Err(CatalogConflict::CatalogTokenMismatch)
    );

    let mut wrong_canonical = catalog;
    wrong_canonical
        .assertions
        .values_mut()
        .next()
        .expect("assertion")
        .canonical_bytes
        .push(0);
    assert_eq!(
        validate_accepted_catalog(&wrong_canonical),
        Err(CatalogConflict::CanonicalMismatch)
    );
}

#[test]
fn legacy_descriptors_are_diagnostic_but_never_strict() {
    let legacy = legacy_descriptor();
    validate_legacy_descriptors(&[legacy.clone(), legacy.clone()])
        .expect("exact legacy duplicate is diagnostic");
    let mut conflict = legacy.clone();
    conflict.message = "conflicting legacy alias".to_string();
    assert_eq!(
        validate_legacy_descriptors(&[legacy.clone(), conflict]),
        Err(CatalogConflict::LegacyAliasConflict)
    );

    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    assert_eq!(
        builder.insert(legacy),
        Err(CatalogConflict::LegacyIdentityForbidden)
    );
    assert_eq!(
        builder.insert(descriptor()),
        Err(CatalogConflict::PostConflict)
    );
}

#[test]
fn forged_legacy_accepted_catalog_cannot_resolve_events() {
    let descriptor = legacy_descriptor();
    let fingerprint = descriptor.fingerprint().expect("legacy fingerprint");
    let admitted = AdmittedAssertion {
        canonical_bytes: descriptor.canonical_bytes().expect("legacy canonical"),
        descriptor,
        fingerprint,
    };
    let assertions = BTreeMap::from([(fingerprint, admitted)]);
    let token = catalog_token(&assertions);
    let catalog = AcceptedCatalog {
        catalog_version: ASSERTION_CATALOG_VERSION,
        token,
        status: CatalogValidationStatus::Accepted,
        assertions,
    };
    assert_eq!(
        validate_accepted_catalog(&catalog),
        Err(CatalogConflict::LegacyIdentityForbidden)
    );
    assert_eq!(
        catalog.resolve_event(&BoundAssertionEvent {
            catalog_token: token,
            fingerprint,
            kind: AssertionKind::Always,
        }),
        Err(CatalogConflict::LegacyIdentityForbidden)
    );
}

#[test]
fn builder_exposes_bounded_frame_counts() {
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    assert_eq!(builder.expected_frames(), 1);
    assert_eq!(builder.received_frames(), 0);
    builder.insert(descriptor()).expect("descriptor");
    assert_eq!(builder.received_frames(), 1);
}
