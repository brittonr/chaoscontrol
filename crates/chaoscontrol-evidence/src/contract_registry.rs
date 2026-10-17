use std::collections::BTreeSet;

use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

pub const ALLOWED_OWNERSHIP: [&str; 3] = ["excluded", "nickel-authored", "rust-derived"];
pub const REQUIRED_CONTRACT_IDS: [&str; 18] = [
    "assertion-summary",
    "bug-report",
    "campaign-progress",
    "campaign-reports",
    "campaign-profile",
    "checkpoint-reference",
    "dogfood-receipt",
    "execution-traces",
    "fault-outcomes",
    "finite-fault-schedule-profile",
    "raw-runtime-logs",
    "portable-snapshot-descriptor",
    "replay-verdict",
    "run-config",
    "secrets-and-crypto-internals",
    "simulator-profile",
    "simulator-runtime-records",
    "snapshot-reference",
];

const AUTHORED_PROFILES: [&str; 4] = [
    "run-config",
    "simulator-profile",
    "campaign-profile",
    "finite-fault-schedule-profile",
];
const RUNTIME_FAMILIES: [&str; 12] = [
    "bug-report",
    "assertion-summary",
    "checkpoint-reference",
    "snapshot-reference",
    "portable-snapshot-descriptor",
    "replay-verdict",
    "campaign-progress",
    "simulator-runtime-records",
    "execution-traces",
    "fault-outcomes",
    "campaign-reports",
    "dogfood-receipt",
];
const SHARED_PRIMITIVES: &str = "contracts/evidence/profile-primitives.ncl";

pub fn validate_contract_registry_json(text: &str) -> EvidenceResult<String> {
    let registry: Value = serde_json::from_str(text)?;
    validate_contract_registry(&registry)
}

pub fn validate_contract_registry(registry: &Value) -> EvidenceResult<String> {
    let mut errors = Vec::new();

    require(
        registry.get("schema_version").and_then(Value::as_str) == Some("1"),
        "schema_version must be '1'",
        &mut errors,
    );
    require(
        registry
            .get("policy")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "policy must be non-empty",
        &mut errors,
    );

    let families_value = registry.get("families");
    let families = families_value.and_then(Value::as_array);
    require(
        families.is_some_and(|entries| !entries.is_empty()),
        "families must be a non-empty list",
        &mut errors,
    );
    let empty = Vec::new();
    let families = families.unwrap_or(&empty);

    let allowed = ALLOWED_OWNERSHIP.into_iter().collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    let mut ownerships = BTreeSet::new();

    for (index, entry) in families.iter().enumerate() {
        let prefix = format!("families[{index}]");
        let Some(entry) = entry.as_object() else {
            errors.push(format!("{prefix} must be an object"));
            continue;
        };

        let entry_id = entry.get("id").and_then(Value::as_str);
        let ownership = entry.get("ownership").and_then(Value::as_str);
        if let Some(entry_id) = entry_id {
            require(
                ids.insert(entry_id.to_string()),
                format!("{prefix}.id is duplicated: {entry_id}"),
                &mut errors,
            );
            if AUTHORED_PROFILES.contains(&entry_id) {
                require(
                    ownership == Some("nickel-authored"),
                    format!("{prefix}: profile intent must be nickel-authored"),
                    &mut errors,
                );
                require(
                    entry
                        .get("source_paths")
                        .and_then(Value::as_array)
                        .is_some_and(|paths| {
                            paths
                                .iter()
                                .any(|path| path.as_str() == Some(SHARED_PRIMITIVES))
                        }),
                    format!("{prefix}: profile must declare the shared primitive contract"),
                    &mut errors,
                );
            }
            if RUNTIME_FAMILIES.contains(&entry_id) {
                require(
                    ownership == Some("rust-derived") || ownership == Some("excluded"),
                    format!("{prefix}: runtime observations must remain Rust-derived or excluded"),
                    &mut errors,
                );
            }
        }
        if let Some(ownership) = ownership {
            ownerships.insert(ownership.to_string());
        }

        require(
            entry_id.is_some_and(|value| !value.is_empty()),
            format!("{prefix}.id must be non-empty"),
            &mut errors,
        );
        require(
            ownership.is_some_and(|value| allowed.contains(value)),
            format!(
                "{prefix}.ownership must be one of {}",
                py_list(ALLOWED_OWNERSHIP)
            ),
            &mut errors,
        );
        require(
            entry
                .get("owner")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            format!("{prefix}.owner must be non-empty"),
            &mut errors,
        );
        require(
            non_empty_strings(entry.get("source_paths")),
            format!("{prefix}.source_paths must be non-empty strings"),
            &mut errors,
        );
        require(
            entry
                .get("source_paths")
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .enumerate()
                        .all(|(index, path)| !values[..index].contains(path))
                }),
            format!("{prefix}.source_paths must not contain duplicates"),
            &mut errors,
        );
        require(
            entry.get("artifact_paths").is_some_and(Value::is_array),
            format!("{prefix}.artifact_paths must be a list"),
            &mut errors,
        );
        require(
            non_empty_strings(entry.get("validation_commands")),
            format!("{prefix}.validation_commands must be non-empty strings"),
            &mut errors,
        );
        require(
            non_empty_strings(entry.get("fixture_coverage")),
            format!("{prefix}.fixture_coverage must be non-empty strings"),
            &mut errors,
        );
        require(
            entry
                .get("freshness")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            format!("{prefix}.freshness must be non-empty"),
            &mut errors,
        );
        require(
            entry
                .get("rationale")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            format!("{prefix}.rationale must be non-empty"),
            &mut errors,
        );

        if ownership == Some("excluded") {
            require(
                entry
                    .get("artifact_paths")
                    .and_then(Value::as_array)
                    .is_none_or(|values| values.is_empty()),
                format!("{prefix} is excluded and must not declare durable artifact_paths"),
                &mut errors,
            );
        }
    }

    let required = REQUIRED_CONTRACT_IDS
        .into_iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    let missing = required.difference(&ids).cloned().collect::<Vec<_>>();
    require(
        missing.is_empty(),
        format!("missing required family ids: {}", py_list(missing)),
        &mut errors,
    );
    let missing_ownerships = allowed
        .iter()
        .filter(|ownership| !ownerships.contains(**ownership))
        .count();
    require(
        missing_ownerships == 0,
        format!(
            "registry must include all ownership classes; saw {}",
            py_list(ownerships.iter())
        ),
        &mut errors,
    );

    if errors.is_empty() {
        Ok(format!(
            "contract registry ok: {} families, ownership={}",
            families.len(),
            ownerships.iter().cloned().collect::<Vec<_>>().join(",")
        ))
    } else {
        Err(EvidenceError::new(errors.join("\n")))
    }
}

fn require(condition: bool, message: impl Into<String>, errors: &mut Vec<String>) {
    if !condition {
        errors.push(message.into());
    }
}

fn non_empty_strings(value: Option<&Value>) -> bool {
    let Some(values) = value.and_then(Value::as_array) else {
        return false;
    };
    values
        .iter()
        .all(|item| item.as_str().is_some_and(|text| !text.is_empty()))
}

fn py_list(values: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    format!(
        "[{}]",
        values
            .into_iter()
            .map(|value| format!("'{}'", value.as_ref()))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
