use crate::assert::{AssertionKind, CatalogEntry, CatalogLogicalKey, ASSERTION_CATALOG};
use crate::transport;
use chaoscontrol_protocol::assertion_catalog::{
    catalog_token, AcceptedCatalog, CatalogBuilder, CatalogConflict, MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use chaoscontrol_protocol::assertion_wire::{
    encode_catalog_begin, encode_catalog_complete, encode_descriptor_frame,
};
use chaoscontrol_protocol::{
    CMD_ASSERT_CATALOG_BEGIN, CMD_ASSERT_CATALOG_COMPLETE, CMD_ASSERT_CATALOG_DESCRIPTOR,
    PAYLOAD_MAX,
};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundIdentity {
    pub catalog_token: AssertionFingerprint,
    pub fingerprint: AssertionFingerprint,
    pub compatibility_id: Option<u32>,
}

#[derive(Debug)]
struct SdkCatalog {
    descriptors: Vec<AssertionDescriptor>,
    accepted: Result<Option<AcceptedCatalog>, CatalogConflict>,
}

static SDK_CATALOG: OnceLock<SdkCatalog> = OnceLock::new();

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
    if ASSERTION_CATALOG.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return SdkCatalog {
            descriptors: Vec::new(),
            accepted: Err(CatalogConflict::CardinalityOverflow),
        };
    }
    let mut descriptors = Vec::with_capacity(ASSERTION_CATALOG.len());
    for entry in ASSERTION_CATALOG.iter() {
        match descriptor_from_entry(entry) {
            Ok(descriptor) => descriptors.push(descriptor),
            Err(error) => {
                return SdkCatalog {
                    descriptors,
                    accepted: Err(error),
                };
            }
        }
    }
    let accepted = if descriptors.is_empty() {
        Ok(None)
    } else {
        accept_descriptors(&descriptors).map(Some)
    };
    SdkCatalog {
        descriptors,
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
            chaoscontrol_protocol::assertion_catalog::AdmittedAssertion {
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
        CatalogLogicalKey::Automatic(source_site) => AssertionLogicalKey::Automatic {
            source_site: source_site.to_string(),
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

fn protocol_kind(kind: AssertionKind) -> chaoscontrol_protocol::assertion_identity::AssertionKind {
    match kind {
        AssertionKind::Always => chaoscontrol_protocol::assertion_identity::AssertionKind::Always,
        AssertionKind::Sometimes => {
            chaoscontrol_protocol::assertion_identity::AssertionKind::Sometimes
        }
        AssertionKind::Reachable => {
            chaoscontrol_protocol::assertion_identity::AssertionKind::Reachable
        }
        AssertionKind::Unreachable => {
            chaoscontrol_protocol::assertion_identity::AssertionKind::Unreachable
        }
    }
}

fn protocol_catalog_kind(
    kind: u8,
) -> Result<chaoscontrol_protocol::assertion_identity::AssertionKind, CatalogConflict> {
    match kind {
        crate::assert::CATALOG_KIND_ALWAYS => Ok(protocol_kind(AssertionKind::Always)),
        crate::assert::CATALOG_KIND_SOMETIMES => Ok(protocol_kind(AssertionKind::Sometimes)),
        crate::assert::CATALOG_KIND_REACHABLE => Ok(protocol_kind(AssertionKind::Reachable)),
        crate::assert::CATALOG_KIND_UNREACHABLE => Ok(protocol_kind(AssertionKind::Unreachable)),
        _ => Err(CatalogConflict::Descriptor(
            chaoscontrol_protocol::assertion_identity::IdentityError::InvalidKind,
        )),
    }
}

fn normalized_metadata(value: &str) -> String {
    if value.is_empty() {
        return "uncategorized".to_string();
    }
    value.to_string()
}
