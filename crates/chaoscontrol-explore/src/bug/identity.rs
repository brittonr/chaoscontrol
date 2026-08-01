use chaoscontrol_fault::oracle::{AssertionRecord, OracleReport};
use chaoscontrol_protocol::admission::AssertionEvidenceIdentity;
use snafu::Snafu;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Snafu)]
pub enum BugIdentityError {
    #[snafu(display("legacy assertion ID-only bug is diagnostic-only and cannot replay"))]
    Missing,
    #[snafu(display("bug assertion identity is invalid"))]
    Invalid,
    #[snafu(display("bug assertion alias is outside the u32 range"))]
    AliasRange,
    #[snafu(display("bug assertion alias conflicts with its exact descriptor"))]
    AliasMismatch,
    #[snafu(display("bug assertion identity does not match the restored assertion report"))]
    ReportMismatch,
    #[snafu(display("bug carrier shape conflicts with replay identity requirements"))]
    MalformedCarrier,
    #[snafu(display("bug artifact hash does not bind the loaded bug carrier"))]
    ArtifactMismatch,
}

pub fn validate_carrier(
    assertion_id: u64,
    identity: Option<&AssertionEvidenceIdentity>,
) -> Result<&AssertionEvidenceIdentity, BugIdentityError> {
    let identity = identity.ok_or(BugIdentityError::Missing)?;
    identity
        .validate_for_catalog_admission()
        .map_err(|_| BugIdentityError::Invalid)?;
    let alias = u32::try_from(assertion_id).map_err(|_| BugIdentityError::AliasRange)?;
    let matches = identity
        .compatibility_id()
        .map_or(alias == 0, |compatibility_id| compatibility_id == alias);
    if !matches {
        return Err(BugIdentityError::AliasMismatch);
    }
    Ok(identity)
}

pub fn resolve_restored_report<'a>(
    assertion_id: u64,
    identity: Option<&AssertionEvidenceIdentity>,
    report: &'a OracleReport,
) -> Result<&'a AssertionRecord, BugIdentityError> {
    let identity = validate_carrier(assertion_id, identity)?;
    chaoscontrol_fault::oracle_validation::resolve_assertion_evidence(report, identity)
        .map_err(|_| BugIdentityError::ReportMismatch)
}

#[cfg(test)]
mod tests {
    use super::{resolve_restored_report, validate_carrier, BugIdentityError};
    use chaoscontrol_fault::oracle::{PropertyOracle, Verdict};
    use chaoscontrol_protocol::admission::{
        token_for_descriptors, AssertionEvidenceIdentity, BoundAssertionEvent, CatalogBuilder,
    };
    use chaoscontrol_protocol::identity::{
        AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
    };

    const SHARED_ALIAS: u32 = 17;

    fn descriptor(namespace: &str, key: &str, message: &str) -> AssertionDescriptor {
        AssertionDescriptor {
            identity_version: ASSERTION_IDENTITY_VERSION,
            namespace: namespace.to_string(),
            logical_key: AssertionLogicalKey::Stable {
                key: key.to_string(),
            },
            kind: AssertionKind::Always,
            message: message.to_string(),
            source_file: "src/main.rs".to_string(),
            source_line: 1,
            source_column: 1,
            guest: "guest".to_string(),
            category: "invariant".to_string(),
            compatibility_id: Some(SHARED_ALIAS),
        }
    }

    fn report_and_identities() -> (
        chaoscontrol_fault::oracle::OracleReport,
        AssertionEvidenceIdentity,
        AssertionEvidenceIdentity,
    ) {
        let first = descriptor("org.example.first", "first", "first assertion");
        let second = descriptor("org.example.second", "second", "second assertion");
        let descriptors = vec![first, second];
        let token = token_for_descriptors(&descriptors).expect("catalog token");
        let mut builder = CatalogBuilder::begin(descriptors.len()).expect("catalog begins");
        for descriptor in descriptors {
            builder.insert(descriptor).expect("descriptor inserts");
        }
        let catalog = builder.complete(token).expect("catalog completes");
        let first_admitted = catalog
            .assertions
            .values()
            .find(|item| item.descriptor.message == "first assertion")
            .expect("first assertion")
            .clone();
        let second_admitted = catalog
            .assertions
            .values()
            .find(|item| item.descriptor.message == "second assertion")
            .expect("second assertion")
            .clone();
        let first_identity = AssertionEvidenceIdentity::from_admitted(&first_admitted, token)
            .expect("first identity");
        let second_identity = AssertionEvidenceIdentity::from_admitted(&second_admitted, token)
            .expect("second identity");
        let mut oracle = PropertyOracle::new();
        oracle.activate_catalog(catalog).expect("catalog activates");
        oracle.begin_run();
        oracle
            .record_bound_event(
                &BoundAssertionEvent {
                    catalog_token: token,
                    fingerprint: first_identity.fingerprint,
                    kind: AssertionKind::Always,
                },
                false,
                None,
            )
            .expect("failure records");
        oracle.end_run();
        (oracle.report(), first_identity, second_identity)
    }

    #[test]
    fn exact_identity_resolves_despite_alias_collision() {
        let (report, first, second) = report_and_identities();
        assert!(report.record_for_compatibility_id(SHARED_ALIAS).is_err());

        let first_record = resolve_restored_report(SHARED_ALIAS.into(), Some(&first), &report)
            .expect("first identity resolves");
        assert_eq!(first_record.message, "first assertion");
        assert_eq!(first_record.verdict(), Verdict::Failed);
        let second_record = resolve_restored_report(SHARED_ALIAS.into(), Some(&second), &report)
            .expect("second identity resolves");
        assert_eq!(second_record.message, "second assertion");
    }

    #[test]
    fn rejects_forged_descriptor() {
        let (_, mut identity, _) = report_and_identities();
        identity.descriptor.message = "forged".to_string();
        assert_eq!(
            validate_carrier(SHARED_ALIAS.into(), Some(&identity)),
            Err(BugIdentityError::Invalid)
        );
    }

    #[test]
    fn rejects_catalog_token_substitution() {
        let (report, mut identity, _) = report_and_identities();
        identity.catalog_token = chaoscontrol_protocol::identity::AssertionFingerprint::ZERO;
        assert_eq!(
            resolve_restored_report(SHARED_ALIAS.into(), Some(&identity), &report),
            Err(BugIdentityError::ReportMismatch)
        );
    }

    #[test]
    fn rejects_alias_substitution() {
        let (_, identity, _) = report_and_identities();
        let substituted_alias = u64::from(SHARED_ALIAS) + 1;
        assert_eq!(
            validate_carrier(substituted_alias, Some(&identity)),
            Err(BugIdentityError::AliasMismatch)
        );
    }

    #[test]
    fn accepts_zero_alias_when_descriptor_has_no_compatibility_alias() {
        let (_, mut identity, _) = report_and_identities();
        identity.descriptor.compatibility_id = None;
        identity.fingerprint = identity.descriptor.fingerprint().expect("fingerprint");
        identity.canonical_descriptor = identity.descriptor.canonical_bytes().expect("canonical");

        assert!(validate_carrier(0, Some(&identity)).is_ok());
        assert_eq!(
            validate_carrier(1, Some(&identity)),
            Err(BugIdentityError::AliasMismatch)
        );
    }

    #[test]
    fn rejects_canonical_legacy_descriptor() {
        let descriptor = AssertionDescriptor {
            logical_key: AssertionLogicalKey::LegacyU32 { id: SHARED_ALIAS },
            ..descriptor("org.example.legacy", "unused", "legacy assertion")
        };
        let identity = AssertionEvidenceIdentity {
            fingerprint: descriptor.fingerprint().expect("fingerprint"),
            canonical_descriptor: descriptor.canonical_bytes().expect("canonical"),
            descriptor,
            catalog_token: chaoscontrol_protocol::identity::AssertionFingerprint::ZERO,
        };
        assert!(identity.validate().is_ok());
        assert_eq!(
            validate_carrier(SHARED_ALIAS.into(), Some(&identity)),
            Err(BugIdentityError::Invalid)
        );
    }

    #[test]
    fn rejects_legacy_id_only_bug() {
        assert_eq!(
            validate_carrier(SHARED_ALIAS.into(), None),
            Err(BugIdentityError::Missing)
        );
    }
}
