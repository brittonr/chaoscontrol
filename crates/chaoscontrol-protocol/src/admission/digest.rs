const CATALOG_DOMAIN: &[u8] = b"chaoscontrol.assertion-catalog.v1\0";

pub fn catalog_token(
    assertions: &std::collections::BTreeMap<
        crate::identity::AssertionFingerprint,
        crate::admission::AdmittedAssertion,
    >,
) -> crate::identity::AssertionFingerprint {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CATALOG_DOMAIN);
    hasher.update(&(assertions.len() as u64).to_le_bytes());
    for (fingerprint, assertion) in assertions {
        debug_assert!(
            assertion.canonical_bytes.len() <= crate::identity::MAX_ASSERTION_CANONICAL_BYTES
        );
        hasher.update(&fingerprint.0);
        hasher.update(&(assertion.canonical_bytes.len() as u64).to_le_bytes());
        hasher.update(&assertion.canonical_bytes);
    }
    crate::identity::AssertionFingerprint(*hasher.finalize().as_bytes())
}

pub fn token_for_descriptors(
    descriptors: &[crate::identity::AssertionDescriptor],
) -> Result<crate::identity::AssertionFingerprint, crate::admission::CatalogConflict> {
    if descriptors.len() > crate::admission::MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(crate::admission::CatalogConflict::CardinalityOverflow);
    }
    let mut builder = crate::admission::CatalogBuilder::begin(descriptors.len())?;
    for descriptor in descriptors {
        builder.insert(descriptor.clone())?;
    }
    Ok(catalog_token(&builder.by_fingerprint))
}
