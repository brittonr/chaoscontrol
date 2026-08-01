pub use crate::assertion_catalog_token::{catalog_token, token_for_descriptors};
use crate::assertion_catalog_validation::classify_descriptor_conflict;
pub use crate::assertion_catalog_validation::{
    validate_accepted_catalog, validate_legacy_descriptors,
};
use crate::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey, IdentityError,
    MAX_ASSERTION_CANONICAL_BYTES,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_ASSERTION_CATALOG_ENTRIES: usize = 4096;
pub const MAX_ASSERTION_REPORT_ENTRIES: usize = MAX_ASSERTION_CATALOG_ENTRIES;
pub const ASSERTION_CATALOG_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogValidationStatus {
    Pending,
    Accepted,
    FatalConflict,
    LegacyAmbiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedAssertion {
    pub descriptor: AssertionDescriptor,
    pub fingerprint: AssertionFingerprint,
    pub canonical_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedCatalog {
    pub catalog_version: u8,
    pub token: AssertionFingerprint,
    pub status: CatalogValidationStatus,
    pub assertions: BTreeMap<AssertionFingerprint, AdmittedAssertion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogConflict {
    AlreadyBegun,
    CanonicalMismatch,
    CardinalityOverflow,
    CatalogStatusMismatch,
    CatalogVersionMismatch,
    CounterOverflow,
    CatalogIncomplete,
    CatalogTokenMismatch,
    Descriptor(IdentityError),
    EmptyCatalog,
    FingerprintCollision,
    GuestConflict,
    KindConflict,
    LegacyAliasConflict,
    LegacyIdentityForbidden,
    LogicalKeyConflict,
    MessageConflict,
    NamespaceConflict,
    NoActiveRun,
    PostCompletion,
    PostConflict,
    SourceConflict,
    CategoryConflict,
    CompatibilityAliasConflict,
    UnexpectedDescriptorCount,
    UnknownFingerprint,
    EventCatalogMismatch,
    EventKindMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogInsert {
    Inserted,
    ExactDuplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundAssertionEvent {
    pub catalog_token: AssertionFingerprint,
    pub fingerprint: AssertionFingerprint,
    pub kind: AssertionKind,
}

#[derive(Debug, Clone)]
pub struct CatalogBuilder {
    expected_frames: usize,
    received_frames: usize,
    completed: bool,
    conflict: Option<CatalogConflict>,
    by_logical_key: BTreeMap<(String, AssertionLogicalKey), AdmittedAssertion>,
    by_compatibility_id: BTreeMap<(String, u32), AdmittedAssertion>,
    pub(crate) by_fingerprint: BTreeMap<AssertionFingerprint, AdmittedAssertion>,
}

impl CatalogBuilder {
    pub fn begin(expected_frames: usize) -> Result<Self, CatalogConflict> {
        if expected_frames == 0 {
            return Err(CatalogConflict::EmptyCatalog);
        }
        if expected_frames > MAX_ASSERTION_CATALOG_ENTRIES {
            return Err(CatalogConflict::CardinalityOverflow);
        }
        Ok(Self {
            expected_frames,
            received_frames: 0,
            completed: false,
            conflict: None,
            by_logical_key: BTreeMap::new(),
            by_compatibility_id: BTreeMap::new(),
            by_fingerprint: BTreeMap::new(),
        })
    }

    pub fn insert(
        &mut self,
        descriptor: AssertionDescriptor,
    ) -> Result<CatalogInsert, CatalogConflict> {
        let fingerprint = match descriptor.fingerprint() {
            Ok(fingerprint) => fingerprint,
            Err(error) => return self.fail(CatalogConflict::Descriptor(error)),
        };
        self.insert_with_fingerprint(descriptor, fingerprint)
    }

    pub fn insert_with_fingerprint(
        &mut self,
        descriptor: AssertionDescriptor,
        fingerprint: AssertionFingerprint,
    ) -> Result<CatalogInsert, CatalogConflict> {
        if self.completed {
            return Err(CatalogConflict::PostCompletion);
        }
        if self.conflict.is_some() {
            return Err(CatalogConflict::PostConflict);
        }
        if matches!(
            &descriptor.logical_key,
            AssertionLogicalKey::LegacyU32 { .. }
        ) {
            return self.fail(CatalogConflict::LegacyIdentityForbidden);
        }
        self.received_frames = match self.received_frames.checked_add(1) {
            Some(count) => count,
            None => return self.fail(CatalogConflict::CounterOverflow),
        };
        if self.received_frames > self.expected_frames {
            return self.fail(CatalogConflict::UnexpectedDescriptorCount);
        }
        let canonical_bytes = match descriptor.canonical_bytes() {
            Ok(canonical) => canonical,
            Err(error) => return self.fail(CatalogConflict::Descriptor(error)),
        };
        if canonical_bytes.len() > MAX_ASSERTION_CANONICAL_BYTES {
            return self.fail(CatalogConflict::Descriptor(IdentityError::FieldTooLong(
                "canonical_descriptor",
            )));
        }
        let logical_key = (descriptor.namespace.clone(), descriptor.logical_key.clone());
        if let Some(existing) = self.by_logical_key.get(&logical_key) {
            if existing.canonical_bytes == canonical_bytes && existing.fingerprint == fingerprint {
                return Ok(CatalogInsert::ExactDuplicate);
            }
            return self.fail(classify_descriptor_conflict(
                &existing.descriptor,
                &descriptor,
            ));
        }
        if let Some(existing) = self.by_fingerprint.get(&fingerprint) {
            if existing.canonical_bytes != canonical_bytes {
                return self.fail(CatalogConflict::FingerprintCollision);
            }
        }
        if let Some(compatibility_id) = descriptor.compatibility_id {
            let alias_key = (descriptor.namespace.clone(), compatibility_id);
            if let Some(existing) = self.by_compatibility_id.get(&alias_key) {
                if existing.canonical_bytes != canonical_bytes {
                    let conflict = if matches!(
                        descriptor.logical_key,
                        AssertionLogicalKey::LegacyU32 { .. }
                    ) || matches!(
                        existing.descriptor.logical_key,
                        AssertionLogicalKey::LegacyU32 { .. }
                    ) {
                        CatalogConflict::LegacyAliasConflict
                    } else {
                        CatalogConflict::CompatibilityAliasConflict
                    };
                    return self.fail(conflict);
                }
            }
        }
        if self.by_fingerprint.len() >= MAX_ASSERTION_CATALOG_ENTRIES {
            return self.fail(CatalogConflict::CardinalityOverflow);
        }
        let admitted = AdmittedAssertion {
            descriptor,
            fingerprint,
            canonical_bytes,
        };
        self.by_logical_key.insert(logical_key, admitted.clone());
        if let Some(compatibility_id) = admitted.descriptor.compatibility_id {
            let alias_key = (admitted.descriptor.namespace.clone(), compatibility_id);
            self.by_compatibility_id.insert(alias_key, admitted.clone());
        }
        self.by_fingerprint.insert(fingerprint, admitted);
        Ok(CatalogInsert::Inserted)
    }

    pub fn complete(
        mut self,
        claimed_token: AssertionFingerprint,
    ) -> Result<AcceptedCatalog, CatalogConflict> {
        if let Some(conflict) = self.conflict {
            return Err(conflict);
        }
        if self.completed {
            return Err(CatalogConflict::PostCompletion);
        }
        if self.received_frames != self.expected_frames {
            return Err(CatalogConflict::CatalogIncomplete);
        }
        for (key, admitted) in &self.by_fingerprint {
            let canonical = admitted
                .descriptor
                .canonical_bytes()
                .map_err(CatalogConflict::Descriptor)?;
            let fingerprint = admitted
                .descriptor
                .fingerprint()
                .map_err(CatalogConflict::Descriptor)?;
            if *key != fingerprint || admitted.fingerprint != fingerprint {
                return Err(CatalogConflict::Descriptor(
                    IdentityError::InvalidFingerprint,
                ));
            }
            if admitted.canonical_bytes != canonical {
                return Err(CatalogConflict::CanonicalMismatch);
            }
        }
        let token = catalog_token(&self.by_fingerprint);
        if token != claimed_token {
            return Err(CatalogConflict::CatalogTokenMismatch);
        }
        self.completed = true;
        Ok(AcceptedCatalog {
            catalog_version: ASSERTION_CATALOG_VERSION,
            token,
            status: CatalogValidationStatus::Accepted,
            assertions: self.by_fingerprint,
        })
    }

    pub fn expected_frames(&self) -> usize {
        self.expected_frames
    }

    pub fn received_frames(&self) -> usize {
        self.received_frames
    }

    fn fail<T>(&mut self, conflict: CatalogConflict) -> Result<T, CatalogConflict> {
        self.conflict = Some(conflict.clone());
        Err(conflict)
    }
}

impl AcceptedCatalog {
    pub fn resolve_event(
        &self,
        event: &BoundAssertionEvent,
    ) -> Result<&AdmittedAssertion, CatalogConflict> {
        if self.status != CatalogValidationStatus::Accepted {
            return Err(CatalogConflict::PostConflict);
        }
        if self.assertions.values().any(|assertion| {
            matches!(
                &assertion.descriptor.logical_key,
                AssertionLogicalKey::LegacyU32 { .. }
            )
        }) {
            return Err(CatalogConflict::LegacyIdentityForbidden);
        }
        if event.catalog_token != self.token {
            return Err(CatalogConflict::EventCatalogMismatch);
        }
        let admitted = self
            .assertions
            .get(&event.fingerprint)
            .ok_or(CatalogConflict::UnknownFingerprint)?;
        if admitted.descriptor.kind != event.kind {
            return Err(CatalogConflict::EventKindMismatch);
        }
        Ok(admitted)
    }
}
