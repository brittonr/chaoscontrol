use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_protocol::admission::{AcceptedCatalog, CatalogBuilder, CatalogValidationStatus};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) use crate::sdk_local_event::ResolvedLocalIdentity;

pub const MAX_SDK_JSONL_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_SDK_JSONL_LINE_BYTES: usize = 16 * 1024;
pub const MAX_SDK_JSONL_EVENTS: usize = 65_536;

#[derive(Debug, Clone)]
pub(crate) struct LocalIdentityValidation {
    pub events: BTreeMap<usize, ResolvedLocalIdentity>,
    pub catalog: BTreeMap<String, ResolvedLocalIdentity>,
    pub catalog_status: CatalogValidationStatus,
    pub legacy_ambiguous: bool,
}

pub(crate) fn validate_local_identity_stream(
    content: &str,
) -> EvidenceResult<LocalIdentityValidation> {
    if content.len() as u64 > MAX_SDK_JSONL_BYTES {
        return Err(EvidenceError::new("SDK JSONL exceeds the input byte limit"));
    }
    let mut builder: Option<CatalogBuilder> = None;
    let mut accepted: Option<AcceptedCatalog> = None;
    let mut event_state = crate::sdk_local_event::LocalEventState::default();
    let mut event_count = 0_usize;
    for (line_index, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_SDK_JSONL_LINE_BYTES {
            return line_error(line_index, "line exceeds the byte limit");
        }
        crate::json_preflight::preflight_json(line, crate::json_preflight::JSONL_LINE_LIMITS)
            .map_err(|error| {
                EvidenceError::new(format!("line {}: {}", line_index + 1, error.message()))
            })?;
        event_count = event_count
            .checked_add(1)
            .ok_or_else(|| EvidenceError::new("SDK JSONL event count overflow"))?;
        if event_count > MAX_SDK_JSONL_EVENTS {
            return line_error(line_index, "event count exceeds the limit");
        }
        let value: Value = serde_json::from_str(line).map_err(|error| {
            EvidenceError::new(format!("invalid JSONL at line {}: {error}", line_index + 1))
        })?;
        let Some(object) = value.as_object() else {
            return line_error(line_index, "record must be an object");
        };
        if object.contains_key("chaoscontrol_assertion_catalog") {
            crate::sdk_local_catalog::apply_catalog_line(
                line,
                line_index,
                &mut builder,
                &mut accepted,
            )?;
            continue;
        }
        if object.contains_key("chaoscontrol_assertion_quarantine") {
            event_state.mark_quarantine(line_index);
            continue;
        }
        if object.contains_key("antithesis_assert") {
            event_state.apply_line(line, line_index, accepted.as_ref())?;
        }
    }
    if builder.is_some() {
        return Err(EvidenceError::new(
            "assertion catalog is missing completion",
        ));
    }
    let legacy_ambiguous = event_state.legacy_ambiguous();
    let catalog_status = accepted.as_ref().map_or_else(
        || {
            if legacy_ambiguous {
                CatalogValidationStatus::LegacyAmbiguous
            } else {
                CatalogValidationStatus::Pending
            }
        },
        |_| CatalogValidationStatus::Accepted,
    );
    let catalog = accepted
        .as_ref()
        .map(catalog_identities)
        .unwrap_or_default();
    Ok(LocalIdentityValidation {
        events: event_state.events,
        catalog,
        catalog_status,
        legacy_ambiguous,
    })
}

fn catalog_identities(catalog: &AcceptedCatalog) -> BTreeMap<String, ResolvedLocalIdentity> {
    catalog
        .assertions
        .values()
        .map(|assertion| {
            let identity = ResolvedLocalIdentity {
                key: assertion.fingerprint.to_hex(),
                descriptor: assertion.descriptor.clone(),
                fingerprint: assertion.fingerprint,
                catalog_token: catalog.token,
            };
            (identity.key.clone(), identity)
        })
        .collect()
}

fn line_error<T>(line_index: usize, message: &str) -> EvidenceResult<T> {
    Err(EvidenceError::new(format!(
        "line {}: {message}",
        line_index + 1
    )))
}
