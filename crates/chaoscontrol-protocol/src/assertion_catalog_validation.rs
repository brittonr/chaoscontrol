use crate::assertion_catalog::{
    AcceptedCatalog, CatalogBuilder, CatalogConflict, ASSERTION_CATALOG_VERSION,
    MAX_ASSERTION_CATALOG_ENTRIES,
};
use crate::assertion_identity::{AssertionDescriptor, AssertionLogicalKey};
use std::collections::BTreeMap;

pub fn validate_accepted_catalog(catalog: &AcceptedCatalog) -> Result<(), CatalogConflict> {
    if catalog.catalog_version != ASSERTION_CATALOG_VERSION {
        return Err(CatalogConflict::CatalogVersionMismatch);
    }
    if catalog.status != crate::assertion_catalog::CatalogValidationStatus::Accepted {
        return Err(CatalogConflict::CatalogStatusMismatch);
    }
    if catalog.assertions.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(CatalogConflict::CardinalityOverflow);
    }
    let mut builder = CatalogBuilder::begin(catalog.assertions.len())?;
    for (key, assertion) in &catalog.assertions {
        let canonical = assertion
            .descriptor
            .canonical_bytes()
            .map_err(CatalogConflict::Descriptor)?;
        let fingerprint = assertion
            .descriptor
            .fingerprint()
            .map_err(CatalogConflict::Descriptor)?;
        if canonical != assertion.canonical_bytes {
            return Err(CatalogConflict::CanonicalMismatch);
        }
        if fingerprint != assertion.fingerprint || fingerprint != *key {
            return Err(CatalogConflict::FingerprintCollision);
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
    descriptors: &[AssertionDescriptor],
) -> Result<(), CatalogConflict> {
    if descriptors.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(CatalogConflict::CardinalityOverflow);
    }
    let mut aliases: BTreeMap<(String, u32), Vec<u8>> = BTreeMap::new();
    for descriptor in descriptors {
        let AssertionLogicalKey::LegacyU32 { id } = &descriptor.logical_key else {
            return Err(CatalogConflict::LegacyIdentityForbidden);
        };
        if descriptor.compatibility_id != Some(*id) {
            return Err(CatalogConflict::LegacyAliasConflict);
        }
        let canonical = descriptor
            .canonical_bytes()
            .map_err(CatalogConflict::Descriptor)?;
        let alias = (descriptor.namespace.clone(), *id);
        if aliases
            .insert(alias, canonical.clone())
            .is_some_and(|existing| existing != canonical)
        {
            return Err(CatalogConflict::LegacyAliasConflict);
        }
    }
    Ok(())
}

pub(crate) fn classify_descriptor_conflict(
    existing: &AssertionDescriptor,
    candidate: &AssertionDescriptor,
) -> CatalogConflict {
    if matches!(existing.logical_key, AssertionLogicalKey::LegacyU32 { .. }) {
        return CatalogConflict::LegacyAliasConflict;
    }
    if existing.namespace != candidate.namespace {
        return CatalogConflict::NamespaceConflict;
    }
    if existing.kind != candidate.kind {
        return CatalogConflict::KindConflict;
    }
    if existing.message != candidate.message {
        return CatalogConflict::MessageConflict;
    }
    if existing.source_file != candidate.source_file
        || existing.source_line != candidate.source_line
        || existing.source_column != candidate.source_column
    {
        return CatalogConflict::SourceConflict;
    }
    if existing.guest != candidate.guest {
        return CatalogConflict::GuestConflict;
    }
    if existing.category != candidate.category {
        return CatalogConflict::CategoryConflict;
    }
    CatalogConflict::LogicalKeyConflict
}
