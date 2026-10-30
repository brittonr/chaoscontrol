//! Pure transport dispatch and provider proof contract; no filesystem effects.
use super::*;
use crate::consistency_checker::{
    self, ConsistencyChecker, OperationHistory, SingleRegisterChecker,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "evidence_class",
    content = "evidence",
    rename_all = "snake_case"
)]
pub enum ImportedHistory {
    CompletionOrder(OperationHistory),
    Linearizability(SemanticHistory),
}

fn schema_error(error: impl fmt::Display) -> SemanticError {
    SemanticError::new(SemanticErrorClass::Schema, error.to_string())
}

// r[impl chaoscontrol.semantic_history.provider_contract]
/// Dispatch only exact supported versions. Never reinterpret v1 as v2.
pub fn import_json(bytes: &[u8]) -> SemanticResult<ImportedHistory> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(schema_error)?;
    match value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(1) => {
            let history: OperationHistory = serde_json::from_value(value).map_err(schema_error)?;
            consistency_checker::validate_history(&history).map_err(schema_error)?;
            Ok(ImportedHistory::CompletionOrder(history))
        }
        Some(2) => {
            let history: SemanticHistory = serde_json::from_value(value).map_err(schema_error)?;
            admit_history(&history)?;
            Ok(ImportedHistory::Linearizability(history))
        }
        _ => Err(schema_error(
            "unsupported or missing history schema_version",
        )),
    }
}

/// Return original-schema transport, not a wrapper or lossy migration.
pub fn export_json(history: &ImportedHistory) -> SemanticResult<Vec<u8>> {
    match history {
        ImportedHistory::CompletionOrder(history) => serde_json::to_vec_pretty(history),
        ImportedHistory::Linearizability(history) => serde_json::to_vec_pretty(history),
    }
    .map_err(schema_error)
}

pub fn check_json(history: &ImportedHistory) -> SemanticResult<Vec<u8>> {
    match history {
        ImportedHistory::CompletionOrder(history) => {
            let report = SingleRegisterChecker.check(history).map_err(schema_error)?;
            serde_json::to_vec_pretty(&report).map_err(schema_error)
        }
        ImportedHistory::Linearizability(history) => {
            serde_json::to_vec_pretty(&check_linearizability(history)?).map_err(schema_error)
        }
    }
}

pub fn validate_json(history: &ImportedHistory, bytes: &[u8]) -> SemanticResult<()> {
    match history {
        ImportedHistory::CompletionOrder(history) => {
            let report = serde_json::from_slice(bytes).map_err(schema_error)?;
            consistency_checker::validate_report_for_history(&report, history)
                .map_err(schema_error)?;
            require(
                report == SingleRegisterChecker.check(history).map_err(schema_error)?,
                SemanticErrorClass::Outcome,
                "legacy report does not match completion-order checker readback",
            )
        }
        ImportedHistory::Linearizability(history) => {
            let report = serde_json::from_slice(bytes).map_err(schema_error)?;
            validate_semantic_report(&report, history)
        }
    }
}

// r[impl chaoscontrol.semantic_history.provider_contract]
pub(super) fn validate_proof(
    report: &SemanticCheckReport,
    expected: &SemanticCheckReport,
) -> SemanticResult<()> {
    require(
        report.verdict == expected.verdict
            && report.witness == expected.witness
            && report.invalid == expected.invalid
            && report.unknown == expected.unknown
            && report.reduction == expected.reduction,
        SemanticErrorClass::Outcome,
        "report proof does not match deterministic bounded checker readback",
    )
}

pub(super) fn validate_reference_completion(
    events: &[ReferenceEvent],
    completion: &ReferenceEvent,
) -> SemanticResult<()> {
    let invocation = events.iter().find(|event| {
        event.attempt_id == completion.attempt_id && event.event_type == ReferenceEventType::Invoke
    });
    require(
        invocation.is_some_and(|event| {
            event.index < completion.index
                && event.process == completion.process
                && event.key == completion.key
                && event.function == completion.function
        }),
        SemanticErrorClass::Pairing,
        "reference completion metadata does not match its earlier invocation",
    )
}

/// Synthetic checker inputs only; these are not guest observations or receipts.
pub fn fixtures() -> Vec<(&'static str, SemanticHistory)> {
    let valid = valid_concurrent_fixture();
    let mut unsupported = valid.clone();
    unsupported.profile.model = "unsupported-contract-model".to_string();
    let mut pending = valid.clone();
    pending.events.pop();
    pending.completeness.completions -= 1;
    pending.completeness.pending += 1;
    let mut bounded = valid.clone();
    bounded.profile.bounds.max_operations = 1;
    vec![
        ("valid", valid),
        ("invalid", invalid_stale_read_fixture()),
        ("unknown-model", unsupported),
        ("unknown-pending", pending),
        ("unknown-bound", bounded),
    ]
}

/// Mutation corpus invoked by the provider CLI, without guest or replay claims.
pub fn selftest() -> SemanticResult<()> {
    semantic_history_selftest()?;
    legacy_selftest()?;
    for (_, history) in fixtures() {
        let imported = ImportedHistory::Linearizability(history.clone());
        let transport = export_json(&imported)?;
        let readback = import_json(&transport)?;
        require(
            imported == readback,
            SemanticErrorClass::Identity,
            "import readback changed",
        )?;
        validate_json(&readback, &check_json(&imported)?)?;
        let report = check_linearizability(&history)?;
        let mut changed = history.clone();
        changed.source_artifact.push_str("-mutated");
        require(
            validate_semantic_report(&report, &changed).is_err(),
            SemanticErrorClass::Identity,
            "mutated source was accepted",
        )?;
        let mut forged = report.clone();
        forged.verdict = LinearizabilityVerdict::Valid;
        forged.invalid = None;
        forged.unknown = None;
        let mut proof_mutation = report.clone();
        if let Some(step) = proof_mutation.witness.first_mut() {
            step.state_before.push_str("-forged");
        }
        if let Some(invalid) = &mut proof_mutation.invalid {
            invalid.model_state.push_str("-forged");
        }
        if let Some(unknown) = &mut proof_mutation.unknown {
            unknown.message.push_str("-forged");
        }
        require(
            validate_semantic_report(&proof_mutation, &history).is_err(),
            SemanticErrorClass::Outcome,
            "mutated retained proof was accepted",
        )?;
        forged.witness.clear();
        require(
            validate_semantic_report(&forged, &history).is_err(),
            SemanticErrorClass::Outcome,
            "fabricated valid proof was accepted",
        )?;
    }
    Ok(())
}

fn legacy_selftest() -> SemanticResult<()> {
    let history = consistency_checker::sample_good_history();
    let digest = consistency_checker::history_digest(&history).map_err(schema_error)?;
    let imported = ImportedHistory::CompletionOrder(history);
    let readback = import_json(&export_json(&imported)?)?;
    require(
        imported == readback,
        SemanticErrorClass::Identity,
        "v1 readback changed",
    )?;
    let ImportedHistory::CompletionOrder(history) = readback else {
        return Err(schema_error("v1 was promoted to v2"));
    };
    require(
        consistency_checker::history_digest(&history).map_err(schema_error)? == digest,
        SemanticErrorClass::Identity,
        "v1 digest semantics changed",
    )?;
    require(
        import_json(br#"{"schema_version":3}"#).is_err()
            && import_json(br#"{"schema_version":1}"#).is_err(),
        SemanticErrorClass::Schema,
        "unsupported or malformed history was accepted",
    )?;
    let mut bad = valid_concurrent_fixture();
    bad.completeness.completions += 1;
    require(
        admit_history(&bad).is_err(),
        SemanticErrorClass::Completeness,
        "incorrect completeness was accepted",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify chaoscontrol.semantic_history.provider_contract]
    #[test]
    fn source_mutations_and_fabricated_proofs_fail() {
        selftest().unwrap();
    }

    #[test]
    fn unsupported_versions_and_malformed_histories_fail() {
        assert!(import_json(br#"{"schema_version":3}"#).is_err());
        assert!(import_json(br#"{"schema_version":1}"#).is_err());
        let mut history = valid_concurrent_fixture();
        history.completeness.completions += 1;
        assert!(admit_history(&history).is_err());
    }

    #[test]
    fn completion_metadata_cannot_be_silently_discarded() {
        let history = valid_concurrent_fixture();
        let mut reference = export_jepsen_compatible(&history).unwrap();
        reference.events[2].key = "different-register".to_string();
        assert!(import_jepsen_compatible(
            &reference,
            "contract",
            "contract-source",
            history.profile
        )
        .is_err());
    }
}
