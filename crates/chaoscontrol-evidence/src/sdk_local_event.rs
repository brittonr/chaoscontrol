use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_protocol::assertion_catalog::{AcceptedCatalog, BoundAssertionEvent};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, ASSERTION_IDENTITY_VERSION,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const ASSERTION_IDENTITY_FIELD_COUNT: usize = 4;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedLocalIdentity {
    pub key: String,
    pub descriptor: AssertionDescriptor,
    pub fingerprint: AssertionFingerprint,
    pub catalog_token: AssertionFingerprint,
}

#[derive(Debug, Default)]
pub(crate) struct LocalEventState {
    pub events: BTreeMap<usize, ResolvedLocalIdentity>,
    legacy: BTreeMap<String, LegacyMetadata>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssertionEnvelope {
    antithesis_assert: AssertionBody,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssertionBody {
    assert_type: String,
    condition: bool,
    hit: bool,
    #[serde(default)]
    must_hit: Option<bool>,
    id: String,
    message: String,
    #[serde(default)]
    display_type: Option<String>,
    details: Value,
    #[serde(default)]
    identity_version: Option<u8>,
    #[serde(default)]
    catalog_token: Option<AssertionFingerprint>,
    #[serde(default)]
    assertion_fingerprint: Option<AssertionFingerprint>,
    #[serde(default)]
    catalog_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyMetadata {
    message: String,
    assert_type: String,
    guest: String,
    category: String,
}

impl LocalEventState {
    pub fn apply_line(
        &mut self,
        line: &str,
        line_index: usize,
        catalog: Option<&AcceptedCatalog>,
    ) -> EvidenceResult<()> {
        let assertion: AssertionEnvelope = serde_json::from_str(line)
            .map_err(|error| EvidenceError::new(format!("line {}: {error}", line_index + 1)))?;
        let assertion = assertion.antithesis_assert;
        let identity_count = [
            assertion.identity_version.is_some(),
            assertion.catalog_token.is_some(),
            assertion.assertion_fingerprint.is_some(),
            assertion.catalog_status.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if identity_count == 0 {
            return self.insert_legacy(assertion, line_index);
        }
        if identity_count != ASSERTION_IDENTITY_FIELD_COUNT {
            return line_error(line_index, "assertion identity fields are incomplete");
        }
        let catalog = catalog.ok_or_else(|| {
            EvidenceError::new(format!(
                "line {}: event before catalog completion",
                line_index + 1
            ))
        })?;
        let resolved = resolve_assertion(catalog, &assertion, line_index)?;
        self.events.insert(line_index, resolved);
        Ok(())
    }

    pub fn mark_quarantine(&mut self, line_index: usize) {
        self.legacy.insert(
            format!("quarantine:{line_index}"),
            LegacyMetadata {
                message: "quarantined legacy assertion".to_string(),
                assert_type: "unknown".to_string(),
                guest: "uncategorized".to_string(),
                category: "uncategorized".to_string(),
            },
        );
    }

    pub fn legacy_ambiguous(&self) -> bool {
        !self.legacy.is_empty()
    }

    fn insert_legacy(&mut self, assertion: AssertionBody, line_index: usize) -> EvidenceResult<()> {
        if assertion.id.is_empty() {
            return line_error(line_index, "legacy assertion ID is empty");
        }
        let details = assertion.details.as_object();
        let metadata = LegacyMetadata {
            message: assertion.message,
            assert_type: assertion.assert_type,
            guest: detail_string(details, "guest", "uncategorized"),
            category: detail_string(details, "category", "uncategorized"),
        };
        if let Some(existing) = self.legacy.get(&assertion.id) {
            if existing != &metadata {
                return line_error(line_index, "legacy assertion metadata conflict");
            }
            return Ok(());
        }
        self.legacy.insert(assertion.id, metadata);
        Ok(())
    }
}

fn resolve_assertion(
    catalog: &AcceptedCatalog,
    assertion: &AssertionBody,
    line_index: usize,
) -> EvidenceResult<ResolvedLocalIdentity> {
    if assertion.identity_version != Some(ASSERTION_IDENTITY_VERSION)
        || assertion.catalog_status.as_deref() != Some("accepted")
    {
        return line_error(
            line_index,
            "assertion identity version or status is invalid",
        );
    }
    let kind = assertion_kind(&assertion.assert_type, assertion.condition)?;
    let event = BoundAssertionEvent {
        catalog_token: assertion
            .catalog_token
            .expect("identity fields were counted"),
        fingerprint: assertion
            .assertion_fingerprint
            .expect("identity fields were counted"),
        kind,
    };
    let admitted = catalog
        .resolve_event(&event)
        .map_err(|error| EvidenceError::new(format!("line {}: {error:?}", line_index + 1)))?;
    if assertion.message != admitted.descriptor.message {
        return line_error(
            line_index,
            "event message conflicts with the catalog descriptor",
        );
    }
    if !assertion.hit
        || assertion
            .display_type
            .as_deref()
            .is_some_and(|value| value != assertion.assert_type)
    {
        return line_error(line_index, "assertion event shape is invalid");
    }
    let expected_must_hit = matches!(kind, AssertionKind::Sometimes | AssertionKind::Reachable);
    if assertion
        .must_hit
        .is_some_and(|must_hit| must_hit != expected_must_hit)
    {
        return line_error(line_index, "assertion must_hit conflicts with its kind");
    }
    if let Some(compatibility_id) = admitted.descriptor.compatibility_id {
        if assertion.id != format!("{compatibility_id:08x}") {
            return line_error(line_index, "compatibility ID conflicts with the descriptor");
        }
    }
    validate_event_metadata(assertion, &admitted.descriptor, line_index)?;
    Ok(ResolvedLocalIdentity {
        key: admitted.fingerprint.to_hex(),
        descriptor: admitted.descriptor.clone(),
        fingerprint: admitted.fingerprint,
        catalog_token: catalog.token,
    })
}

fn assertion_kind(assert_type: &str, condition: bool) -> EvidenceResult<AssertionKind> {
    match assert_type {
        "always" => Ok(AssertionKind::Always),
        "sometimes" => Ok(AssertionKind::Sometimes),
        "reachability" if condition => Ok(AssertionKind::Reachable),
        "reachability" => Ok(AssertionKind::Unreachable),
        _ => Err(EvidenceError::new("assertion type is unknown")),
    }
}

fn validate_event_metadata(
    assertion: &AssertionBody,
    descriptor: &AssertionDescriptor,
    line_index: usize,
) -> EvidenceResult<()> {
    let Some(details) = assertion.details.as_object() else {
        return line_error(line_index, "assertion details must be an object");
    };
    for (field, expected) in [
        ("guest", descriptor.guest.as_str()),
        ("category", descriptor.category.as_str()),
    ] {
        if let Some(actual) = details.get(field).and_then(Value::as_str) {
            if actual != expected {
                return line_error(line_index, &format!("event {field} conflicts with catalog"));
            }
        }
    }
    Ok(())
}

fn detail_string(details: Option<&Map<String, Value>>, field: &str, default: &str) -> String {
    details
        .and_then(|value| value.get(field))
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn line_error<T>(line_index: usize, message: &str) -> EvidenceResult<T> {
    Err(EvidenceError::new(format!(
        "line {}: {message}",
        line_index + 1
    )))
}
