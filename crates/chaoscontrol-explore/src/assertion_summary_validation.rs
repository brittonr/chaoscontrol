pub(crate) fn validate_assertion_details(
    assertions: &[crate::explorer::AssertionDetail],
) -> Result<::chaoscontrol_protocol::admission::CatalogValidationStatus, String> {
    if assertions.is_empty() {
        return Ok(::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous);
    }
    if assertions.len() > ::chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES {
        return Err("assertion summary exceeds descriptor cardinality".to_string());
    }
    let identified = assertions
        .iter()
        .filter(|detail| detail.identity.is_some())
        .count();
    if identified == 0 {
        validate_legacy(assertions)?;
        return Ok(::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous);
    }
    if identified != assertions.len() {
        return Err("mixed legacy and structured assertion details".to_string());
    }
    validate_strict(assertions)?;
    Ok(::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted)
}

fn validate_legacy(assertions: &[crate::explorer::AssertionDetail]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for detail in assertions {
        validate_common(detail)?;
        if !ids.insert(detail.id) {
            return Err("duplicate legacy assertion ID".to_string());
        }
    }
    Ok(())
}

pub(crate) fn validate_fatal_details(
    assertions: &[crate::explorer::AssertionDetail],
) -> Result<(), String> {
    if assertions.is_empty()
        || assertions.len() > ::chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES
    {
        return Err("fatal assertion summary cardinality is invalid".to_string());
    }
    for detail in assertions {
        validate_common(detail)?;
        if detail.identity.is_some() {
            validate_strict_identity(detail)?;
        }
    }
    Ok(())
}

fn validate_strict(assertions: &[crate::explorer::AssertionDetail]) -> Result<(), String> {
    let mut builder = ::chaoscontrol_protocol::admission::CatalogBuilder::begin(assertions.len())
        .map_err(|error| format!("invalid assertion catalog: {error:?}"))?;
    let mut fingerprints = std::collections::BTreeSet::new();
    let mut catalog_token: Option<::chaoscontrol_protocol::identity::AssertionFingerprint> = None;
    for detail in assertions {
        validate_common(detail)?;
        let identity = validate_strict_identity(detail)?;
        let fingerprint = identity.fingerprint;
        if !fingerprints.insert(fingerprint) {
            return Err("duplicate assertion fingerprint".to_string());
        }
        let token = identity.catalog_tokens[0];
        if catalog_token
            .replace(token)
            .is_some_and(|value| value != token)
        {
            return Err("assertion catalog tokens disagree".to_string());
        }
        builder
            .insert_with_fingerprint(identity.descriptor.clone(), fingerprint)
            .map_err(|error| format!("assertion catalog conflict: {error:?}"))?;
    }
    builder
        .complete(catalog_token.expect("strict catalog is non-empty"))
        .map_err(|error| format!("assertion catalog completion failed: {error:?}"))?;
    Ok(())
}

fn validate_strict_identity(
    detail: &crate::explorer::AssertionDetail,
) -> Result<&crate::explorer::AssertionIdentityDetail, String> {
    let identity = detail
        .identity
        .as_ref()
        .expect("caller checked identity presence");
    let canonical = identity
        .descriptor
        .canonical_bytes()
        .map_err(|error| format!("invalid descriptor: {error}"))?;
    let fingerprint = identity
        .descriptor
        .fingerprint()
        .map_err(|error| format!("invalid descriptor fingerprint: {error}"))?;
    let expected_id = identity.descriptor.compatibility_id.unwrap_or_default();
    if identity.fingerprint != fingerprint
        || identity.canonical_descriptor
            != chaoscontrol_protocol::identity::encode_lower_hex(&canonical)
        || identity.catalog_tokens.len() != 1
        || detail.id != expected_id
        || detail.message != identity.descriptor.message
        || detail.kind != exact_kind(identity.descriptor.kind)
        || detail.guest != identity.descriptor.guest
        || detail.category != identity.descriptor.category
    {
        return Err("assertion identity fields disagree".to_string());
    }
    Ok(identity)
}

fn validate_common(detail: &crate::explorer::AssertionDetail) -> Result<(), String> {
    if detail.message.is_empty()
        || detail.message.bytes().any(|byte| byte.is_ascii_control())
        || detail.guest.bytes().any(|byte| byte.is_ascii_control())
        || detail.category.bytes().any(|byte| byte.is_ascii_control())
        || detail.message.len() > ::chaoscontrol_protocol::identity::MAX_ASSERTION_MESSAGE_BYTES
        || detail.guest.is_empty()
        || detail.guest.len() > ::chaoscontrol_protocol::identity::MAX_ASSERTION_GUEST_BYTES
        || detail.category.is_empty()
        || detail.category.len() > ::chaoscontrol_protocol::identity::MAX_ASSERTION_CATEGORY_BYTES
        || !normalized_category(&detail.category)
        || detail.last_failure_details.as_ref().is_some_and(|value| {
            value.len() > ::chaoscontrol_protocol::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES
        })
    {
        return Err("assertion detail metadata exceeds bounds".to_string());
    }
    let expected = derive_detail_verdict(detail)?;
    if detail.verdict != expected {
        return Err("assertion detail verdict disagrees with counters".to_string());
    }
    Ok(())
}

fn normalized_category(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

pub(crate) fn derive_detail_verdict(
    detail: &crate::explorer::AssertionDetail,
) -> Result<&'static str, String> {
    let hits = detail
        .true_count
        .checked_add(detail.false_count)
        .ok_or_else(|| "assertion counter overflow".to_string())?;
    if hits != detail.hit_count {
        return Err("assertion counters disagree".to_string());
    }
    match detail.kind.as_str() {
        "always" if detail.false_count > 0 => Ok("failed"),
        "always" if hits > 0 => Ok("passed"),
        "always" => Ok("unexercised"),
        "sometimes" if detail.true_count > 0 => Ok("passed"),
        "sometimes" if hits > 0 => Ok("failed"),
        "sometimes" => Ok("unexercised"),
        "reachable" if detail.false_count > 0 => {
            Err("reachable false count is invalid".to_string())
        }
        "reachable" if detail.true_count > 0 => Ok("passed"),
        "reachable" => Ok("unexercised"),
        "unreachable" if detail.true_count > 0 => {
            Err("unreachable true count is invalid".to_string())
        }
        "unreachable" if detail.false_count > 0 => Ok("failed"),
        "unreachable" => Ok("passed"),
        _ => Err("unknown assertion kind".to_string()),
    }
}

fn exact_kind(kind: ::chaoscontrol_protocol::identity::AssertionKind) -> &'static str {
    match kind {
        ::chaoscontrol_protocol::identity::AssertionKind::Always => "always",
        ::chaoscontrol_protocol::identity::AssertionKind::Sometimes => "sometimes",
        ::chaoscontrol_protocol::identity::AssertionKind::Reachable => "reachable",
        ::chaoscontrol_protocol::identity::AssertionKind::Unreachable => "unreachable",
    }
}
