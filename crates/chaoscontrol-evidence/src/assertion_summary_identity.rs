use serde::de::Deserialize;

const ASSERTION_SUMMARY_SCHEMA: &str = "chaoscontrol.assertion-summary.v2";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryEnvelope {
    schema: String,
    catalog_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus,
    collision_safe_evidence: bool,
    assertions: Vec<ReviewAssertion>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewIdentity {
    descriptor: ::chaoscontrol_protocol::identity::AssertionDescriptor,
    fingerprint: ::chaoscontrol_protocol::identity::AssertionFingerprint,
    canonical_descriptor: String,
    catalog_tokens: Vec<::chaoscontrol_protocol::identity::AssertionFingerprint>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewAssertion {
    id: u32,
    #[serde(
        default = "no_review_identity",
        deserialize_with = "crate::non_null_option::deserialize"
    )]
    identity: Option<ReviewIdentity>,
    message: String,
    kind: String,
    guest: String,
    category: String,
    verdict: String,
    hit_count: u64,
    true_count: u64,
    false_count: u64,
    last_failure_details: Option<String>,
}

fn no_review_identity() -> Option<ReviewIdentity> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryIdentityStatus {
    CollisionSafe,
    LegacyAmbiguous,
}

pub(crate) fn validate(
    value: &::serde_json::Value,
    require_collision_safe: bool,
) -> crate::EvidenceResult<SummaryIdentityStatus> {
    let items = value
        .as_array()
        .or_else(|| {
            value
                .get("assertions")
                .and_then(::serde_json::Value::as_array)
        })
        .ok_or_else(|| {
            crate::EvidenceError::new("assertion-summary: expected array or v2 object")
        })?;
    if items.is_empty() {
        return Err(crate::EvidenceError::new(
            "assertion-summary: expected non-empty array",
        ));
    }
    if items.len() > ::chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES {
        return Err(crate::EvidenceError::new(
            "assertion-summary: entry count exceeds the limit",
        ));
    }
    let (assertions, classification) = if value.is_array() {
        (
            Vec::<ReviewAssertion>::deserialize(value).map_err(|error| {
                crate::EvidenceError::new(format!("assertion-summary: {error}"))
            })?,
            None,
        )
    } else {
        let envelope = SummaryEnvelope::deserialize(value)
            .map_err(|error| crate::EvidenceError::new(format!("assertion-summary: {error}")))?;
        if envelope.schema != ASSERTION_SUMMARY_SCHEMA {
            return Err(crate::EvidenceError::new(
                "assertion-summary: unsupported schema",
            ));
        }
        (
            envelope.assertions,
            Some((envelope.catalog_status, envelope.collision_safe_evidence)),
        )
    };
    if classification.is_some_and(|(status, _)| {
        status == ::chaoscontrol_protocol::admission::CatalogValidationStatus::Pending
    }) {
        return Err(crate::EvidenceError::new(
            "assertion-summary: pending is not a v2 status",
        ));
    }
    let identified = assertions
        .iter()
        .filter(|assertion| assertion.identity.is_some())
        .count();
    if identified == 0 {
        match classification {
            None
            | Some((
                ::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous,
                false,
            )) => {
                validate_legacy(&assertions)?;
            }
            Some((
                ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
                false,
            )) => {
                assertions.iter().try_for_each(validate_common)?;
            }
            _ => {
                return Err(crate::EvidenceError::new(
                    "assertion-summary: legacy classification is invalid",
                ))
            }
        }
        if require_collision_safe {
            return Err(crate::EvidenceError::new(
                "assertion-summary: legacy or fatal evidence cannot be promoted",
            ));
        }
        return Ok(SummaryIdentityStatus::LegacyAmbiguous);
    }
    if identified != assertions.len() {
        if classification
            != Some((
                ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
                false,
            ))
        {
            return Err(crate::EvidenceError::new(
                "assertion-summary: mixed legacy and structured identities require fatal classification",
            ));
        }
        for assertion in &assertions {
            validate_common(assertion)?;
            if assertion.identity.is_some() {
                validate_strict_identity(assertion)?;
            }
        }
        if require_collision_safe {
            return Err(crate::EvidenceError::new(
                "assertion-summary: fatal evidence cannot be promoted",
            ));
        }
        return Ok(SummaryIdentityStatus::LegacyAmbiguous);
    }
    match classification {
        None => {
            validate_structured(&assertions)?;
            if require_collision_safe {
                return Err(crate::EvidenceError::new(
                    "assertion-summary: explicit accepted classification is required",
                ));
            }
        }
        Some((::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted, true)) => {
            validate_structured(&assertions)?
        }
        Some((
            ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
            false,
        )) => {
            for assertion in &assertions {
                validate_common(assertion)?;
                validate_strict_identity(assertion)?;
            }
            if require_collision_safe {
                return Err(crate::EvidenceError::new(
                    "assertion-summary: fatal evidence cannot be promoted",
                ));
            }
        }
        _ => {
            return Err(crate::EvidenceError::new(
                "assertion-summary: strict classification is invalid",
            ))
        }
    }
    Ok(SummaryIdentityStatus::CollisionSafe)
}

fn validate_legacy(assertions: &[ReviewAssertion]) -> crate::EvidenceResult<()> {
    let mut ids = std::collections::BTreeSet::new();
    for assertion in assertions {
        validate_common(assertion)?;
        if !ids.insert(assertion.id) {
            return Err(crate::EvidenceError::new(
                "assertion-summary: duplicate legacy assertion ID",
            ));
        }
    }
    Ok(())
}

fn validate_structured(assertions: &[ReviewAssertion]) -> crate::EvidenceResult<()> {
    let mut fingerprints = std::collections::BTreeSet::new();
    let mut catalog_token = None;
    let mut builder =
        ::chaoscontrol_protocol::admission::CatalogBuilder::begin(assertions.len())
            .map_err(|error| crate::EvidenceError::new(format!("assertion-summary: {error:?}")))?;
    for assertion in assertions {
        validate_common(assertion)?;
        let identity = validate_strict_identity(assertion)?;
        if !fingerprints.insert(identity.fingerprint) {
            return Err(crate::EvidenceError::new(
                "assertion-summary: duplicate assertion fingerprint",
            ));
        }
        let identity_token = identity.catalog_tokens[0];
        if catalog_token
            .replace(identity_token)
            .is_some_and(|token| token != identity_token)
        {
            return Err(crate::EvidenceError::new(
                "assertion-summary: inconsistent catalog tokens",
            ));
        }
        builder
            .insert_with_fingerprint(identity.descriptor.clone(), identity.fingerprint)
            .map_err(|error| {
                crate::EvidenceError::new(format!("assertion-summary catalog conflict: {error:?}"))
            })?;
    }
    builder
        .complete(catalog_token.expect("structured summary is non-empty"))
        .map_err(|error| crate::EvidenceError::new(format!("assertion-summary: {error:?}")))?;
    Ok(())
}

fn validate_strict_identity(assertion: &ReviewAssertion) -> crate::EvidenceResult<&ReviewIdentity> {
    let identity = assertion
        .identity
        .as_ref()
        .expect("caller checked identity presence");
    let computed = identity.descriptor.fingerprint().map_err(|error| {
        crate::EvidenceError::new(format!("assertion-summary: invalid descriptor: {error}"))
    })?;
    let canonical = identity.descriptor.canonical_bytes().map_err(|error| {
        crate::EvidenceError::new(format!("assertion-summary: invalid descriptor: {error}"))
    })?;
    if computed != identity.fingerprint {
        return Err(crate::EvidenceError::new(
            "assertion-summary: descriptor fingerprint mismatch",
        ));
    }
    if crate::sdk_local_identity_value::encode_hex(&canonical) != identity.canonical_descriptor {
        return Err(crate::EvidenceError::new(
            "assertion-summary: canonical descriptor mismatch",
        ));
    }
    if identity.catalog_tokens.len() != 1 {
        return Err(crate::EvidenceError::new(
            "assertion-summary: each identity requires one catalog token",
        ));
    }
    validate_metadata(assertion, &identity.descriptor)?;
    Ok(identity)
}

fn validate_common(assertion: &ReviewAssertion) -> crate::EvidenceResult<()> {
    crate::assertion_summary_semantics::validate_common(
        crate::assertion_summary_semantics::AssertionSemantics {
            message: &assertion.message,
            kind: &assertion.kind,
            guest: &assertion.guest,
            category: &assertion.category,
            verdict: &assertion.verdict,
            hit_count: assertion.hit_count,
            true_count: assertion.true_count,
            false_count: assertion.false_count,
            last_failure_details: assertion.last_failure_details.as_deref(),
        },
    )
}

fn validate_metadata(
    assertion: &ReviewAssertion,
    descriptor: &::chaoscontrol_protocol::identity::AssertionDescriptor,
) -> crate::EvidenceResult<()> {
    let descriptor_kind = match descriptor.kind {
        ::chaoscontrol_protocol::identity::AssertionKind::Always => "always",
        ::chaoscontrol_protocol::identity::AssertionKind::Sometimes => "sometimes",
        ::chaoscontrol_protocol::identity::AssertionKind::Reachable => "reachable",
        ::chaoscontrol_protocol::identity::AssertionKind::Unreachable => "unreachable",
    };
    if assertion.message != descriptor.message
        || assertion.kind != descriptor_kind
        || assertion.guest != descriptor.guest
        || assertion.category != descriptor.category
    {
        return Err(crate::EvidenceError::new(
            "assertion-summary: report metadata conflicts with descriptor",
        ));
    }
    let redundant_id_matches = descriptor
        .compatibility_id
        .map_or(assertion.id == 0, |compatibility_id| {
            compatibility_id == assertion.id
        });
    if !redundant_id_matches {
        return Err(crate::EvidenceError::new(
            "assertion-summary: compatibility ID conflicts with descriptor",
        ));
    }
    Ok(())
}
