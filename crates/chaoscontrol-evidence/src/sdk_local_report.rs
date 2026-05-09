use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};

use crate::{EvidenceError, EvidenceResult};

pub const DEFAULT_SDK_LOCAL_EVIDENCE_CLASS: &str = "instrumentation-dry-run";

#[derive(Debug, Clone)]
struct AssertionSite {
    id: String,
    message: String,
    assert_type: String,
    category: String,
    observed: bool,
    observed_hits: u64,
    success_count: u64,
    failure_count: u64,
    adoption_tracks: Vec<String>,
}

pub fn summarize_sdk_local_report(
    input_path: impl AsRef<Path>,
    evidence_class: &str,
) -> EvidenceResult<Value> {
    let input_path = input_path.as_ref();
    let text = std::fs::read_to_string(input_path)
        .map_err(|err| EvidenceError::new(format!("{}: {err}", input_path.display())))?;
    summarize_sdk_local_jsonl(&text, evidence_class, Some(input_path))
}

pub fn summarize_sdk_local_jsonl(
    content: &str,
    evidence_class: &str,
    input_path: Option<&Path>,
) -> EvidenceResult<Value> {
    let mut lifecycle: BTreeMap<String, u64> = BTreeMap::new();
    let mut catalog: BTreeMap<String, AssertionSite> = BTreeMap::new();
    let mut exercised: BTreeSet<String> = BTreeSet::new();
    let mut sometimes_success: BTreeSet<String> = BTreeSet::new();
    let mut reachable_hit: BTreeSet<String> = BTreeSet::new();
    let mut failed = 0u64;
    let mut random_choice_calls = 0u64;
    let mut setup_complete = false;
    let mut adoption_tracks: BTreeMap<String, u64> = BTreeMap::new();

    for (line_idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|err| {
            let location = input_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<jsonl>".to_string());
            EvidenceError::new(format!(
                "invalid JSONL at {location}:{}: {err}",
                line_idx + 1
            ))
        })?;
        let Some(object) = value.as_object() else {
            continue;
        };

        if let Some(assertion) = object.get("antithesis_assert") {
            let assertion = assertion.as_object().ok_or_else(|| {
                EvidenceError::new(format!(
                    "antithesis_assert at line {} must be an object",
                    line_idx + 1
                ))
            })?;
            let assertion_id = value_to_string(assertion.get("id"), "unknown");
            let details = assertion.get("details").and_then(Value::as_object);
            let track = details.and_then(details_track);
            if let Some(track) = &track {
                *adoption_tracks.entry(track.clone()).or_default() += 1;
            }
            let site = catalog
                .entry(assertion_id.clone())
                .or_insert_with(|| AssertionSite {
                    id: assertion_id.clone(),
                    message: value_to_string(assertion.get("message"), "<unnamed>"),
                    assert_type: value_to_string(assertion.get("assert_type"), "unknown"),
                    category: details
                        .and_then(|details| details.get("category"))
                        .map(|value| value_to_string(Some(value), "uncategorized"))
                        .unwrap_or_else(|| "uncategorized".to_string()),
                    observed: false,
                    observed_hits: 0,
                    success_count: 0,
                    failure_count: 0,
                    adoption_tracks: Vec::new(),
                });
            if let Some(track) = track {
                if !site.adoption_tracks.contains(&track) {
                    site.adoption_tracks.push(track);
                }
            }
            if !assertion
                .get("hit")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            exercised.insert(assertion_id.clone());
            site.observed = true;
            site.observed_hits += 1;
            let condition = assertion
                .get("condition")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if condition {
                site.success_count += 1;
            } else {
                site.failure_count += 1;
                failed += 1;
            }
            if site.assert_type == "sometimes" && condition {
                sometimes_success.insert(assertion_id.clone());
            }
            if site.assert_type == "reachability" && condition {
                reachable_hit.insert(assertion_id);
            }
            continue;
        }

        if let Some(setup) = object.get("antithesis_setup") {
            setup_complete = true;
            *lifecycle.entry("setup_complete".to_string()).or_default() += 1;
            if let Some(track) = setup
                .as_object()
                .and_then(|setup| setup.get("details"))
                .and_then(Value::as_object)
                .and_then(details_track)
            {
                *adoption_tracks.entry(track).or_default() += 1;
            }
        } else if object.contains_key("chaoscontrol_random_choice") {
            random_choice_calls += 1;
        } else if let Some((event_name, event_value)) = object.iter().next() {
            *lifecycle.entry(event_name.clone()).or_default() += 1;
            if let Some(track) = event_value.as_object().and_then(details_track) {
                *adoption_tracks.entry(track).or_default() += 1;
            }
        }
    }

    let sometimes_without_success = catalog
        .iter()
        .filter(|(id, site)| site.assert_type == "sometimes" && !sometimes_success.contains(*id))
        .map(|(_, site)| site.message.clone())
        .collect::<Vec<_>>();
    let reachable_without_hit = catalog
        .iter()
        .filter(|(id, site)| site.assert_type == "reachability" && !reachable_hit.contains(*id))
        .map(|(_, site)| site.message.clone())
        .collect::<Vec<_>>();
    let uncategorized = catalog
        .values()
        .filter(|site| site.category == "uncategorized")
        .count() as u64;
    let unobserved_assertions = catalog
        .values()
        .filter(|site| !site.observed)
        .map(|site| site.message.clone())
        .collect::<Vec<_>>();

    let mut gaps = Vec::new();
    if !setup_complete {
        gaps.push("missing setup_complete lifecycle event".to_string());
    }
    if uncategorized > 0 {
        gaps.push(format!("{uncategorized} uncategorized assertion(s)"));
    }
    if !sometimes_without_success.is_empty() {
        gaps.push(format!(
            "{} sometimes assertion(s) without observed success",
            sometimes_without_success.len()
        ));
    }
    if !reachable_without_hit.is_empty() {
        gaps.push(format!(
            "{} reachable assertion(s) without observed hit",
            reachable_without_hit.len()
        ));
    }

    let assertion_coverage = catalog
        .values()
        .map(assertion_site_value)
        .collect::<Vec<_>>();

    let mut report = Map::new();
    report.insert(
        "adoption_tracks".to_string(),
        count_map_value(&adoption_tracks),
    );
    report.insert(
        "assertion_coverage".to_string(),
        Value::Array(assertion_coverage),
    );
    report.insert(
        "cataloged_assertions".to_string(),
        Value::from(catalog.len() as u64),
    );
    report.insert(
        "evidence_class".to_string(),
        Value::String(evidence_class.to_string()),
    );
    report.insert(
        "exercised_assertions".to_string(),
        Value::from(exercised.len() as u64),
    );
    report.insert("failed_assertions".to_string(), Value::from(failed));
    report.insert("gaps".to_string(), string_array(gaps));
    report.insert(
        "instrumentation_sources".to_string(),
        count_map_value(&adoption_tracks),
    );
    report.insert("lifecycle_events".to_string(), count_map_value(&lifecycle));
    report.insert(
        "observed_assertions".to_string(),
        Value::from(exercised.len() as u64),
    );
    report.insert(
        "random_choice_calls".to_string(),
        Value::from(random_choice_calls),
    );
    report.insert(
        "reachable_without_hit".to_string(),
        string_array(reachable_without_hit),
    );
    report.insert(
        "registered_assertions".to_string(),
        Value::from(catalog.len() as u64),
    );
    report.insert(
        "replay_boundary".to_string(),
        Value::String(
            "local SDK JSONL proves instrumentation shape only; VM campaign and replay artifacts must be reviewed separately"
                .to_string(),
        ),
    );
    report.insert("replay_evidence".to_string(), Value::Bool(false));
    report.insert(
        "schema".to_string(),
        Value::String("chaoscontrol.sdk.local_report.v1".to_string()),
    );
    report.insert("setup_complete".to_string(), Value::Bool(setup_complete));
    report.insert(
        "sometimes_without_success".to_string(),
        string_array(sometimes_without_success),
    );
    report.insert(
        "uncategorized_assertions".to_string(),
        Value::from(uncategorized),
    );
    report.insert(
        "unobserved_assertion_count".to_string(),
        Value::from(unobserved_assertions.len() as u64),
    );
    report.insert(
        "unobserved_assertions".to_string(),
        string_array(unobserved_assertions),
    );
    Ok(Value::Object(report))
}

pub fn write_sdk_local_report(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    evidence_class: &str,
) -> EvidenceResult<Value> {
    let report = summarize_sdk_local_report(input_path, evidence_class)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(report)
}

pub fn check_sdk_local_report_tracks() -> EvidenceResult<String> {
    let harness = "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":{\"adoption_track\":\"external-harness\"}}}\n{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"must_hit\":false,\"id\":\"1\",\"message\":\"driver invariant\",\"display_type\":\"always\",\"details\":{\"category\":\"driver\",\"adoption_track\":\"external-harness\"}}}\n";
    let in_process = "{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"must_hit\":false,\"id\":\"2\",\"message\":\"internal invariant\",\"display_type\":\"always\",\"details\":{\"category\":\"service-invariant\",\"instrumentation_source\":\"in-process-service\"}}}\n";
    assert_tracks("harness-only", harness, &[("external-harness", 2)])?;
    assert_tracks("in-process-only", in_process, &[("in-process-service", 1)])?;
    assert_tracks(
        "mixed",
        &format!("{harness}{in_process}"),
        &[("external-harness", 2), ("in-process-service", 1)],
    )?;
    Ok("sdk-local-report-tracks: ok".to_string())
}

fn assert_tracks(name: &str, content: &str, expected: &[(&str, u64)]) -> EvidenceResult<()> {
    let report = summarize_sdk_local_jsonl(content, DEFAULT_SDK_LOCAL_EVIDENCE_CLASS, None)?;
    let expected = expected
        .iter()
        .map(|(key, value)| ((*key).to_string(), *value))
        .collect::<BTreeMap<_, _>>();
    let actual = count_object(report.get("adoption_tracks"));
    if actual != expected {
        return Err(EvidenceError::new(format!(
            "{name}: expected {expected:?}, got {actual:?}"
        )));
    }
    let sources = count_object(report.get("instrumentation_sources"));
    if sources != expected {
        return Err(EvidenceError::new(format!(
            "{name}: instrumentation_sources drifted"
        )));
    }
    if report.get("replay_evidence") != Some(&Value::Bool(false)) {
        return Err(EvidenceError::new(format!(
            "{name}: local report claimed replay evidence"
        )));
    }
    Ok(())
}

fn details_track(details: &Map<String, Value>) -> Option<String> {
    details
        .get("adoption_track")
        .or_else(|| details.get("instrumentation_source"))
        .map(|value| value_to_string(Some(value), ""))
}

fn value_to_string(value: Option<&Value>, default: &str) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => default.to_string(),
        Some(value) => value.to_string(),
    }
}

fn assertion_site_value(site: &AssertionSite) -> Value {
    let mut value = Map::new();
    value.insert(
        "adoption_tracks".to_string(),
        string_array(site.adoption_tracks.clone()),
    );
    value.insert(
        "assert_type".to_string(),
        Value::String(site.assert_type.clone()),
    );
    value.insert("category".to_string(), Value::String(site.category.clone()));
    value.insert("failure_count".to_string(), Value::from(site.failure_count));
    value.insert("id".to_string(), Value::String(site.id.clone()));
    value.insert("message".to_string(), Value::String(site.message.clone()));
    value.insert("observed".to_string(), Value::Bool(site.observed));
    value.insert("observed_hits".to_string(), Value::from(site.observed_hits));
    value.insert("success_count".to_string(), Value::from(site.success_count));
    Value::Object(value)
}

fn count_map_value(values: &BTreeMap<String, u64>) -> Value {
    let mut object = Map::new();
    for (key, value) in values {
        object.insert(key.clone(), Value::from(*value));
    }
    Value::Object(object)
}

fn count_object(value: Option<&Value>) -> BTreeMap<String, u64> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| value.as_u64().map(|value| (key.clone(), value)))
                .collect()
        })
        .unwrap_or_default()
}

fn string_array(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}
