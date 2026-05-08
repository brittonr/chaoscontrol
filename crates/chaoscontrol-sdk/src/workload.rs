//! Rust-first workload harness helpers.
//!
//! This module is intentionally small: it keeps the existing SDK primitives
//! as the source of truth while giving downstream Rust projects a repeatable
//! setup/scenario/report shape.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;
use std::time::Instant;

/// Minimal harness metadata for a Rust workload.
#[derive(Debug, Clone)]
pub struct WorkloadHarness {
    name: String,
}

impl WorkloadHarness {
    /// Create a harness for one downstream Rust workload.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Return the workload name used in lifecycle/report metadata.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Initialize ChaosControl SDK transport/catalog emission.
    pub fn init(&self) {
        crate::chaoscontrol_init();
        crate::lifecycle::send_event(
            "workload_init",
            &json!({
                "workload": self.name,
            }),
        );
    }

    /// Mark workload setup complete and attach the workload name to details.
    pub fn setup_complete(&self, mut details: Value) {
        if let Value::Object(ref mut object) = details {
            object
                .entry("workload")
                .or_insert_with(|| Value::String(self.name.clone()));
        }
        crate::lifecycle::setup_complete(&details);
    }

    /// Run a named scenario and emit start/finish lifecycle events.
    pub fn scenario<T>(&self, name: &str, run: impl FnOnce() -> T) -> T {
        crate::lifecycle::send_event(
            "scenario_start",
            &json!({
                "workload": self.name,
                "scenario": name,
            }),
        );
        let started = Instant::now();
        let result = run();
        crate::lifecycle::send_event(
            "scenario_finish",
            &json!({
                "workload": self.name,
                "scenario": name,
                "elapsed_ms": started.elapsed().as_millis(),
            }),
        );
        result
    }
}

/// Per-assertion local dry-run coverage detail.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssertionCoverage {
    pub id: String,
    pub message: String,
    pub assert_type: String,
    pub category: String,
    pub observed: bool,
    pub observed_hits: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub adoption_tracks: Vec<String>,
}

/// Parsed local dry-run report from `CHAOSCONTROL_SDK_LOCAL_OUTPUT` JSONL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalDryRunReport {
    pub setup_complete: bool,
    pub lifecycle_events: BTreeMap<String, usize>,
    pub cataloged_assertions: usize,
    pub exercised_assertions: usize,
    pub failed_assertions: usize,
    pub sometimes_without_success: Vec<String>,
    pub reachable_without_hit: Vec<String>,
    pub uncategorized_assertions: usize,
    pub random_choice_calls: usize,
    pub assertion_coverage: Vec<AssertionCoverage>,
    pub unobserved_assertions: Vec<String>,
    pub adoption_tracks: BTreeMap<String, usize>,
}

impl LocalDryRunReport {
    /// Parse a local JSONL output file emitted by the SDK.
    pub fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::from_jsonl(&fs::read_to_string(path)?)
    }

    /// Parse local JSONL output content emitted by the SDK.
    pub fn from_jsonl(content: &str) -> io::Result<Self> {
        let mut report = LocalDryRunReport::default();
        let mut catalog = BTreeMap::<String, CatalogSite>::new();
        let mut exercised = BTreeSet::<String>::new();
        let mut sometimes_success = BTreeSet::<String>::new();
        let mut reachable_hit = BTreeSet::<String>::new();

        fn details_track(details: &Value) -> Option<String> {
            details
                .get("adoption_track")
                .or_else(|| details.get("instrumentation_source"))
                .and_then(Value::as_str)
                .map(str::to_string)
        }

        fn note_track(report: &mut LocalDryRunReport, track: Option<String>) -> Option<String> {
            let selected = track?;
            *report.adoption_tracks.entry(selected.clone()).or_default() += 1;
            Some(selected)
        }

        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid SDK JSONL at line {}: {err}", line_no + 1),
                )
            })?;

            if let Some(assertion) = value.get("antithesis_assert") {
                let id = assertion
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let message = assertion
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("<unnamed>")
                    .to_string();
                let assert_type = assertion
                    .get("assert_type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let hit = assertion
                    .get("hit")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let condition = assertion
                    .get("condition")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let details = assertion.get("details").unwrap_or(&Value::Null);
                let category = details
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("uncategorized")
                    .to_string();
                let track = note_track(&mut report, details_track(details));

                let site = catalog.entry(id.clone()).or_insert(CatalogSite {
                    message: message.clone(),
                    assert_type: assert_type.clone(),
                    category: category.clone(),
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

                if !hit {
                    continue;
                }

                exercised.insert(id.clone());
                site.observed_hits += 1;
                if condition {
                    site.success_count += 1;
                } else {
                    site.failure_count += 1;
                    report.failed_assertions += 1;
                }
                match assert_type.as_str() {
                    "sometimes" if condition => {
                        sometimes_success.insert(id);
                    }
                    "reachability" if condition => {
                        reachable_hit.insert(id);
                    }
                    _ => {}
                }
            } else if let Some(setup) = value.get("antithesis_setup") {
                report.setup_complete = true;
                *report
                    .lifecycle_events
                    .entry("setup_complete".to_string())
                    .or_default() += 1;
                note_track(&mut report, setup.get("details").and_then(details_track));
            } else if value.get("chaoscontrol_random_choice").is_some() {
                report.random_choice_calls += 1;
            } else if let Some((event, payload)) = value.as_object().and_then(|o| o.iter().next()) {
                *report.lifecycle_events.entry(event.clone()).or_default() += 1;
                note_track(&mut report, details_track(payload));
            }
        }

        report.cataloged_assertions = catalog.len();
        report.exercised_assertions = exercised.len();
        report.uncategorized_assertions = catalog
            .values()
            .filter(|site| site.category == "uncategorized")
            .count();

        for (id, site) in catalog {
            let observed = site.observed_hits > 0;
            if !observed {
                report.unobserved_assertions.push(site.message.clone());
            }
            if site.assert_type == "sometimes" && !sometimes_success.contains(&id) {
                report.sometimes_without_success.push(site.message.clone());
            }
            if site.assert_type == "reachability" && !reachable_hit.contains(&id) {
                report.reachable_without_hit.push(site.message.clone());
            }
            report.assertion_coverage.push(AssertionCoverage {
                id,
                message: site.message,
                assert_type: site.assert_type,
                category: site.category,
                observed,
                observed_hits: site.observed_hits,
                success_count: site.success_count,
                failure_count: site.failure_count,
                adoption_tracks: site.adoption_tracks,
            });
        }

        Ok(report)
    }

    /// Return concise human-facing instrumentation gaps.
    pub fn gaps(&self) -> Vec<String> {
        let mut gaps = Vec::new();
        if !self.setup_complete {
            gaps.push("missing setup_complete lifecycle event".to_string());
        }
        if self.uncategorized_assertions > 0 {
            gaps.push(format!(
                "{} uncategorized assertion(s)",
                self.uncategorized_assertions
            ));
        }
        if !self.sometimes_without_success.is_empty() {
            gaps.push(format!(
                "{} sometimes assertion(s) without observed success",
                self.sometimes_without_success.len()
            ));
        }
        if !self.reachable_without_hit.is_empty() {
            gaps.push(format!(
                "{} reachable assertion(s) without observed hit",
                self.reachable_without_hit.len()
            ));
        }
        gaps
    }
}

#[derive(Debug, Clone)]
struct CatalogSite {
    message: String,
    assert_type: String,
    category: String,
    observed_hits: usize,
    success_count: usize,
    failure_count: usize,
    adoption_tracks: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_flags_missing_setup_and_unexercised_assertions() {
        let content = r#"
{"antithesis_assert":{"assert_type":"sometimes","condition":false,"hit":false,"must_hit":true,"id":"00000001","message":"write succeeds","display_type":"sometimes","details":{"category":"operation"}}}
{"antithesis_assert":{"assert_type":"reachability","condition":false,"hit":false,"must_hit":true,"id":"00000002","message":"leader elected","display_type":"reachability","details":{"category":"branch"}}}
{"antithesis_assert":{"assert_type":"always","condition":true,"hit":false,"must_hit":false,"id":"00000003","message":"single leader","display_type":"always","details":{"category":"uncategorized"}}}
{"chaoscontrol_random_choice":{"n":3,"choice":1}}
"#;
        let report = LocalDryRunReport::from_jsonl(content).unwrap();
        assert!(!report.setup_complete);
        assert_eq!(report.cataloged_assertions, 3);
        assert_eq!(report.exercised_assertions, 0);
        assert_eq!(report.random_choice_calls, 1);
        assert_eq!(report.uncategorized_assertions, 1);
        assert_eq!(report.unobserved_assertions.len(), 3);
        assert_eq!(report.assertion_coverage.len(), 3);
        assert!(report
            .assertion_coverage
            .iter()
            .all(|coverage| !coverage.observed && coverage.observed_hits == 0));
        assert_eq!(report.sometimes_without_success, vec!["write succeeds"]);
        assert_eq!(report.reachable_without_hit, vec!["leader elected"]);
        assert!(report
            .gaps()
            .contains(&"missing setup_complete lifecycle event".to_string()));
    }

    #[test]
    fn report_records_setup_and_successful_sometimes() {
        let content = r#"
{"antithesis_setup":{"status":"complete","details":{"workload":"sample"}}}
{"antithesis_assert":{"assert_type":"sometimes","condition":false,"hit":false,"must_hit":true,"id":"00000001","message":"write succeeds","display_type":"sometimes","details":{"category":"operation"}}}
{"antithesis_assert":{"assert_type":"sometimes","condition":true,"hit":true,"must_hit":true,"id":"00000001","message":"write succeeds","display_type":"sometimes","details":{}}}
"#;
        let report = LocalDryRunReport::from_jsonl(content).unwrap();
        assert!(report.setup_complete);
        assert_eq!(report.lifecycle_events.get("setup_complete"), Some(&1));
        assert_eq!(report.cataloged_assertions, 1);
        assert_eq!(report.exercised_assertions, 1);
        assert!(report.sometimes_without_success.is_empty());
        assert!(report.unobserved_assertions.is_empty());
        assert_eq!(report.assertion_coverage[0].observed_hits, 1);
        assert_eq!(report.assertion_coverage[0].success_count, 1);
    }

    #[test]
    fn report_records_adoption_tracks() {
        let content = r#"
{"antithesis_setup":{"status":"complete","details":{"workload":"sample","adoption_track":"external-harness"}}}
{"scenario_start":{"workload":"sample","scenario":"drive","adoption_track":"external-harness"}}
{"antithesis_assert":{"assert_type":"always","condition":true,"hit":true,"must_hit":false,"id":"00000001","message":"driver invariant","display_type":"always","details":{"category":"operation","adoption_track":"external-harness"}}}
{"antithesis_assert":{"assert_type":"always","condition":true,"hit":true,"must_hit":false,"id":"00000002","message":"internal invariant","display_type":"always","details":{"category":"service-invariant","instrumentation_source":"in-process-service"}}}
"#;
        let report = LocalDryRunReport::from_jsonl(content).unwrap();
        assert_eq!(report.adoption_tracks.get("external-harness"), Some(&3));
        assert_eq!(report.adoption_tracks.get("in-process-service"), Some(&1));
        assert_eq!(
            report.assertion_coverage[0].adoption_tracks,
            vec!["external-harness"]
        );
        assert_eq!(
            report.assertion_coverage[1].adoption_tracks,
            vec!["in-process-service"]
        );
    }
}
