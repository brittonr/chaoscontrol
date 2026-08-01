use chaoscontrol_protocol::admission::AssertionEvidenceIdentity;
use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

pub(crate) fn require_only_fields(
    carrier: &Value,
    allowed: &[&str],
    context: &str,
) -> EvidenceResult<()> {
    let object = carrier
        .as_object()
        .ok_or_else(|| EvidenceError::new(format!("{context}: expected object")))?;
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(EvidenceError::new(format!(
            "{context}: unknown field {field:?}"
        )));
    }
    Ok(())
}

pub(crate) fn optional_identity(
    carrier: &Value,
    field: &str,
    assertion_id: u64,
    required: bool,
    context: &str,
) -> EvidenceResult<Option<AssertionEvidenceIdentity>> {
    let value = match carrier.get(field) {
        Some(value) if value.is_null() => {
            return Err(EvidenceError::new(format!(
                "{context}.{field}: explicit null is invalid"
            )));
        }
        Some(value) => value,
        None if required => {
            return Err(EvidenceError::new(format!(
                "{context}: legacy assertion ID-only evidence cannot promote"
            )));
        }
        None => return Ok(None),
    };
    let identity: AssertionEvidenceIdentity =
        serde_json::from_value(value.clone()).map_err(|error| {
            EvidenceError::new(format!("{context}.{field}: invalid identity: {error}"))
        })?;
    identity
        .validate_compatibility_alias(assertion_id)
        .map_err(|error| {
            EvidenceError::new(format!(
                "{context}.{field}: identity is not strict catalog evidence: {error:?}"
            ))
        })?;
    Ok(Some(identity))
}
