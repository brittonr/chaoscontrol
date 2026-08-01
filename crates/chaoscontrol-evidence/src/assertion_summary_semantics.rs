use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_protocol::assertion_identity::{
    MAX_ASSERTION_CATEGORY_BYTES, MAX_ASSERTION_EVENT_DETAILS_BYTES, MAX_ASSERTION_GUEST_BYTES,
    MAX_ASSERTION_MESSAGE_BYTES,
};

pub(crate) struct AssertionSemantics<'a> {
    pub message: &'a str,
    pub kind: &'a str,
    pub guest: &'a str,
    pub category: &'a str,
    pub verdict: &'a str,
    pub hit_count: u64,
    pub true_count: u64,
    pub false_count: u64,
    pub last_failure_details: Option<&'a str>,
}

pub(crate) fn validate_common(assertion: AssertionSemantics<'_>) -> EvidenceResult<()> {
    for (field, value) in [
        ("message", assertion.message),
        ("kind", assertion.kind),
        ("guest", assertion.guest),
        ("category", assertion.category),
    ] {
        if value.is_empty() {
            return Err(EvidenceError::new(format!(
                "assertion-summary.{field}: expected non-empty string"
            )));
        }
        if value.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(EvidenceError::new(format!(
                "assertion-summary.{field}: ASCII controls are forbidden"
            )));
        }
    }
    if assertion.message.len() > MAX_ASSERTION_MESSAGE_BYTES
        || assertion.guest.len() > MAX_ASSERTION_GUEST_BYTES
        || assertion.category.len() > MAX_ASSERTION_CATEGORY_BYTES
        || !normalized_category(assertion.category)
    {
        return Err(EvidenceError::new(
            "assertion-summary: bounded metadata is invalid",
        ));
    }
    let counted = assertion
        .true_count
        .checked_add(assertion.false_count)
        .ok_or_else(|| EvidenceError::new("assertion-summary: counter overflow"))?;
    if counted != assertion.hit_count {
        return Err(EvidenceError::new(
            "assertion-summary: true and false counts do not equal hit count",
        ));
    }
    let expected = expected_verdict(&assertion)?;
    if assertion.verdict != expected {
        return Err(EvidenceError::new(
            "assertion-summary: verdict conflicts with kind and counts",
        ));
    }
    if assertion
        .last_failure_details
        .is_some_and(|details| details.len() > MAX_ASSERTION_EVENT_DETAILS_BYTES)
    {
        return Err(EvidenceError::new(
            "assertion-summary: failure details exceed the byte limit",
        ));
    }
    Ok(())
}

fn expected_verdict(assertion: &AssertionSemantics<'_>) -> EvidenceResult<&'static str> {
    match assertion.kind {
        "always" if assertion.hit_count == 0 => Ok("unexercised"),
        "always" if assertion.false_count == 0 => Ok("passed"),
        "always" => Ok("failed"),
        "sometimes" if assertion.hit_count == 0 => Ok("unexercised"),
        "sometimes" if assertion.true_count > 0 => Ok("passed"),
        "sometimes" => Ok("failed"),
        "reachable" if assertion.false_count == 0 && assertion.hit_count > 0 => Ok("passed"),
        "reachable" if assertion.false_count == 0 => Ok("unexercised"),
        "unreachable" if assertion.true_count == 0 && assertion.hit_count == 0 => Ok("passed"),
        "unreachable" if assertion.true_count == 0 => Ok("failed"),
        _ => Err(EvidenceError::new(
            "assertion-summary: kind or kind/count semantics are invalid",
        )),
    }
}

fn normalized_category(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}
