#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundIdentity {
    pub catalog_token: ::chaoscontrol_protocol::identity::AssertionFingerprint,
    pub fingerprint: ::chaoscontrol_protocol::identity::AssertionFingerprint,
    pub compatibility_id: Option<u32>,
}

#[derive(Debug)]
struct SdkCatalog {
    descriptors: Vec<::chaoscontrol_protocol::identity::AssertionDescriptor>,
    accepted: Result<
        Option<::chaoscontrol_protocol::admission::AcceptedCatalog>,
        ::chaoscontrol_protocol::admission::CatalogConflict,
    >,
}

static SDK_CATALOG: std::sync::OnceLock<SdkCatalog> = std::sync::OnceLock::new();

pub(crate) fn emit_catalog() {
    let catalog = SDK_CATALOG.get_or_init(build_catalog);
    if catalog.descriptors.is_empty() {
        return;
    }
    let descriptor_count = u32::try_from(catalog.descriptors.len()).unwrap_or(u32::MAX);
    let mut payload = [0_u8; ::chaoscontrol_protocol::PAYLOAD_MAX];
    if let Ok(length) = ::chaoscontrol_protocol::transport::encode_catalog_begin(&mut payload) {
        crate::transport::hypercall_raw(
            ::chaoscontrol_protocol::CMD_ASSERT_CATALOG_BEGIN,
            0,
            descriptor_count,
            &payload[..length],
        );
    }
    for descriptor in &catalog.descriptors {
        if let Ok(length) =
            ::chaoscontrol_protocol::transport::encode_descriptor_frame(descriptor, &mut payload)
        {
            crate::transport::hypercall_raw(
                ::chaoscontrol_protocol::CMD_ASSERT_CATALOG_DESCRIPTOR,
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
        .map_or(
            ::chaoscontrol_protocol::identity::AssertionFingerprint::ZERO,
            |accepted| accepted.token,
        );
    if let Ok(length) =
        ::chaoscontrol_protocol::transport::encode_catalog_complete(token, &mut payload)
    {
        crate::transport::hypercall_raw(
            ::chaoscontrol_protocol::CMD_ASSERT_CATALOG_COMPLETE,
            0,
            descriptor_count,
            &payload[..length],
        );
    }
}

pub(crate) fn resolve_compatibility(
    id: u32,
    kind: crate::assert::AssertionKind,
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
    kind: crate::assert::AssertionKind,
    message: &str,
) -> Option<BoundIdentity> {
    let catalog = SDK_CATALOG.get_or_init(build_catalog);
    let accepted = catalog.accepted.as_ref().ok()?.as_ref()?;
    let logical_key = ::chaoscontrol_protocol::identity::AssertionLogicalKey::Stable {
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
    if crate::assert::ASSERTION_CATALOG.len()
        > ::chaoscontrol_protocol::admission::MAX_ASSERTION_CATALOG_ENTRIES
    {
        return SdkCatalog {
            descriptors: Vec::new(),
            accepted: Err(::chaoscontrol_protocol::admission::CatalogConflict::CardinalityOverflow),
        };
    }
    let mut descriptors = Vec::with_capacity(crate::assert::ASSERTION_CATALOG.len());
    for entry in crate::assert::ASSERTION_CATALOG.iter() {
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
    descriptors: &[::chaoscontrol_protocol::identity::AssertionDescriptor],
) -> Result<
    ::chaoscontrol_protocol::admission::AcceptedCatalog,
    ::chaoscontrol_protocol::admission::CatalogConflict,
> {
    let mut builder = ::chaoscontrol_protocol::admission::CatalogBuilder::begin(descriptors.len())?;
    for descriptor in descriptors {
        builder.insert(descriptor.clone())?;
    }
    let mut assertions = std::collections::BTreeMap::new();
    for descriptor in descriptors {
        let fingerprint = descriptor
            .fingerprint()
            .map_err(::chaoscontrol_protocol::admission::CatalogConflict::Descriptor)?;
        let canonical_bytes = descriptor
            .canonical_bytes()
            .map_err(::chaoscontrol_protocol::admission::CatalogConflict::Descriptor)?;
        assertions.entry(fingerprint).or_insert_with(|| {
            chaoscontrol_protocol::admission::AdmittedAssertion {
                descriptor: descriptor.clone(),
                fingerprint,
                canonical_bytes,
            }
        });
    }
    builder.complete(::chaoscontrol_protocol::admission::catalog_token(
        &assertions,
    ))
}

fn descriptor_from_entry(
    entry: &crate::assert::CatalogEntry,
) -> Result<
    ::chaoscontrol_protocol::identity::AssertionDescriptor,
    ::chaoscontrol_protocol::admission::CatalogConflict,
> {
    let logical_key = match entry.logical_key {
        crate::assert::CatalogLogicalKey::Automatic(source_site) => {
            ::chaoscontrol_protocol::identity::AssertionLogicalKey::Automatic {
                source_site: source_site.to_string(),
            }
        }
        crate::assert::CatalogLogicalKey::Stable(key) => {
            ::chaoscontrol_protocol::identity::AssertionLogicalKey::Stable {
                key: key.to_string(),
            }
        }
    };
    let descriptor = ::chaoscontrol_protocol::identity::AssertionDescriptor {
        identity_version: ::chaoscontrol_protocol::identity::ASSERTION_IDENTITY_VERSION,
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
    descriptor
        .validate()
        .map_err(::chaoscontrol_protocol::admission::CatalogConflict::Descriptor)?;
    Ok(descriptor)
}

fn protocol_kind(
    kind: crate::assert::AssertionKind,
) -> chaoscontrol_protocol::identity::AssertionKind {
    match kind {
        crate::assert::AssertionKind::Always => {
            chaoscontrol_protocol::identity::AssertionKind::Always
        }
        crate::assert::AssertionKind::Sometimes => {
            chaoscontrol_protocol::identity::AssertionKind::Sometimes
        }
        crate::assert::AssertionKind::Reachable => {
            chaoscontrol_protocol::identity::AssertionKind::Reachable
        }
        crate::assert::AssertionKind::Unreachable => {
            chaoscontrol_protocol::identity::AssertionKind::Unreachable
        }
    }
}

fn protocol_catalog_kind(
    kind: u8,
) -> Result<
    chaoscontrol_protocol::identity::AssertionKind,
    ::chaoscontrol_protocol::admission::CatalogConflict,
> {
    match kind {
        crate::assert::CATALOG_KIND_ALWAYS => {
            Ok(protocol_kind(crate::assert::AssertionKind::Always))
        }
        crate::assert::CATALOG_KIND_SOMETIMES => {
            Ok(protocol_kind(crate::assert::AssertionKind::Sometimes))
        }
        crate::assert::CATALOG_KIND_REACHABLE => {
            Ok(protocol_kind(crate::assert::AssertionKind::Reachable))
        }
        crate::assert::CATALOG_KIND_UNREACHABLE => {
            Ok(protocol_kind(crate::assert::AssertionKind::Unreachable))
        }
        _ => Err(
            ::chaoscontrol_protocol::admission::CatalogConflict::Descriptor(
                chaoscontrol_protocol::identity::AssertionError::InvalidKind,
            ),
        ),
    }
}

fn normalized_metadata(value: &str) -> String {
    if value.is_empty() {
        return "uncategorized".to_string();
    }
    value.to_string()
}
