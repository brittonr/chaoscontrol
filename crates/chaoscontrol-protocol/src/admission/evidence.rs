#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionEvidenceIdentity {
    pub descriptor: crate::identity::AssertionDescriptor,
    pub fingerprint: crate::identity::AssertionFingerprint,
    pub canonical_descriptor: Vec<u8>,
    pub catalog_token: crate::identity::AssertionFingerprint,
}

impl AssertionEvidenceIdentity {
    pub fn from_admitted(
        assertion: &crate::admission::AdmittedAssertion,
        catalog_token: crate::identity::AssertionFingerprint,
    ) -> Result<Self, crate::admission::CatalogConflict> {
        let identity = Self {
            descriptor: assertion.descriptor.clone(),
            fingerprint: assertion.fingerprint,
            canonical_descriptor: assertion.canonical_bytes.clone(),
            catalog_token,
        };
        identity.validate_for_catalog_admission()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), crate::admission::CatalogConflict> {
        let fingerprint = self
            .descriptor
            .fingerprint()
            .map_err(crate::admission::CatalogConflict::Descriptor)?;
        if fingerprint != self.fingerprint {
            return Err(crate::admission::CatalogConflict::FingerprintCollision);
        }
        let canonical = self
            .descriptor
            .canonical_bytes()
            .map_err(crate::admission::CatalogConflict::Descriptor)?;
        if canonical != self.canonical_descriptor {
            return Err(crate::admission::CatalogConflict::CanonicalMismatch);
        }
        Ok(())
    }

    pub fn validate_for_catalog_admission(&self) -> Result<(), crate::admission::CatalogConflict> {
        self.validate()?;
        let mut builder = crate::admission::CatalogBuilder::begin(1)?;
        builder.insert_with_fingerprint(self.descriptor.clone(), self.fingerprint)?;
        Ok(())
    }

    pub fn validate_for_catalog<'a>(
        &self,
        catalog: &'a crate::admission::AcceptedCatalog,
    ) -> Result<&'a crate::admission::AdmittedAssertion, crate::admission::CatalogConflict> {
        self.validate_for_catalog_admission()?;
        crate::admission::validate_accepted_catalog(catalog)?;
        if self.catalog_token != catalog.token {
            return Err(crate::admission::CatalogConflict::CatalogTokenMismatch);
        }
        let admitted = catalog
            .assertions
            .get(&self.fingerprint)
            .ok_or(crate::admission::CatalogConflict::UnknownFingerprint)?;
        if admitted.descriptor != self.descriptor
            || admitted.canonical_bytes != self.canonical_descriptor
            || admitted.fingerprint != self.fingerprint
        {
            return Err(crate::admission::CatalogConflict::CanonicalMismatch);
        }
        Ok(admitted)
    }

    pub fn validate_compatibility_alias(
        &self,
        alias: u64,
    ) -> Result<(), crate::admission::CatalogConflict> {
        self.validate_for_catalog_admission()?;
        let alias = u32::try_from(alias)
            .map_err(|_| crate::admission::CatalogConflict::CompatibilityAliasConflict)?;
        let matches = self
            .compatibility_id()
            .map_or(alias == 0, |compatibility_id| compatibility_id == alias);
        if !matches {
            return Err(crate::admission::CatalogConflict::CompatibilityAliasConflict);
        }
        Ok(())
    }

    pub const fn compatibility_id(&self) -> Option<u32> {
        self.descriptor.compatibility_id
    }
}

#[cfg(test)]
mod tests {
    use super::AssertionEvidenceIdentity;

    use crate::identity::{AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION};

    const PRESENT_ALIAS: u32 = 7;

    fn identity(
        compatibility_id: Option<u32>,
        logical_key: AssertionLogicalKey,
    ) -> AssertionEvidenceIdentity {
        let descriptor = crate::identity::AssertionDescriptor {
            identity_version: ASSERTION_IDENTITY_VERSION,
            namespace: "org.example.evidence".to_string(),
            logical_key,
            kind: AssertionKind::Always,
            message: "evidence assertion".to_string(),
            source_file: "src/lib.rs".to_string(),
            source_line: 1,
            source_column: 1,
            guest: "guest".to_string(),
            category: "invariant".to_string(),
            compatibility_id,
        };
        let fingerprint = descriptor.fingerprint().expect("fingerprint");
        let canonical_descriptor = descriptor.canonical_bytes().expect("canonical descriptor");
        AssertionEvidenceIdentity {
            descriptor,
            fingerprint,
            canonical_descriptor,
            catalog_token: crate::identity::AssertionFingerprint::ZERO,
        }
    }

    #[test]
    fn redundant_alias_matches_present_or_absent_descriptor_alias() {
        let present = identity(
            Some(PRESENT_ALIAS),
            AssertionLogicalKey::Stable {
                key: "present".to_string(),
            },
        );
        assert!(present
            .validate_compatibility_alias(u64::from(PRESENT_ALIAS))
            .is_ok());
        assert_eq!(
            present.validate_compatibility_alias(0),
            Err(crate::admission::CatalogConflict::CompatibilityAliasConflict)
        );

        let absent = identity(
            None,
            AssertionLogicalKey::Stable {
                key: "absent".to_string(),
            },
        );
        assert!(absent.validate_compatibility_alias(0).is_ok());
        assert_eq!(
            absent.validate_compatibility_alias(u64::from(PRESENT_ALIAS)),
            Err(crate::admission::CatalogConflict::CompatibilityAliasConflict)
        );
    }

    #[test]
    fn canonical_legacy_descriptor_is_not_catalog_authority() {
        let legacy = identity(
            Some(PRESENT_ALIAS),
            AssertionLogicalKey::LegacyU32 { id: PRESENT_ALIAS },
        );
        assert!(legacy.validate().is_ok());
        assert_eq!(
            legacy.validate_for_catalog_admission(),
            Err(crate::admission::CatalogConflict::LegacyIdentityForbidden)
        );
    }
}
