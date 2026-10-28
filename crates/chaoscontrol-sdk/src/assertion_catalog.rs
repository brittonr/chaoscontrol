use crate::assert::{AssertionKind, CatalogEntry, CatalogLogicalKey, ASSERTION_CATALOG};
use crate::transport;
use chaoscontrol_protocol::admission::{
    catalog_token, AcceptedCatalog, CatalogBuilder, CatalogConflict, MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use chaoscontrol_protocol::transport::{
    encode_catalog_begin, encode_catalog_complete, encode_descriptor_frame,
};
use chaoscontrol_protocol::{
    CMD_ASSERT_CATALOG_BEGIN, CMD_ASSERT_CATALOG_COMPLETE, CMD_ASSERT_CATALOG_DESCRIPTOR,
    PAYLOAD_MAX,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Source metadata for one catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CatalogLocation {
    pub class: &'static str,
    pub function: &'static str,
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
    pub display_type: Option<&'static str>,
    pub must_hit: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundIdentity {
    pub catalog_token: AssertionFingerprint,
    pub fingerprint: AssertionFingerprint,
    pub compatibility_id: Option<u32>,
}

#[derive(Debug)]
struct SdkCatalog {
    descriptors: Vec<AssertionDescriptor>,
    must_hit: Vec<bool>,
    accepted: Result<Option<AcceptedCatalog>, CatalogConflict>,
}

static SDK_CATALOG: OnceLock<SdkCatalog> = OnceLock::new();

/// Raw assertion descriptors registered before catalog emission.
struct RawRegistration {
    descriptor: AssertionDescriptor,
    location: CatalogLocation,
}

static RAW_REGISTRATIONS: Mutex<Vec<RawRegistration>> = Mutex::new(Vec::new());
static RAW_CLOSED: AtomicBool = AtomicBool::new(false);

/// Register one runtime raw assertion before the catalog is emitted.
pub(crate) fn register_raw(
    descriptor: AssertionDescriptor,
    class: &str,
    function: &str,
    display_type: &str,
    must_hit: bool,
) -> bool {
    if descriptor.validate().is_err() {
        return false;
    }
    let Ok(mut registrations) = RAW_REGISTRATIONS.lock() else {
        return false;
    };
    if registrations
        .iter()
        .any(|raw| raw.descriptor.compatibility_id == descriptor.compatibility_id)
    {
        return true;
    }
    if RAW_CLOSED.load(Ordering::SeqCst) {
        return false;
    }
    let location = CatalogLocation {
        class: Box::leak(class.to_string().into_boxed_str()),
        function: Box::leak(function.to_string().into_boxed_str()),
        file: Box::leak(descriptor.source_file.clone().into_boxed_str()),
        line: descriptor.source_line,
        column: descriptor.source_column,
        display_type: Some(Box::leak(display_type.to_string().into_boxed_str())),
        must_hit: Some(must_hit),
    };
    registrations.push(RawRegistration {
        descriptor,
        location,
    });
    true
}

pub(crate) fn emit_catalog() {
    let catalog = SDK_CATALOG.get_or_init(build_catalog);
    if catalog.descriptors.is_empty() {
        return;
    }
    let descriptor_count = u32::try_from(catalog.descriptors.len()).unwrap_or(u32::MAX);
    let mut payload = [0_u8; PAYLOAD_MAX];
    if let Ok(length) = encode_catalog_begin(&mut payload) {
        transport::hypercall_raw(
            CMD_ASSERT_CATALOG_BEGIN,
            0,
            descriptor_count,
            &payload[..length],
        );
    }
    for descriptor in &catalog.descriptors {
        if let Ok(length) = encode_descriptor_frame(descriptor, &mut payload) {
            transport::hypercall_raw(
                CMD_ASSERT_CATALOG_DESCRIPTOR,
                0,
                descriptor.compatibility_id.unwrap_or_default(),
                &payload[..length],
            );
        }
    }
    let token = catalog
        .accepted
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .map_or(AssertionFingerprint::ZERO, |accepted| accepted.token);
    if let Ok(length) = encode_catalog_complete(token, &mut payload) {
        transport::hypercall_raw(
            CMD_ASSERT_CATALOG_COMPLETE,
            0,
            descriptor_count,
            &payload[..length],
        );
    }
    if crate::internal::is_in_vm() {
        for (descriptor, must_hit) in catalog.descriptors.iter().zip(&catalog.must_hit) {
            let Ok(fingerprint) = descriptor.fingerprint() else {
                continue;
            };
            let details = serde_json::json!({
                "fingerprint": fingerprint.to_hex(),
                "must_hit": must_hit,
            });
            crate::lifecycle::send_event("chaoscontrol_must_hit", &details);
        }
    }
    if crate::internal::is_local_output() {
        if let Ok(Some(accepted)) = catalog.accepted.as_ref() {
            for descriptor in &catalog.descriptors {
                crate::transport::local_emit_declaration(descriptor, accepted.token);
            }
        }
    }
    for entry in crate::guidance::GUIDANCE_CATALOG.iter() {
        crate::guidance::emit_catalog_record(entry);
    }
}

pub(crate) fn resolve_compatibility(
    id: u32,
    kind: AssertionKind,
    message: &str,
) -> Option<BoundIdentity> {
    let catalog = SDK_CATALOG.get_or_init(build_catalog);
    let accepted = catalog.accepted.as_ref().ok()?.as_ref()?;
    let mut matches = accepted.assertions.values().filter(|assertion| {
        assertion.descriptor.compatibility_id == Some(id)
            && assertion.descriptor.kind == protocol_kind(kind)
            && assertion.descriptor.message == message
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(BoundIdentity {
        catalog_token: accepted.token,
        fingerprint: first.fingerprint,
        compatibility_id: first.descriptor.compatibility_id,
    })
}

/// Resolve the source metadata for one compatibility assertion site.
pub(crate) fn location_for_compatibility(
    id: u32,
    kind: AssertionKind,
    message: &str,
) -> Option<CatalogLocation> {
    let catalog_kind = kind.to_catalog_kind();
    let mut matches = ASSERTION_CATALOG
        .iter()
        .filter(|entry| entry.id == id && entry.kind == catalog_kind && entry.message == message);
    match (matches.next(), matches.next()) {
        (Some(first), None) => Some(CatalogLocation {
            class: first.class,
            function: (first.function)(),
            file: first.file,
            line: first.line,
            column: first.column,
            display_type: None,
            must_hit: Some(first.must_hit),
        }),
        (None, _) => raw_location_for_compatibility(id, kind, message),
        (Some(_), Some(_)) => None,
    }
}

fn raw_location_for_compatibility(
    id: u32,
    kind: AssertionKind,
    message: &str,
) -> Option<CatalogLocation> {
    let kind = protocol_kind(kind);
    let registrations = RAW_REGISTRATIONS.lock().ok()?;
    let mut matches = registrations.iter().filter(|raw| {
        raw.descriptor.compatibility_id == Some(id)
            && raw.descriptor.kind == kind
            && raw.descriptor.message == message
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first.location)
}

pub(crate) fn resolve_stable(
    namespace: &str,
    key: &str,
    kind: AssertionKind,
    message: &str,
) -> Option<BoundIdentity> {
    let catalog = SDK_CATALOG.get_or_init(build_catalog);
    let accepted = catalog.accepted.as_ref().ok()?.as_ref()?;
    let logical_key = AssertionLogicalKey::Stable {
        key: key.to_string(),
    };
    let mut matches = accepted.assertions.values().filter(|assertion| {
        assertion.descriptor.namespace == namespace
            && assertion.descriptor.logical_key == logical_key
            && assertion.descriptor.kind == protocol_kind(kind)
            && assertion.descriptor.message == message
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(BoundIdentity {
        catalog_token: accepted.token,
        fingerprint: first.fingerprint,
        compatibility_id: first.descriptor.compatibility_id,
    })
}

fn build_catalog() -> SdkCatalog {
    RAW_CLOSED.store(true, Ordering::SeqCst);
    if ASSERTION_CATALOG.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return SdkCatalog {
            descriptors: Vec::new(),
            must_hit: Vec::new(),
            accepted: Err(CatalogConflict::CardinalityOverflow),
        };
    }
    let raw_registrations = RAW_REGISTRATIONS
        .lock()
        .map(|registrations| {
            registrations
                .iter()
                .map(|raw| {
                    (
                        raw.descriptor.clone(),
                        raw.location.must_hit.unwrap_or(true),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if ASSERTION_CATALOG
        .len()
        .saturating_add(raw_registrations.len())
        > MAX_ASSERTION_CATALOG_ENTRIES
    {
        return SdkCatalog {
            descriptors: Vec::new(),
            must_hit: Vec::new(),
            accepted: Err(CatalogConflict::CardinalityOverflow),
        };
    }
    let mut descriptors = Vec::with_capacity(ASSERTION_CATALOG.len() + raw_registrations.len());
    let mut must_hit = Vec::with_capacity(descriptors.capacity());
    for entry in ASSERTION_CATALOG.iter() {
        match descriptor_from_entry(entry) {
            Ok(descriptor) => {
                descriptors.push(descriptor);
                must_hit.push(entry.must_hit);
            }
            Err(error) => {
                return SdkCatalog {
                    descriptors,
                    must_hit,
                    accepted: Err(error),
                };
            }
        }
    }
    for (descriptor, raw_must_hit) in raw_registrations {
        descriptors.push(descriptor);
        must_hit.push(raw_must_hit);
    }
    let accepted = if descriptors.is_empty() {
        Ok(None)
    } else {
        accept_descriptors(&descriptors).map(Some)
    };
    SdkCatalog {
        descriptors,
        must_hit,
        accepted,
    }
}

fn accept_descriptors(
    descriptors: &[AssertionDescriptor],
) -> Result<AcceptedCatalog, CatalogConflict> {
    let mut builder = CatalogBuilder::begin(descriptors.len())?;
    for descriptor in descriptors {
        builder.insert(descriptor.clone())?;
    }
    let mut assertions = BTreeMap::new();
    for descriptor in descriptors {
        let fingerprint = descriptor
            .fingerprint()
            .map_err(CatalogConflict::Descriptor)?;
        let canonical_bytes = descriptor
            .canonical_bytes()
            .map_err(CatalogConflict::Descriptor)?;
        assertions.entry(fingerprint).or_insert_with(|| {
            chaoscontrol_protocol::admission::AdmittedAssertion {
                descriptor: descriptor.clone(),
                fingerprint,
                canonical_bytes,
            }
        });
    }
    builder.complete(catalog_token(&assertions))
}

fn descriptor_from_entry(entry: &CatalogEntry) -> Result<AssertionDescriptor, CatalogConflict> {
    let logical_key = match entry.logical_key {
        CatalogLogicalKey::Automatic(_) => AssertionLogicalKey::Automatic {
            source_site: format!("{}:{}:{}", entry.file, entry.line, entry.column),
        },
        CatalogLogicalKey::Stable(key) => AssertionLogicalKey::Stable {
            key: key.to_string(),
        },
    };
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: entry.namespace.to_string(),
        logical_key,
        compatibility_id: Some(entry.id),
        kind: protocol_catalog_kind(entry.kind)?,
        message: entry.message.to_string(),
        source_file: entry.file.to_string(),
        source_line: entry.line,
        source_column: entry.column,
        guest: normalized_metadata(entry.guest),
        category: normalized_metadata(entry.category),
    };
    descriptor.validate().map_err(CatalogConflict::Descriptor)?;
    Ok(descriptor)
}

fn protocol_kind(kind: AssertionKind) -> chaoscontrol_protocol::identity::AssertionKind {
    match kind {
        AssertionKind::Always => chaoscontrol_protocol::identity::AssertionKind::Always,
        AssertionKind::Sometimes => chaoscontrol_protocol::identity::AssertionKind::Sometimes,
        AssertionKind::Reachable => chaoscontrol_protocol::identity::AssertionKind::Reachable,
        AssertionKind::Unreachable => chaoscontrol_protocol::identity::AssertionKind::Unreachable,
    }
}

fn protocol_catalog_kind(
    kind: u8,
) -> Result<chaoscontrol_protocol::identity::AssertionKind, CatalogConflict> {
    match kind {
        crate::assert::CATALOG_KIND_ALWAYS => Ok(protocol_kind(AssertionKind::Always)),
        crate::assert::CATALOG_KIND_SOMETIMES => Ok(protocol_kind(AssertionKind::Sometimes)),
        crate::assert::CATALOG_KIND_REACHABLE => Ok(protocol_kind(AssertionKind::Reachable)),
        crate::assert::CATALOG_KIND_UNREACHABLE => Ok(protocol_kind(AssertionKind::Unreachable)),
        _ => Err(CatalogConflict::Descriptor(
            chaoscontrol_protocol::identity::AssertionError::InvalidKind,
        )),
    }
}

fn normalized_metadata(value: &str) -> String {
    if value.is_empty() {
        return "uncategorized".to_string();
    }
    value.to_string()
}
