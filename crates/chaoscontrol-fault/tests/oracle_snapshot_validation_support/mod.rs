use crate::oracle_snapshot_common::{descriptor, COMPATIBILITY_ID};
use chaoscontrol_protocol::admission::{
    catalog_token, AcceptedCatalog, AdmittedAssertion, CatalogValidationStatus,
    ASSERTION_CATALOG_VERSION,
};
use chaoscontrol_protocol::identity::AssertionLogicalKey;
use std::collections::BTreeMap;

pub const FUTURE_RUN_ID: u32 = 2;

pub fn legacy_catalog() -> (AcceptedCatalog, AdmittedAssertion) {
    let mut descriptor = descriptor();
    descriptor.namespace = "legacy:guest".to_string();
    descriptor.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    let fingerprint = descriptor.fingerprint().expect("legacy fingerprint");
    let admitted = AdmittedAssertion {
        canonical_bytes: descriptor.canonical_bytes().expect("legacy canonical"),
        descriptor,
        fingerprint,
    };
    let assertions = BTreeMap::from([(fingerprint, admitted.clone())]);
    let token = catalog_token(&assertions);
    (
        AcceptedCatalog {
            catalog_version: ASSERTION_CATALOG_VERSION,
            token,
            status: CatalogValidationStatus::Accepted,
            assertions,
        },
        admitted,
    )
}
