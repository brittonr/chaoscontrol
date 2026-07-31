use crate::assertion_catalog::{
    AcceptedCatalog, CatalogBuilder, CatalogConflict, ASSERTION_CATALOG_VERSION,
    MAX_ASSERTION_CATALOG_ENTRIES,
};
use crate::assertion_identity::{AssertionDescriptor, AssertionLogicalKey};

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
