pub fn validate_accepted_catalog(
    catalog: &crate::admission::AcceptedCatalog,
) -> Result<(), crate::admission::CatalogConflict> {
    if catalog.catalog_version != crate::admission::ASSERTION_CATALOG_VERSION {
        return Err(crate::admission::CatalogConflict::CatalogVersionMismatch);
    }
    if catalog.status != crate::admission::CatalogValidationStatus::Accepted {
        return Err(crate::admission::CatalogConflict::CatalogStatusMismatch);
    }
    if catalog.assertions.len() > crate::admission::MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(crate::admission::CatalogConflict::CardinalityOverflow);
    }
    let mut builder = crate::admission::CatalogBuilder::begin(catalog.assertions.len())?;
    for (key, assertion) in &catalog.assertions {
        let canonical = assertion
            .descriptor
            .canonical_bytes()
            .map_err(crate::admission::CatalogConflict::Descriptor)?;
        let fingerprint = assertion
            .descriptor
            .fingerprint()
            .map_err(crate::admission::CatalogConflict::Descriptor)?;
        if canonical != assertion.canonical_bytes {
            return Err(crate::admission::CatalogConflict::CanonicalMismatch);
        }
        if fingerprint != assertion.fingerprint || fingerprint != *key {
            return Err(crate::admission::CatalogConflict::FingerprintCollision);
        }
        builder.insert_with_fingerprint(assertion.descriptor.clone(), *key)?;
    }
    builder.complete(catalog.token)?;
    Ok(())
}

/// Validate legacy descriptors for diagnostic quarantine only.
///
/// A successful result does not admit these descriptors to a strict catalog.
pub fn validate_legacy_descriptors(
    descriptors: &[crate::identity::AssertionDescriptor],
) -> Result<(), crate::admission::CatalogConflict> {
    if descriptors.len() > crate::admission::MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(crate::admission::CatalogConflict::CardinalityOverflow);
    }
    let mut aliases: std::collections::BTreeMap<(String, u32), Vec<u8>> =
        std::collections::BTreeMap::new();
    for descriptor in descriptors {
        let crate::identity::AssertionLogicalKey::LegacyU32 { id } = &descriptor.logical_key else {
            return Err(crate::admission::CatalogConflict::LegacyIdentityForbidden);
        };
        if descriptor.compatibility_id != Some(*id) {
            return Err(crate::admission::CatalogConflict::LegacyAliasConflict);
        }
        let canonical = descriptor
            .canonical_bytes()
            .map_err(crate::admission::CatalogConflict::Descriptor)?;
        let alias = (descriptor.namespace.clone(), *id);
        if aliases
            .insert(alias, canonical.clone())
            .is_some_and(|existing| existing != canonical)
        {
            return Err(crate::admission::CatalogConflict::LegacyAliasConflict);
        }
    }
    Ok(())
}

pub(crate) fn classify_descriptor_conflict(
    existing: &crate::identity::AssertionDescriptor,
    candidate: &crate::identity::AssertionDescriptor,
) -> crate::admission::CatalogConflict {
    if matches!(
        existing.logical_key,
        crate::identity::AssertionLogicalKey::LegacyU32 { .. }
    ) {
        return crate::admission::CatalogConflict::LegacyAliasConflict;
    }
    if existing.namespace != candidate.namespace {
        return crate::admission::CatalogConflict::NamespaceConflict;
    }
    if existing.kind != candidate.kind {
        return crate::admission::CatalogConflict::KindConflict;
    }
    if existing.message != candidate.message {
        return crate::admission::CatalogConflict::MessageConflict;
    }
    if existing.source_file != candidate.source_file
        || existing.source_line != candidate.source_line
        || existing.source_column != candidate.source_column
    {
        return crate::admission::CatalogConflict::SourceConflict;
    }
    if existing.guest != candidate.guest {
        return crate::admission::CatalogConflict::GuestConflict;
    }
    if existing.category != candidate.category {
        return crate::admission::CatalogConflict::CategoryConflict;
    }
    crate::admission::CatalogConflict::LogicalKeyConflict
}
