//! Typed projection of the v1 Wasm component exploration profile contract.
//!
//! The Nickel contract `contracts/evidence/wasm-component-exploration-profile.ncl`
//! owns admission wording; this DTO and its validation mirror that contract
//! exactly so a Nickel-admitted profile deserializes unchanged and any drift
//! fails closed in the core too.

use serde::{Deserialize, Serialize};

use crate::{blake3_hex, canonical_json, is_git_revision, is_identifier};

pub const PROFILE_SCHEMA: &str = "chaoscontrol.wasm-component-exploration-profile.v1";
pub const BUNDLE_STATUS_NOT_YET_MATERIALIZED: &str = "not-yet-materialized";
pub const BUNDLE_STATUS_MATERIALIZED: &str = "materialized";
pub const TOOL_SUITE_SOURCE: &str = "nix-lock";
pub const WASM_TOOLS_SUITE: &str = "wasm-tools 1.245.1";
pub const WASMTIME_VERSION: &str = "wasmtime 43.0.0";
pub const RUNTIME_PROPOSALS: &str = "all-proposals-disabled";
pub const COMPONENT_FEATURES: &str = "post-mvp-features-disabled";
pub const HOST_AUTHORITY_NONE: &str = "none";
pub const EPOCH_DEADLINE_NONE: &str = "none";

pub const REQUIRED_GENERATORS: [&str; 3] = ["wasm-smith", "wit-smith", "wasm-tools-component"];
pub const REQUIRED_MUTATORS: [&str; 2] = ["wasm-mutate", "profile-aware"];
pub const REQUIRED_SHRINKERS: [&str; 2] = ["wasm-shrink", "bounded-reducer"];
pub const REQUIRED_STRATEGIES: [&str; 2] = ["cranelift", "pulley"];
pub const REQUIRED_CORPUS_CLASSES: [&str; 6] = [
    "baseline-component",
    "baseline-wit-package",
    "generated-malformed",
    "generated-valid",
    "mutated-invalid",
    "mutated-valid",
];
pub const REQUIRED_OUTCOME_CLASSES: [&str; 13] = [
    "bound-skip",
    "compile-rejection",
    "deterministic-trap",
    "harness-error",
    "link-rejection",
    "profile-rejection",
    "replay-mismatch",
    "shrink-complete",
    "static-rejection",
    "strategy-divergence",
    "strategy-match",
    "transform-divergence",
    "transform-match",
];
pub const REQUIRED_OBSERVATION_FIELDS: [&str; 7] = [
    "hostcall-transcript",
    "outcome",
    "preserves-output",
    "resource-class",
    "result-values",
    "state-identity-blake3",
    "trap-class",
];
pub const REQUIRED_NON_CLAIMS: [&str; 6] = [
    "not-assertion-readiness",
    "not-package-trust",
    "not-release-eligibility",
    "not-sandbox-proof",
    "not-semantic-equivalence",
    "not-vm-replay-proof",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationProfile {
    pub schema: String,
    pub profile_id: String,
    pub materialization: Materialization,
    pub tool_cohort: ToolCohort,
    pub runtime_cohort: RuntimeCohort,
    pub corpus_classes: Vec<String>,
    pub outcome_classes: Vec<String>,
    pub observation_fields: Vec<String>,
    pub bounds: ExplorationBounds,
    pub retention: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Materialization {
    pub mantle_revision: String,
    pub bundle_profile_id: String,
    pub bundle_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCohort {
    pub suite_source: String,
    pub wasm_tools_suite: String,
    pub generators: Vec<String>,
    pub mutators: Vec<String>,
    pub shrinkers: Vec<String>,
    pub waffle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCohort {
    pub wasmtime_version: String,
    pub strategies: Vec<String>,
    pub proposals: String,
    pub component_features: String,
    pub host_authority: String,
    pub epoch_deadline: String,
    pub wasmtime_fuel: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationBounds {
    pub maximum_artifact_bytes: u64,
    pub maximum_wit_bytes: u64,
    pub maximum_imports: u64,
    pub maximum_exports: u64,
    pub maximum_functions: u64,
    pub maximum_types: u64,
    pub maximum_memories: u64,
    pub maximum_tables: u64,
    pub maximum_hostcall_bytes: u64,
    pub maximum_generation_cases: u64,
    pub maximum_mutation_cases: u64,
    pub maximum_branches: u64,
    pub maximum_samples: u64,
    pub maximum_shrink_attempts: u64,
    pub maximum_runtime_steps: u64,
    pub maximum_concurrent_cases: u64,
    pub maximum_retained_artifacts: u64,
}

fn exact_set(field: &str, actual: &[String], required: &[&str]) -> Result<(), String> {
    let matches = actual.len() == required.len()
        && actual
            .iter()
            .zip(required.iter())
            .all(|(actual, required)| actual == required);
    if matches {
        Ok(())
    } else {
        Err(format!(
            "profile {field} must be exactly {required:?}, found {actual:?}"
        ))
    }
}

fn require_exact(field: &str, actual: &str, required: &str) -> Result<(), String> {
    if actual == required {
        Ok(())
    } else {
        Err(format!(
            "profile {field} must be {required:?}, found {actual:?}"
        ))
    }
}

fn require_positive(bounds: &ExplorationBounds) -> Result<(), String> {
    let values = [
        ("maximum_artifact_bytes", bounds.maximum_artifact_bytes),
        ("maximum_wit_bytes", bounds.maximum_wit_bytes),
        ("maximum_imports", bounds.maximum_imports),
        ("maximum_exports", bounds.maximum_exports),
        ("maximum_functions", bounds.maximum_functions),
        ("maximum_types", bounds.maximum_types),
        ("maximum_memories", bounds.maximum_memories),
        ("maximum_tables", bounds.maximum_tables),
        ("maximum_hostcall_bytes", bounds.maximum_hostcall_bytes),
        ("maximum_generation_cases", bounds.maximum_generation_cases),
        ("maximum_mutation_cases", bounds.maximum_mutation_cases),
        ("maximum_branches", bounds.maximum_branches),
        ("maximum_samples", bounds.maximum_samples),
        ("maximum_shrink_attempts", bounds.maximum_shrink_attempts),
        ("maximum_runtime_steps", bounds.maximum_runtime_steps),
        ("maximum_concurrent_cases", bounds.maximum_concurrent_cases),
        (
            "maximum_retained_artifacts",
            bounds.maximum_retained_artifacts,
        ),
    ];
    for (field, value) in values {
        if value == 0 {
            return Err(format!("profile bound {field} must be positive"));
        }
    }
    Ok(())
}

/// Validates one profile against the pinned v1 contract, including the
/// cross-bound coherence rules the Nickel contract enforces.
pub fn validate_profile(profile: &ExplorationProfile) -> Result<(), String> {
    if profile.schema != PROFILE_SCHEMA {
        return Err(format!(
            "profile schema must be {PROFILE_SCHEMA:?}, found {:?}",
            profile.schema
        ));
    }
    if !is_identifier(&profile.profile_id) {
        return Err(format!(
            "profile_id {:?} is not a bounded identifier",
            profile.profile_id
        ));
    }
    let materialization = &profile.materialization;
    if !is_git_revision(&materialization.mantle_revision) {
        return Err(
            "materialization.mantle_revision must be 40 lowercase hex characters".to_string(),
        );
    }
    if !is_identifier(&materialization.bundle_profile_id) {
        return Err("materialization.bundle_profile_id must be a bounded identifier".to_string());
    }
    // v1 pins the bundle status: no Mantle materialization bundle is admitted
    // yet, so proof-eligible admission must stay impossible.
    require_exact(
        "materialization.bundle_status",
        &materialization.bundle_status,
        BUNDLE_STATUS_NOT_YET_MATERIALIZED,
    )?;
    let tools = &profile.tool_cohort;
    require_exact(
        "tool_cohort.suite_source",
        &tools.suite_source,
        TOOL_SUITE_SOURCE,
    )?;
    require_exact(
        "tool_cohort.wasm_tools_suite",
        &tools.wasm_tools_suite,
        WASM_TOOLS_SUITE,
    )?;
    exact_set(
        "tool_cohort.generators",
        &tools.generators,
        &REQUIRED_GENERATORS,
    )?;
    exact_set("tool_cohort.mutators", &tools.mutators, &REQUIRED_MUTATORS)?;
    exact_set(
        "tool_cohort.shrinkers",
        &tools.shrinkers,
        &REQUIRED_SHRINKERS,
    )?;
    if let Some(waffle) = &tools.waffle {
        if waffle.is_empty() {
            return Err("tool_cohort.waffle must be null or a non-empty cohort pin".to_string());
        }
    }
    let runtime = &profile.runtime_cohort;
    require_exact(
        "runtime_cohort.wasmtime_version",
        &runtime.wasmtime_version,
        WASMTIME_VERSION,
    )?;
    exact_set(
        "runtime_cohort.strategies",
        &runtime.strategies,
        &REQUIRED_STRATEGIES,
    )?;
    require_exact(
        "runtime_cohort.proposals",
        &runtime.proposals,
        RUNTIME_PROPOSALS,
    )?;
    require_exact(
        "runtime_cohort.component_features",
        &runtime.component_features,
        COMPONENT_FEATURES,
    )?;
    require_exact(
        "runtime_cohort.host_authority",
        &runtime.host_authority,
        HOST_AUTHORITY_NONE,
    )?;
    require_exact(
        "runtime_cohort.epoch_deadline",
        &runtime.epoch_deadline,
        EPOCH_DEADLINE_NONE,
    )?;
    if runtime.wasmtime_fuel == 0 {
        return Err("runtime_cohort.wasmtime_fuel must be positive".to_string());
    }
    exact_set(
        "corpus_classes",
        &profile.corpus_classes,
        &REQUIRED_CORPUS_CLASSES,
    )?;
    exact_set(
        "outcome_classes",
        &profile.outcome_classes,
        &REQUIRED_OUTCOME_CLASSES,
    )?;
    exact_set(
        "observation_fields",
        &profile.observation_fields,
        &REQUIRED_OBSERVATION_FIELDS,
    )?;
    require_positive(&profile.bounds)?;
    let bounds = &profile.bounds;
    if bounds.maximum_wit_bytes > bounds.maximum_artifact_bytes {
        return Err("bound maximum_wit_bytes must not exceed maximum_artifact_bytes".to_string());
    }
    if bounds.maximum_retained_artifacts > bounds.maximum_samples {
        return Err("bound maximum_retained_artifacts must not exceed maximum_samples".to_string());
    }
    if bounds.maximum_shrink_attempts > bounds.maximum_mutation_cases {
        return Err(
            "bound maximum_shrink_attempts must not exceed maximum_mutation_cases".to_string(),
        );
    }
    if profile.retention.is_empty() {
        return Err("profile retention must be a non-empty plan statement".to_string());
    }
    exact_set("non_claims", &profile.non_claims, &REQUIRED_NON_CLAIMS)?;
    Ok(())
}

/// BLAKE3 identity of the profile facts.
pub fn profile_identity(profile: &ExplorationProfile) -> Result<String, String> {
    Ok(blake3_hex(&canonical_json(profile)?))
}

/// Proof-eligible admission requires an admitted materialization bundle. The
/// v1 contract pins the bundle to not-yet-materialized, so this stays false
/// for every valid v1 profile and every case remains diagnostic-only.
pub fn proof_eligible(profile: &ExplorationProfile) -> bool {
    profile.materialization.bundle_status == BUNDLE_STATUS_MATERIALIZED
}

#[cfg(test)]
pub(crate) fn example_profile() -> ExplorationProfile {
    ExplorationProfile {
        schema: PROFILE_SCHEMA.to_string(),
        profile_id: "wasm-component-exploration-v1".to_string(),
        materialization: Materialization {
            mantle_revision: "a141fcbaafe41f9a413a81275a33fe915bfca370".to_string(),
            bundle_profile_id: "wasm-component-exploration-closure-v1".to_string(),
            bundle_status: BUNDLE_STATUS_NOT_YET_MATERIALIZED.to_string(),
        },
        tool_cohort: ToolCohort {
            suite_source: TOOL_SUITE_SOURCE.to_string(),
            wasm_tools_suite: WASM_TOOLS_SUITE.to_string(),
            generators: REQUIRED_GENERATORS
                .iter()
                .map(|value| value.to_string())
                .collect(),
            mutators: REQUIRED_MUTATORS
                .iter()
                .map(|value| value.to_string())
                .collect(),
            shrinkers: REQUIRED_SHRINKERS
                .iter()
                .map(|value| value.to_string())
                .collect(),
            waffle: None,
        },
        runtime_cohort: RuntimeCohort {
            wasmtime_version: WASMTIME_VERSION.to_string(),
            strategies: REQUIRED_STRATEGIES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            proposals: RUNTIME_PROPOSALS.to_string(),
            component_features: COMPONENT_FEATURES.to_string(),
            host_authority: HOST_AUTHORITY_NONE.to_string(),
            epoch_deadline: EPOCH_DEADLINE_NONE.to_string(),
            wasmtime_fuel: 1_000_000,
        },
        corpus_classes: REQUIRED_CORPUS_CLASSES
            .iter()
            .map(|value| value.to_string())
            .collect(),
        outcome_classes: REQUIRED_OUTCOME_CLASSES
            .iter()
            .map(|value| value.to_string())
            .collect(),
        observation_fields: REQUIRED_OBSERVATION_FIELDS
            .iter()
            .map(|value| value.to_string())
            .collect(),
        bounds: ExplorationBounds {
            maximum_artifact_bytes: 16_777_216,
            maximum_wit_bytes: 1_048_576,
            maximum_imports: 4096,
            maximum_exports: 4096,
            maximum_functions: 4096,
            maximum_types: 4096,
            maximum_memories: 64,
            maximum_tables: 64,
            maximum_hostcall_bytes: 1_048_576,
            maximum_generation_cases: 4096,
            maximum_mutation_cases: 4096,
            maximum_branches: 4096,
            maximum_samples: 65_536,
            maximum_shrink_attempts: 4096,
            maximum_runtime_steps: 1_000_000_000,
            maximum_concurrent_cases: 64,
            maximum_retained_artifacts: 4096,
        },
        retention: "retain bounded exploration artifacts for the local evidence lane".to_string(),
        non_claims: REQUIRED_NON_CLAIMS
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_blake3_hex;

    #[test]
    fn nickel_example_profile_validates_and_has_a_stable_identity() {
        let profile = example_profile();
        validate_profile(&profile).expect("example profile must validate");
        let first = profile_identity(&profile).expect("identity");
        let second = profile_identity(&profile).expect("identity");
        assert_eq!(first, second);
        assert_eq!(first.len(), crate::BLAKE3_HEX_LENGTH);
    }

    #[test]
    fn proof_eligibility_tracks_the_bundle_status_and_v1_stays_diagnostic() {
        let mut materialized = example_profile();
        materialized.materialization.bundle_status = BUNDLE_STATUS_MATERIALIZED.to_string();
        assert!(proof_eligible(&materialized));

        let profile = example_profile();
        validate_profile(&profile).expect("v1 profile must validate");
        assert!(!proof_eligible(&profile));
    }

    #[test]
    fn cohort_drift_is_rejected() {
        let mut profile = example_profile();
        profile.tool_cohort.wasm_tools_suite = "wasm-tools 1.244.0".to_string();
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.runtime_cohort.strategies = vec!["cranelift".to_string()];
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.tool_cohort.generators = vec![
            "wasm-smith".to_string(),
            "wit-smith".to_string(),
            "unknown-generator".to_string(),
        ];
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn bound_inversion_and_zero_bounds_are_rejected() {
        let mut profile = example_profile();
        profile.bounds.maximum_wit_bytes = profile.bounds.maximum_artifact_bytes + 1;
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.bounds.maximum_shrink_attempts = profile.bounds.maximum_mutation_cases + 1;
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.bounds.maximum_retained_artifacts = profile.bounds.maximum_samples + 1;
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.bounds.maximum_tables = 0;
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn premature_admission_and_missing_non_claims_are_rejected() {
        let mut profile = example_profile();
        profile.materialization.bundle_status = "materialized".to_string();
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.non_claims.remove(5);
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.non_claims[0] = "not-vm-replay-proof".to_string();
        profile.non_claims[5] = "not-assertion-readiness".to_string();
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn schema_revision_and_identifier_drift_are_rejected() {
        let mut profile = example_profile();
        profile.schema = "chaoscontrol.wasm-component-exploration-profile.v2".to_string();
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.materialization.mantle_revision = "NOT-A-REVISION".to_string();
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.profile_id = "Wasm Exploration".to_string();
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.runtime_cohort.wasmtime_fuel = 0;
        assert!(validate_profile(&profile).is_err());

        let mut profile = example_profile();
        profile.tool_cohort.waffle = Some(String::new());
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn nickel_contract_json_deserializes_into_this_dto() {
        let json = serde_json::json!({
            "schema": PROFILE_SCHEMA,
            "profile_id": "wasm-component-exploration-v1",
            "materialization": {
                "mantle_revision": "a141fcbaafe41f9a413a81275a33fe915bfca370",
                "bundle_profile_id": "wasm-component-exploration-closure-v1",
                "bundle_status": BUNDLE_STATUS_NOT_YET_MATERIALIZED,
            },
            "tool_cohort": {
                "suite_source": TOOL_SUITE_SOURCE,
                "wasm_tools_suite": WASM_TOOLS_SUITE,
                "generators": REQUIRED_GENERATORS,
                "mutators": REQUIRED_MUTATORS,
                "shrinkers": REQUIRED_SHRINKERS,
                "waffle": null,
            },
            "runtime_cohort": {
                "wasmtime_version": WASMTIME_VERSION,
                "strategies": REQUIRED_STRATEGIES,
                "proposals": RUNTIME_PROPOSALS,
                "component_features": COMPONENT_FEATURES,
                "host_authority": HOST_AUTHORITY_NONE,
                "epoch_deadline": EPOCH_DEADLINE_NONE,
                "wasmtime_fuel": 1_000_000u64,
            },
            "corpus_classes": REQUIRED_CORPUS_CLASSES,
            "outcome_classes": REQUIRED_OUTCOME_CLASSES,
            "observation_fields": REQUIRED_OBSERVATION_FIELDS,
            "bounds": serde_json::json!({
                "maximum_artifact_bytes": 16_777_216u64,
                "maximum_wit_bytes": 1_048_576u64,
                "maximum_imports": 4096u64,
                "maximum_exports": 4096u64,
                "maximum_functions": 4096u64,
                "maximum_types": 4096u64,
                "maximum_memories": 64u64,
                "maximum_tables": 64u64,
                "maximum_hostcall_bytes": 1_048_576u64,
                "maximum_generation_cases": 4096u64,
                "maximum_mutation_cases": 4096u64,
                "maximum_branches": 4096u64,
                "maximum_samples": 65_536u64,
                "maximum_shrink_attempts": 4096u64,
                "maximum_runtime_steps": 1_000_000_000u64,
                "maximum_concurrent_cases": 64u64,
                "maximum_retained_artifacts": 4096u64,
            }),
            "retention": "retain bounded exploration artifacts for the local evidence lane",
            "non_claims": REQUIRED_NON_CLAIMS,
        });
        let parsed: ExplorationProfile =
            serde_json::from_value(json).expect("Nickel example must deserialize");
        validate_profile(&parsed).expect("Nickel example must validate");
        assert_eq!(parsed, example_profile());
        assert!(is_blake3_hex(&profile_identity(&parsed).expect("identity")));
    }
}
