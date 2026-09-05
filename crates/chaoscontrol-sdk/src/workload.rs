//! Rust-first workload harness helpers.
//!
//! This module is intentionally small: it keeps the existing SDK primitives
//! as the source of truth while giving downstream Rust projects a repeatable
//! setup/scenario/report shape.

const MAX_LOCAL_JSONL_BYTES: usize = 16 * 1024 * 1024;
const MAX_LOCAL_JSONL_LINE_BYTES: usize = 16 * 1024;
const MAX_LOCAL_JSONL_EVENTS: usize = 65_536;

/// Minimal harness metadata for a Rust workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadAdapterIdentity {
    pub workload: String,
    pub adapter_version: String,
    pub scenario: String,
    pub seed_or_schedule_ref: String,
    pub evidence_class: WorkloadEvidenceClass,
    pub artifact_digests: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadEvidenceClass {
    SimulatorLocal,
    VmSnapshotReplay,
}

impl WorkloadEvidenceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SimulatorLocal => "simulator-local",
            Self::VmSnapshotReplay => "vm-snapshot-replay",
        }
    }
}

/// Minimal harness metadata for a Rust workload.
#[derive(Debug, Clone)]
pub struct WorkloadHarness {
    name: String,
    adapter_version: Option<String>,
    artifact_digests: std::collections::BTreeMap<String, String>,
}

impl WorkloadHarness {
    /// Create a harness for one downstream Rust workload.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            adapter_version: None,
            artifact_digests: std::collections::BTreeMap::new(),
        }
    }

    /// Attach a shared simulator/VM adapter version to workload lifecycle metadata.
    pub fn with_adapter_version(mut self, adapter_version: impl Into<String>) -> Self {
        self.adapter_version = Some(adapter_version.into());
        self
    }

    /// Attach a named artifact digest that should appear in simulator/VM bridge metadata.
    pub fn with_artifact_digest(
        mut self,
        name: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        self.artifact_digests.insert(name.into(), digest.into());
        self
    }

    /// Build comparable adapter identity metadata for simulator or VM/hypervisor receipts.
    pub fn adapter_identity(
        &self,
        scenario: impl Into<String>,
        seed_or_schedule_ref: impl Into<String>,
        evidence_class: WorkloadEvidenceClass,
    ) -> WorkloadAdapterIdentity {
        WorkloadAdapterIdentity {
            workload: self.name.clone(),
            adapter_version: self
                .adapter_version
                .clone()
                .unwrap_or_else(|| "unspecified-adapter".to_string()),
            scenario: scenario.into(),
            seed_or_schedule_ref: seed_or_schedule_ref.into(),
            evidence_class,
            artifact_digests: self.artifact_digests.clone(),
        }
    }

    /// Serialize adapter identity metadata for SDK lifecycle event details.
    pub fn adapter_identity_json(
        &self,
        scenario: impl Into<String>,
        seed_or_schedule_ref: impl Into<String>,
        evidence_class: WorkloadEvidenceClass,
    ) -> ::serde_json::Value {
        let identity = self.adapter_identity(scenario, seed_or_schedule_ref, evidence_class);
        ::serde_json::json!({
            "workload": identity.workload,
            "adapter_version": identity.adapter_version,
            "scenario": identity.scenario,
            "seed_or_schedule_ref": identity.seed_or_schedule_ref,
            "evidence_class": identity.evidence_class.as_str(),
            "artifact_digests": identity.artifact_digests,
        })
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
            &::serde_json::json!({
                "workload": self.name,
                "adapter_version": self.adapter_version,
                "artifact_digests": self.artifact_digests,
            }),
        );
    }

    /// Mark workload setup complete and attach the workload name to details.
    pub fn setup_complete(&self, mut details: ::serde_json::Value) {
        if let ::serde_json::Value::Object(ref mut object) = details {
            object
                .entry("workload")
                .or_insert_with(|| ::serde_json::Value::String(self.name.clone()));
        }
        crate::lifecycle::setup_complete(&details);
    }

    /// Run a named scenario and emit start/finish lifecycle events.
    pub fn scenario<T>(&self, name: &str, run: impl FnOnce() -> T) -> T {
        crate::lifecycle::send_event(
            "scenario_start",
            &::serde_json::json!({
                "workload": self.name,
                "scenario": name,
                "adapter_version": self.adapter_version,
                "artifact_digests": self.artifact_digests,
            }),
        );
        let started = scenario_clock_now();
        let result = run();
        crate::lifecycle::send_event(
            "scenario_finish",
            &::serde_json::json!({
                "workload": self.name,
                "scenario": name,
                "adapter_version": self.adapter_version,
                "artifact_digests": self.artifact_digests,
                "elapsed_ms": scenario_elapsed_ms(started),
            }),
        );
        result
    }
}

#[allow(unknown_lints)]
#[allow(
    ambient_clock,
    reason = "workload harness shell measures host-side scenario lifecycle duration"
)]
fn scenario_clock_now() -> std::time::Instant {
    std::time::Instant::now()
}

#[allow(unknown_lints)]
#[allow(
    ambient_clock,
    reason = "workload harness shell measures host-side scenario lifecycle duration"
)]
fn scenario_elapsed_ms(started: std::time::Instant) -> u128 {
    started.elapsed().as_millis()
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
    /// This compatibility parser is diagnostic-only and never collision-safe.
    pub collision_safe_evidence: bool,
    pub setup_complete: bool,
    pub lifecycle_events: std::collections::BTreeMap<String, usize>,
    pub cataloged_assertions: usize,
    pub exercised_assertions: usize,
    pub failed_assertions: usize,
    pub sometimes_without_success: Vec<String>,
    pub reachable_without_hit: Vec<String>,
    pub uncategorized_assertions: usize,
    pub random_choice_calls: usize,
    pub assertion_coverage: Vec<AssertionCoverage>,
    pub unobserved_assertions: Vec<String>,
    pub adoption_tracks: std::collections::BTreeMap<String, usize>,
}

impl LocalDryRunReport {
    /// Parse a local JSONL output file emitted by the SDK.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> ::std::io::Result<Self> {
        let content = crate::local_json_security::read_bounded_regular_file(
            path.as_ref(),
            MAX_LOCAL_JSONL_BYTES,
        )?;
        Self::from_jsonl(&content)
    }

    /// Parse local JSONL output content emitted by the SDK.
    ///
    /// This compatibility API rejects conflicting legacy metadata, but it is
    /// diagnostic-only. Use the evidence crate's strict report for promotion.
    pub fn from_jsonl(content: &str) -> ::std::io::Result<Self> {
        if content.len() > MAX_LOCAL_JSONL_BYTES {
            return Err(invalid_data("SDK JSONL exceeds the input byte limit"));
        }
        let mut report = LocalDryRunReport::default();
        let mut catalog = std::collections::BTreeMap::<String, CatalogSite>::new();
        let mut exercised = std::collections::BTreeSet::<String>::new();
        let mut sometimes_success = std::collections::BTreeSet::<String>::new();
        let mut reachable_hit = std::collections::BTreeSet::<String>::new();

        fn details_track(details: &::serde_json::Value) -> Option<String> {
            details
                .get("adoption_track")
                .or_else(|| details.get("instrumentation_source"))
                .and_then(::serde_json::Value::as_str)
                .map(str::to_string)
        }

        fn note_track(report: &mut LocalDryRunReport, track: Option<String>) -> Option<String> {
            let selected = track?;
            *report.adoption_tracks.entry(selected.clone()).or_default() += 1;
            Some(selected)
        }

        let mut event_count = 0_usize;
        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.len() > MAX_LOCAL_JSONL_LINE_BYTES {
                return Err(invalid_data("SDK JSONL line exceeds the byte limit"));
            }
            crate::local_json_security::preflight_json_line(trimmed)?;
            event_count = event_count
                .checked_add(1)
                .ok_or_else(|| invalid_data("SDK JSONL event count overflow"))?;
            if event_count > MAX_LOCAL_JSONL_EVENTS {
                return Err(invalid_data("SDK JSONL event count exceeds the limit"));
            }
            let value: ::serde_json::Value = serde_json::from_str(trimmed).map_err(|err| {
                ::std::io::Error::new(
                    ::std::io::ErrorKind::InvalidData,
                    format!("invalid SDK JSONL at line {}: {err}", line_no + 1),
                )
            })?;

            if let Some(assertion) = value.get("antithesis_assert") {
                let id = required_string(assertion, "id", line_no)?;
                let message = required_string(assertion, "message", line_no)?;
                let assert_type = required_string(assertion, "assert_type", line_no)?;
                if !matches!(
                    assert_type.as_str(),
                    "always" | "sometimes" | "reachability"
                ) {
                    return Err(invalid_data(format!(
                        "unknown assertion type at line {}",
                        line_no + 1
                    )));
                }
                let hit = required_bool(assertion, "hit", line_no)?;
                let condition = required_bool(assertion, "condition", line_no)?;
                let details = assertion
                    .get("details")
                    .unwrap_or(&::serde_json::Value::Null);
                let explicit_category = details
                    .get("category")
                    .and_then(::serde_json::Value::as_str)
                    .map(str::to_string);
                let category = explicit_category
                    .clone()
                    .unwrap_or_else(|| "uncategorized".to_string());
                let track = note_track(&mut report, details_track(details));

                if let Some(existing) = catalog.get_mut(&id) {
                    if existing.message != message || existing.assert_type != assert_type {
                        return Err(invalid_data(format!(
                            "conflicting assertion metadata for ID {id} at line {}",
                            line_no + 1
                        )));
                    }
                    if let Some(explicit) = &explicit_category {
                        if existing.category_explicit && existing.category != *explicit {
                            return Err(invalid_data(format!(
                                "conflicting assertion category for ID {id} at line {}",
                                line_no + 1
                            )));
                        }
                        if !existing.category_explicit {
                            existing.category = explicit.clone();
                            existing.category_explicit = true;
                        }
                    }
                } else {
                    catalog.insert(
                        id.clone(),
                        CatalogSite {
                            message: message.clone(),
                            assert_type: assert_type.clone(),
                            category,
                            category_explicit: explicit_category.is_some(),
                            observed_hits: 0,
                            success_count: 0,
                            failure_count: 0,
                            adoption_tracks: Vec::new(),
                        },
                    );
                }
                let site = catalog
                    .get_mut(&id)
                    .expect("assertion site was inserted or validated");
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
            } else if value.get("antithesis_setup").is_some() {
                let setup = exact_setup_record(&value, line_no)?;
                if report.setup_complete {
                    return Err(invalid_data(format!(
                        "duplicate setup record at line {}",
                        line_no + 1
                    )));
                }
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

fn exact_setup_record(
    value: &::serde_json::Value,
    line_no: usize,
) -> ::std::io::Result<&serde_json::Map<String, ::serde_json::Value>> {
    let outer = value
        .as_object()
        .filter(|outer| outer.len() == 1)
        .ok_or_else(|| invalid_data(format!("invalid setup record at line {}", line_no + 1)))?;
    let setup = outer
        .get("antithesis_setup")
        .and_then(::serde_json::Value::as_object)
        .filter(|setup| setup.len() == 2)
        .ok_or_else(|| invalid_data(format!("invalid setup record at line {}", line_no + 1)))?;
    if setup.get("status").and_then(::serde_json::Value::as_str) != Some("complete")
        || !setup
            .get("details")
            .is_some_and(::serde_json::Value::is_object)
    {
        return Err(invalid_data(format!(
            "invalid setup record at line {}",
            line_no + 1
        )));
    }
    Ok(setup)
}

fn invalid_data(message: impl Into<String>) -> ::std::io::Error {
    ::std::io::Error::new(::std::io::ErrorKind::InvalidData, message.into())
}

fn required_string(
    value: &::serde_json::Value,
    field: &str,
    line_no: usize,
) -> ::std::io::Result<String> {
    let selected = value
        .get(field)
        .and_then(::serde_json::Value::as_str)
        .filter(|selected| !selected.is_empty())
        .ok_or_else(|| {
            invalid_data(format!(
                "assertion {field} must be a non-empty string at line {}",
                line_no + 1
            ))
        })?;
    Ok(selected.to_string())
}

fn required_bool(
    value: &::serde_json::Value,
    field: &str,
    line_no: usize,
) -> ::std::io::Result<bool> {
    value
        .get(field)
        .and_then(::serde_json::Value::as_bool)
        .ok_or_else(|| {
            invalid_data(format!(
                "assertion {field} must be a boolean at line {}",
                line_no + 1
            ))
        })
}

#[derive(Debug, Clone)]
struct CatalogSite {
    message: String,
    assert_type: String,
    category: String,
    category_explicit: bool,
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
    #[test]
    fn legacy_diagnostic_parser_rejects_identity_conflicts() {
        let base = r#"{"antithesis_assert":{"assert_type":"always","condition":true,"hit":true,"id":"same-id","message":"base","details":{"category":"invariant"}}}"#;
        let conflicts = [
            r#"{"antithesis_assert":{"assert_type":"always","condition":true,"hit":true,"id":"same-id","message":"other","details":{"category":"invariant"}}}"#,
            r#"{"antithesis_assert":{"assert_type":"sometimes","condition":true,"hit":true,"id":"same-id","message":"base","details":{"category":"invariant"}}}"#,
            r#"{"antithesis_assert":{"assert_type":"always","condition":true,"hit":true,"id":"same-id","message":"base","details":{"category":"recovery"}}}"#,
        ];
        for conflict in conflicts {
            assert!(LocalDryRunReport::from_jsonl(&format!("{base}\n{conflict}\n")).is_err());
        }
        let malformed_id = r#"{"antithesis_assert":{"assert_type":"always","condition":true,"hit":true,"id":"","message":"base","details":{}}}"#;
        assert!(LocalDryRunReport::from_jsonl(malformed_id).is_err());
        let diagnostic = LocalDryRunReport::from_jsonl(&format!("{base}\n{base}\n"))
            .expect("exact duplicate legacy records");
        assert!(!diagnostic.collision_safe_evidence);
    }

    #[test]
    fn diagnostic_setup_and_json_shape_fail_closed() {
        const OVER_DEEP_LEVELS: usize = 65;
        const OVER_STRING_BUDGET: usize = 13 * 1024;
        for invalid in [
            r#"{"antithesis_setup":null}"#,
            r#"{"antithesis_setup":{"status":"pending","details":{}}}"#,
            r#"{"antithesis_setup":{"status":"complete","details":null}}"#,
            r#"{"antithesis_setup":{"status":"complete","details":{},"extra":true}}"#,
        ] {
            assert!(LocalDryRunReport::from_jsonl(invalid).is_err());
        }
        let valid_setup = r#"{"antithesis_setup":{"status":"complete","details":{}}}"#;
        assert!(LocalDryRunReport::from_jsonl(&format!("{valid_setup}\n{valid_setup}\n")).is_err());
        let deep = format!(
            "{}0{}",
            "[".repeat(OVER_DEEP_LEVELS),
            "]".repeat(OVER_DEEP_LEVELS)
        );
        assert!(LocalDryRunReport::from_jsonl(&deep).is_err());
        let strings = format!(r#"{{"value":"{}"}}"#, "x".repeat(OVER_STRING_BUDGET));
        assert!(LocalDryRunReport::from_jsonl(&strings).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_path_rejects_symlinks_and_non_regular_files() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("chaoscontrol-sdk-workload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create fixture directory");
        let target = root.join("report.jsonl");
        let link = root.join("report-link.jsonl");
        std::fs::write(
            &target,
            r#"{"antithesis_setup":{"status":"complete","details":{}}}"#,
        )
        .expect("write fixture");
        symlink(&target, &link).expect("create symlink");
        assert!(LocalDryRunReport::from_path(&link).is_err());
        assert!(LocalDryRunReport::from_path(&root).is_err());
        std::fs::remove_dir_all(root).expect("remove fixtures");
    }

    #[test]
    fn workload_harness_builds_shared_simulator_vm_adapter_identity() {
        let harness = WorkloadHarness::new("sample-rust-service")
            .with_adapter_version("sample-adapter-v1")
            .with_artifact_digest(
                "workload-adapter",
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            );
        let simulator = harness.adapter_identity(
            "writes survive failover",
            "seed:42 schedule:no-faults",
            WorkloadEvidenceClass::SimulatorLocal,
        );
        let vm = harness.adapter_identity(
            "writes survive failover",
            "seed:42 schedule:no-faults",
            WorkloadEvidenceClass::VmSnapshotReplay,
        );
        assert_eq!(simulator.workload, vm.workload);
        assert_eq!(simulator.adapter_version, "sample-adapter-v1");
        assert_eq!(simulator.scenario, vm.scenario);
        assert_eq!(simulator.seed_or_schedule_ref, vm.seed_or_schedule_ref);
        assert_ne!(simulator.evidence_class, vm.evidence_class);
        assert_eq!(simulator.evidence_class.as_str(), "simulator-local");
        assert_eq!(vm.evidence_class.as_str(), "vm-snapshot-replay");
    }
}
