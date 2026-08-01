use crate::assertion_catalog::{
    AdmittedAssertion, CatalogBuilder, CatalogConflict, MAX_ASSERTION_CATALOG_ENTRIES,
};
use crate::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, MAX_ASSERTION_CANONICAL_BYTES,
};
use std::collections::BTreeMap;

const CATALOG_DOMAIN: &[u8] = b"chaoscontrol.assertion-catalog.v1\0";

pub fn catalog_token(
    assertions: &BTreeMap<AssertionFingerprint, AdmittedAssertion>,
) -> AssertionFingerprint {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CATALOG_DOMAIN);
    hasher.update(&(assertions.len() as u64).to_le_bytes());
    for (fingerprint, assertion) in assertions {
        debug_assert!(assertion.canonical_bytes.len() <= MAX_ASSERTION_CANONICAL_BYTES);
        hasher.update(&fingerprint.0);
        hasher.update(&(assertion.canonical_bytes.len() as u64).to_le_bytes());
        hasher.update(&assertion.canonical_bytes);
    }
    AssertionFingerprint(*hasher.finalize().as_bytes())
}

pub fn token_for_descriptors(
    descriptors: &[AssertionDescriptor],
) -> Result<AssertionFingerprint, CatalogConflict> {
    if descriptors.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(CatalogConflict::CardinalityOverflow);
    }
    let mut builder = CatalogBuilder::begin(descriptors.len())?;
    for descriptor in descriptors {
        builder.insert(descriptor.clone())?;
    }
    Ok(catalog_token(&builder.by_fingerprint))
}
