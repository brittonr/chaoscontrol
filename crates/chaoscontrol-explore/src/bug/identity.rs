use crate::checkpoint::BugSetIdentityError;
use crate::corpus::BugReport;
use crate::explorer::AssertionDetail;
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

pub fn validate_fallback_scope(
    identity: &AssertionEvidenceIdentity,
    scope: Option<&chaoscontrol_protocol::fallback::FallbackAssertionScope>,
) -> Result<(), BugIdentityError> {
    let is_fallback = identity.descriptor.category
        == chaoscontrol_protocol::fallback::FALLBACK_ASSERTION_CATEGORY;
    match (is_fallback, scope) {
        (false, None) => Ok(()),
        (true, Some(scope)) => scope
            .validate_against(identity)
            .map_err(|_| BugIdentityError::MalformedCarrier),
        (false, Some(_)) | (true, None) => Err(BugIdentityError::MalformedCarrier),
    }
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

pub(crate) fn detail_matches_identity(
    detail: &AssertionDetail,
    identity: &AssertionEvidenceIdentity,
) -> bool {
    let Some(candidate) = detail.identity.as_ref() else {
        return false;
    };
    candidate.descriptor == identity.descriptor
        && candidate.fingerprint == identity.fingerprint
        && candidate.canonical_descriptor
            == chaoscontrol_protocol::identity::encode_lower_hex(&identity.canonical_descriptor)
        && candidate.catalog_tokens.as_slice() == [identity.catalog_token]
}

pub fn validate_reported_bug_identities(
    bugs: &[BugReport],
    assertions: &[AssertionDetail],
) -> Result<(), BugSetIdentityError> {
    for bug in bugs {
        let identity = validate_carrier(bug.assertion_id, Some(&bug.assertion_identity)).map_err(
            |source| BugSetIdentityError {
                bug_id: bug.bug_id,
                source,
            },
        )?;
        validate_fallback_scope(identity, bug.fallback_scope.as_ref()).map_err(|source| {
            BugSetIdentityError {
                bug_id: bug.bug_id,
                source,
            }
        })?;
        let exact_matches = assertions
            .iter()
            .filter(|detail| detail_matches_identity(detail, identity))
            .count();
        if exact_matches != 1 {
            return Err(BugSetIdentityError {
                bug_id: bug.bug_id,
                source: BugIdentityError::ReportMismatch,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_restored_report, validate_carrier, validate_fallback_scope, BugIdentityError,
    };
    use chaoscontrol_fault::oracle::{PropertyOracle, Verdict};
    use chaoscontrol_protocol::admission::{
        token_for_descriptors, AssertionEvidenceIdentity, BoundAssertionEvent, CatalogBuilder,
    };
    use chaoscontrol_protocol::fallback::{
        FallbackProcessIdentity, FallbackRecord, FallbackRecordType, FallbackSink,
        FALLBACK_RECORD_SCHEMA_VERSION,
    };
    use chaoscontrol_protocol::identity::{
        AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
    };

    const SHARED_ALIAS: u32 = 17;
    const FALLBACK_RECORD_LIMIT: usize = 1;
    const FALLBACK_RECORD_SEQUENCE: u64 = 0;

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

    fn fallback_identity_and_scope() -> (
        AssertionEvidenceIdentity,
        chaoscontrol_protocol::fallback::FallbackAssertionScope,
    ) {
        let record = FallbackRecord {
            schema_version: FALLBACK_RECORD_SCHEMA_VERSION,
            sequence: FALLBACK_RECORD_SEQUENCE,
            process: FallbackProcessIdentity {
                guest: "guest".to_string(),
                process: "wal-worker".to_string(),
            },
            namespace: "org.example.fallback".to_string(),
            logical_key: "wal-safe".to_string(),
            record_type: FallbackRecordType::Always,
            condition: Some(false),
            message: "WAL state remains safe".to_string(),
            details: serde_json::json!({}),
        };
        let descriptor = record
            .assertion_descriptor()
            .expect("descriptor result")
            .expect("assertion descriptor");
        let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
        let mut builder = CatalogBuilder::begin(FALLBACK_RECORD_LIMIT).expect("builder");
        builder.insert(descriptor).expect("descriptor");
        let catalog = builder.complete(token).expect("catalog");
        let admitted = catalog.assertions.values().next().expect("admitted");
        let identity = AssertionEvidenceIdentity::from_admitted(admitted, token).expect("identity");
        let mut sink = FallbackSink::new(FALLBACK_RECORD_LIMIT).expect("sink");
        sink.admit_line(&serde_json::to_string(&record).expect("line"))
            .expect("admitted line");
        let evidence = sink.evidence().expect("evidence");
        let scope = record
            .assertion_scope(&evidence.sink_blake3)
            .expect("scope result")
            .expect("scope");
        (identity, scope)
    }

    #[test]
    fn fallback_bug_identity_requires_exact_process_scope() {
        let (identity, scope) = fallback_identity_and_scope();
        validate_fallback_scope(&identity, Some(&scope)).expect("valid process scope");
        assert_eq!(
            validate_fallback_scope(&identity, None),
            Err(BugIdentityError::MalformedCarrier)
        );
        let normal = test_identity_for_scope();
        assert_eq!(
            validate_fallback_scope(&normal, Some(&scope)),
            Err(BugIdentityError::MalformedCarrier)
        );
    }

    fn test_identity_for_scope() -> AssertionEvidenceIdentity {
        let descriptor = descriptor("org.example.scope", "normal", "normal assertion");
        let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
        let mut builder = CatalogBuilder::begin(FALLBACK_RECORD_LIMIT).expect("builder");
        builder.insert(descriptor).expect("descriptor");
        let catalog = builder.complete(token).expect("catalog");
        AssertionEvidenceIdentity::from_admitted(
            catalog.assertions.values().next().expect("admitted"),
            token,
        )
        .expect("identity")
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
