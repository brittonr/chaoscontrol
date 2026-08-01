use chaoscontrol_protocol::admission::AssertionEvidenceIdentity;
use chaoscontrol_protocol::identity::{
    encode_lower_hex, AssertionDescriptor, AssertionFingerprint,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{AssertionSummaryEntry, EvidenceError, EvidenceResult};

pub(crate) const ACCEPTED_V2_STATUS: &str = "accepted-v2";
pub(crate) const DIAGNOSTIC_ONLY_STATUS: &str = "legacy-diagnostic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityStatus {
    AcceptedV2,
    DiagnosticOnly,
}

impl IdentityStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AcceptedV2 => ACCEPTED_V2_STATUS,
            Self::DiagnosticOnly => DIAGNOSTIC_ONLY_STATUS,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IdentityAdmission {
    pub(crate) entries: Vec<AssertionSummaryEntry>,
    pub(crate) identities: Vec<(u64, AssertionEvidenceIdentity)>,
    pub(crate) status: IdentityStatus,
    pub(crate) promotion_blocker: Option<String>,
}

#[derive(Deserialize)]
struct SummaryIdentityCarrier {
    id: u64,
    identity: SummaryIdentity,
}

#[derive(Deserialize)]
struct SummaryIdentity {
    descriptor: AssertionDescriptor,
    fingerprint: AssertionFingerprint,
    canonical_descriptor: String,
    catalog_tokens: Vec<AssertionFingerprint>,
}

impl IdentityAdmission {
    pub(crate) fn require_selected_alias(
        &self,
        workload: &str,
        selected_alias: u64,
    ) -> EvidenceResult<()> {
        if self.status != IdentityStatus::AcceptedV2 {
            let blocker = self
                .promotion_blocker
                .as_deref()
                .unwrap_or("assertion summary is diagnostic-only");
            return Err(EvidenceError::new(format!(
                "{workload}: assertions.json is diagnostic-only and cannot promote: {blocker}; fresh admitted v2 KVM evidence is required"
            )));
        }

        let matching_aliases = self
            .identities
            .iter()
            .filter(|(alias, _)| *alias == selected_alias)
            .count();
        if matching_aliases != 1 {
            return Err(EvidenceError::new(format!(
                "{workload}: selected assertion alias {selected_alias} resolves to {matching_aliases} accepted v2 entries"
            )));
        }
        Ok(())
    }

    pub(crate) fn require_evidence_identity(
        &self,
        workload: &str,
        selected_alias: u64,
        expected: &AssertionEvidenceIdentity,
    ) -> EvidenceResult<()> {
        self.require_selected_alias(workload, selected_alias)?;
        let (_, actual) = self
            .identities
            .iter()
            .find(|(alias, _)| *alias == selected_alias)
            .expect("selected alias count was checked");
        if actual != expected {
            return Err(EvidenceError::new(format!(
                "{workload}: bug assertion identity does not match accepted v2 assertions.json"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkloadIdentityStatus<'a> {
    pub(crate) workload: &'a str,
    pub(crate) artifact_status: &'a str,
    pub(crate) report_status: &'a str,
}

pub(crate) fn classify(value: &Value) -> EvidenceResult<IdentityAdmission> {
    crate::validate_assertion_summary(value)?;
    let promotion_blocker = crate::validate_assertion_summary_for_promotion(value)
        .err()
        .map(|error| error.message().to_string());
    let status = if promotion_blocker.is_some() {
        IdentityStatus::DiagnosticOnly
    } else {
        IdentityStatus::AcceptedV2
    };
    let entries_value = value
        .as_array()
        .cloned()
        .or_else(|| value.get("assertions").and_then(Value::as_array).cloned())
        .ok_or_else(|| EvidenceError::new("assertions.json has no assertion entries"))?;
    let entries = serde_json::from_value(Value::Array(entries_value.clone())).map_err(|error| {
        EvidenceError::new(format!("assertions.json entries are invalid: {error}"))
    })?;
    let identities = if status == IdentityStatus::AcceptedV2 {
        accepted_identities(&entries_value)?
    } else {
        Vec::new()
    };

    Ok(IdentityAdmission {
        entries,
        identities,
        status,
        promotion_blocker,
    })
}

fn accepted_identities(values: &[Value]) -> EvidenceResult<Vec<(u64, AssertionEvidenceIdentity)>> {
    values
        .iter()
        .map(|value| {
            let carrier: SummaryIdentityCarrier =
                serde_json::from_value(value.clone()).map_err(|error| {
                    EvidenceError::new(format!("accepted v2 identity is invalid: {error}"))
                })?;
            if carrier.identity.catalog_tokens.len() != 1 {
                return Err(EvidenceError::new(
                    "accepted v2 identity requires one catalog token",
                ));
            }
            let canonical_descriptor = carrier
                .identity
                .descriptor
                .canonical_bytes()
                .map_err(|error| EvidenceError::new(format!("invalid descriptor: {error}")))?;
            if encode_lower_hex(&canonical_descriptor) != carrier.identity.canonical_descriptor {
                return Err(EvidenceError::new(
                    "accepted v2 canonical descriptor does not match",
                ));
            }
            let identity = AssertionEvidenceIdentity {
                descriptor: carrier.identity.descriptor,
                fingerprint: carrier.identity.fingerprint,
                canonical_descriptor,
                catalog_token: carrier.identity.catalog_tokens[0],
            };
            identity
                .validate_for_catalog_admission()
                .map_err(|error| EvidenceError::new(format!("invalid v2 identity: {error:?}")))?;
            Ok((carrier.id, identity))
        })
        .collect()
}

pub(crate) fn require_report_bindings(
    statuses: &[WorkloadIdentityStatus<'_>],
) -> EvidenceResult<()> {
    for status in statuses {
        if status.report_status != status.artifact_status {
            return Err(EvidenceError::new(format!(
                "{}: report identity status {} does not match assertion artifact {}",
                status.workload, status.report_status, status.artifact_status
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_all_accepted(statuses: &[WorkloadIdentityStatus<'_>]) -> EvidenceResult<()> {
    let blockers = statuses
        .iter()
        .filter(|status| status.artifact_status != ACCEPTED_V2_STATUS)
        .map(|status| format!("{}={}", status.workload, status.artifact_status))
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        return Err(EvidenceError::new(format!(
            "assertion-readiness promotion requires fresh admitted v2 KVM evidence; diagnostic-only artifacts: {}",
            blockers.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{classify, IdentityStatus};

    const SELECTED_ALIAS: u64 = 7;
    const WORKLOAD: &str = "fixture";

    fn fixture(input: &str) -> serde_json::Value {
        serde_json::from_str(input).expect("fixture parses")
    }

    #[test]
    fn accepts_admitted_v2_identity() {
        let value = fixture(include_str!(
            "../../tests/fixtures/assertion-readiness/accepted-v2.json"
        ));
        let admission = classify(&value).expect("accepted v2 classifies");

        assert_eq!(admission.status, IdentityStatus::AcceptedV2);
        admission
            .require_selected_alias(WORKLOAD, SELECTED_ALIAS)
            .expect("accepted alias resolves");
    }

    #[test]
    fn keeps_legacy_array_diagnostic_only() {
        let value = fixture(include_str!(
            "../../tests/fixtures/assertion-readiness/legacy-array.json"
        ));
        let admission = classify(&value).expect("legacy array classifies");

        assert_eq!(admission.status, IdentityStatus::DiagnosticOnly);
        let error = admission
            .require_selected_alias(WORKLOAD, SELECTED_ALIAS)
            .expect_err("legacy array cannot promote");
        assert!(error.message().contains("fresh admitted v2 KVM evidence"));
    }

    #[test]
    fn rejects_malformed_summary() {
        let value = fixture(include_str!(
            "../../tests/fixtures/assertion-readiness/malformed.json"
        ));
        let error = classify(&value).expect_err("malformed summary is rejected");

        assert!(error.message().contains("expected non-empty array"));
    }
}
