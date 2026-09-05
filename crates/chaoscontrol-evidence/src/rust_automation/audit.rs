//! Pure cargo-audit triage policy.

// r[impl chaoscontrol.rust_automation.tools]
// r[impl chaoscontrol.rust_automation.validation]

use serde_json::Value;

const ALLOWLIST_VERSION: u64 = 1;
const REQUIRED_FIELDS: [&str; 7] = [
    "category",
    "id",
    "package",
    "version",
    "disposition",
    "rationale",
    "follow_up",
];

type FindingKey = (String, String, String, String);

pub fn validate_report(report: &Value, allowlist: &Value) -> Result<String, String> {
    let vulnerabilities = report
        .pointer("/vulnerabilities/list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !vulnerabilities.is_empty() {
        let mut lines = vec![String::from(
            "dependency audit found vulnerability finding(s):",
        )];
        for item in vulnerabilities {
            let advisory = string_or_unknown(item.pointer("/advisory/id"));
            let package = string_or_unknown(item.pointer("/package/name"));
            let version = string_or_unknown(item.pointer("/package/version"));
            lines.push(format!("- {advisory} {package}@{version}"));
        }
        return Err(lines.join("\n"));
    }

    let findings = warning_findings(report);
    let allowed = validate_allowlist(allowlist)?;
    let finding_keys = findings
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let allowed_keys = allowed
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let untriaged = finding_keys
        .difference(&allowed_keys)
        .cloned()
        .collect::<Vec<_>>();
    let stale = allowed_keys
        .difference(&finding_keys)
        .cloned()
        .collect::<Vec<_>>();
    if !untriaged.is_empty() || !stale.is_empty() {
        let mut lines = Vec::new();
        if !untriaged.is_empty() {
            lines.push(String::from("untriaged cargo-audit warning(s):"));
            lines.extend(untriaged.iter().map(|key| format!("- {}", format_key(key))));
        }
        if !stale.is_empty() {
            lines.push(String::from(
                "stale cargo-audit warning allowlist entry/entries:",
            ));
            lines.extend(stale.iter().map(|key| format!("- {}", format_key(key))));
        }
        return Err(lines.join("\n"));
    }

    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for key in findings.keys() {
        *counts.entry(key.0.clone()).or_default() += 1;
    }
    let encoded = serde_json::to_string(&counts)
        .map_err(|error| format!("cargo-audit warning counts encode failed: {error}"))?;
    Ok(format!(
        "dependency audit ok: vulnerabilities=0 triaged_warnings={encoded}"
    ))
}

fn warning_findings(report: &Value) -> std::collections::BTreeMap<FindingKey, Value> {
    let mut findings = std::collections::BTreeMap::new();
    let Some(warnings) = report.get("warnings").and_then(Value::as_object) else {
        return findings;
    };
    for (category, items) in warnings {
        let Some(items) = items.as_array() else {
            continue;
        };
        for item in items {
            if item.is_object() {
                findings.insert(finding_key(category, item), item.clone());
            }
        }
    }
    findings
}

fn finding_key(category: &str, item: &Value) -> FindingKey {
    (
        category.to_string(),
        string_or_unknown(item.pointer("/advisory/id")),
        string_or_unknown(item.pointer("/package/name")),
        string_or_unknown(item.pointer("/package/version")),
    )
}

fn validate_allowlist(
    allowlist: &Value,
) -> Result<std::collections::BTreeMap<FindingKey, Value>, String> {
    if allowlist.get("version").and_then(Value::as_u64) != Some(ALLOWLIST_VERSION) {
        return Err(String::from("allowlist version must be 1"));
    }
    let entries = allowlist
        .get("warnings")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("allowlist must contain a warnings list"))?;
    let mut allowed = std::collections::BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let number = index + 1;
        let Some(object) = entry.as_object() else {
            return Err(format!("allowlist entry {number} is not an object"));
        };
        let missing = REQUIRED_FIELDS
            .iter()
            .filter(|field| {
                object
                    .get(**field)
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
            })
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "allowlist entry {number} missing required field(s): {}",
                missing.join(", ")
            ));
        }
        let key = (
            required_string(entry, "category")?,
            required_string(entry, "id")?,
            required_string(entry, "package")?,
            required_string(entry, "version")?,
        );
        if allowed.insert(key.clone(), entry.clone()).is_some() {
            return Err(format!(
                "duplicate allowlist entry for {}",
                format_key(&key)
            ));
        }
    }
    Ok(allowed)
}

fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing {field}"))
}

fn string_or_unknown(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn format_key(key: &FindingKey) -> String {
    format!("{}:{}:{}@{}", key.0, key.1, key.2, key.3)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate_report;

    fn allowed() -> serde_json::Value {
        json!({
            "version": 1,
            "warnings": [{
                "category": "unmaintained",
                "id": "RUSTSEC-TEST-0001",
                "package": "demo",
                "version": "1.0.0",
                "disposition": "accepted-test-risk",
                "rationale": "fixture",
                "follow_up": "remove fixture"
            }]
        })
    }

    fn report() -> serde_json::Value {
        json!({
            "vulnerabilities": {"list": []},
            "warnings": {"unmaintained": [{
                "advisory": {"id": "RUSTSEC-TEST-0001"},
                "package": {"name": "demo", "version": "1.0.0"}
            }]}
        })
    }

    #[test]
    fn matching_warning_is_accepted() {
        assert_eq!(
            validate_report(&report(), &allowed()).expect("valid"),
            "dependency audit ok: vulnerabilities=0 triaged_warnings={\"unmaintained\":1}"
        );
    }

    #[test]
    fn vulnerability_untriaged_and_stale_cases_fail() {
        let vulnerable = json!({
            "vulnerabilities": {"list": [{
                "advisory": {"id": "RUSTSEC-TEST-9999"},
                "package": {"name": "bad", "version": "9.9.9"}
            }]},
            "warnings": {}
        });
        assert!(
            validate_report(&vulnerable, &json!({"version": 1, "warnings": []}))
                .expect_err("vulnerability")
                .contains("vulnerability")
        );
        assert!(validate_report(
            &json!({"vulnerabilities": {"list": []}, "warnings": {}}),
            &allowed()
        )
        .expect_err("stale")
        .contains("stale"));
        let mut changed = report();
        changed["warnings"]["unsound"] = json!([{
            "advisory": {"id": "RUSTSEC-TEST-0002"},
            "package": {"name": "other", "version": "2.0.0"}
        }]);
        assert!(validate_report(&changed, &allowed())
            .expect_err("untriaged")
            .contains("untriaged"));
    }
}
