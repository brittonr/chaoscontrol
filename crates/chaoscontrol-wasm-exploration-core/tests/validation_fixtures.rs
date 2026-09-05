//! Positive and negative validation fixtures for the Wasm component
//! exploration rail.
//!
//! r[chaoscontrol.wasm_component.validation] requires deterministic positive
//! and negative cases covering admission, cohort, materialization,
//! promotion, boundary classification, trap and divergence handling, corpus
//! replay, shrink predicates, bounds, and evidence claims.
//! r[chaoscontrol.wasm_component.materialization] requires unmaterialized
//! bundles to keep every case diagnostic-only and every promotion denied.
//! Every fixture here is a pure core decision, so the cases stay valid
//! before and after the harness shells land.

use chaoscontrol_wasm_exploration_core as exploration;

use exploration::profile::{
    BUNDLE_STATUS_NOT_YET_MATERIALIZED, COMPONENT_FEATURES, EPOCH_DEADLINE_NONE,
    HOST_AUTHORITY_NONE, REQUIRED_CORPUS_CLASSES, REQUIRED_GENERATORS, REQUIRED_MUTATORS,
    REQUIRED_NON_CLAIMS, REQUIRED_OBSERVATION_FIELDS, REQUIRED_OUTCOME_CLASSES, REQUIRED_SHRINKERS,
    REQUIRED_STRATEGIES, RUNTIME_PROPOSALS, TOOL_SUITE_SOURCE, WASMTIME_VERSION, WASM_TOOLS_SUITE,
};
use exploration::promotion::PromotionOutcome;
use exploration::{
    admit_promotion, admit_shrink_step, case_identity, check_claim, classify_boundary_rejection,
    compare_strategies, decide_bound, divergence_predicate, exploration_report, open_shrink_ledger,
    predicate_holds, profile_identity, proof_eligible, record_accepted_step, validate_case,
    validate_profile, verify_corpus_replay, AdmissionBoundary, BoundDecision, BoundSkip,
    CorpusClass, CorpusReplayVerdict, ExpectedValidity, ExplorationBounds, ExplorationCase,
    ExplorationProfile, GeneratorKind, Materialization, NormalizedObservation, OutcomeClass,
    PromotionDenial, RuntimeCohort, ShrinkRejection, Strategy, StrategyRun, ToolCohort,
    BOUNDED_EVIDENCE_CLAIM, DIAGNOSTIC_ONLY_CLAIM, PROFILE_SCHEMA,
};

/// The valid v1 profile exactly as the Nickel contract example pins it.
fn v1_profile() -> ExplorationProfile {
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
            generators: REQUIRED_GENERATORS.iter().map(|v| v.to_string()).collect(),
            mutators: REQUIRED_MUTATORS.iter().map(|v| v.to_string()).collect(),
            shrinkers: REQUIRED_SHRINKERS.iter().map(|v| v.to_string()).collect(),
            waffle: None,
        },
        runtime_cohort: RuntimeCohort {
            wasmtime_version: WASMTIME_VERSION.to_string(),
            strategies: REQUIRED_STRATEGIES.iter().map(|v| v.to_string()).collect(),
            proposals: RUNTIME_PROPOSALS.to_string(),
            component_features: COMPONENT_FEATURES.to_string(),
            host_authority: HOST_AUTHORITY_NONE.to_string(),
            epoch_deadline: EPOCH_DEADLINE_NONE.to_string(),
            wasmtime_fuel: 1_000_000,
        },
        corpus_classes: REQUIRED_CORPUS_CLASSES
            .iter()
            .map(|v| v.to_string())
            .collect(),
        outcome_classes: REQUIRED_OUTCOME_CLASSES
            .iter()
            .map(|v| v.to_string())
            .collect(),
        observation_fields: REQUIRED_OBSERVATION_FIELDS
            .iter()
            .map(|v| v.to_string())
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
        non_claims: REQUIRED_NON_CLAIMS.iter().map(|v| v.to_string()).collect(),
    }
}

fn fixture_case(
    case_id: &str,
    corpus_class: CorpusClass,
    generator: GeneratorKind,
) -> ExplorationCase {
    ExplorationCase {
        case_id: case_id.to_string(),
        corpus_class,
        generator,
        generator_version: format!("{} 1.245.1", generator.as_str()),
        seed_hex: "00112233445566778899aabbccddeeff".to_string(),
        configuration_blake3: exploration::blake3_hex(b"generation-configuration"),
        parent_case_id: None,
        componentization_blake3: Some(exploration::blake3_hex(b"wit-componentization-inputs")),
        expected_validity: ExpectedValidity::Valid,
        artifact_blake3: exploration::blake3_hex(b"generated-component-bytes"),
        artifact_size_bytes: 512,
        wit_blake3: Some(exploration::blake3_hex(b"wit-package-bytes")),
        wit_size_bytes: Some(256),
    }
}

fn generated_case() -> ExplorationCase {
    fixture_case(
        "component-case-0001",
        CorpusClass::GeneratedValid,
        GeneratorKind::WasmSmith,
    )
}

fn trapped_observation(trap_class: &str, result: &str) -> NormalizedObservation {
    NormalizedObservation {
        outcome: OutcomeClass::DeterministicTrap,
        trap_class: Some(trap_class.to_string()),
        resource_class: Some("fuel-retired".to_string()),
        result_values: vec![result.to_string()],
        preserves_output: Some("<true>".to_string()),
        hostcall_transcript: vec!["sqrt(4) -> 2".to_string()],
        state_identity_blake3: Some(exploration::blake3_hex(b"final-state")),
    }
}

fn run(strategy: Strategy, observation: NormalizedObservation) -> StrategyRun {
    StrategyRun {
        strategy,
        observation,
    }
}

fn diverging_comparison() -> exploration::StrategyComparison {
    let mut pulley = trapped_observation("wasm trap: unreachable", "1");
    pulley.result_values = vec!["2".to_string()];
    compare_strategies(
        "component-case-0001",
        &exploration::blake3_hex(b"case-identity"),
        run(
            Strategy::Cranelift,
            trapped_observation("wasm trap: unreachable", "1"),
        ),
        run(Strategy::Pulley, pulley),
    )
    .expect("comparison")
}

fn shrink_candidate(size: u64) -> exploration::ShrinkCandidate {
    exploration::ShrinkCandidate {
        candidate_blake3: exploration::blake3_hex(&size.to_le_bytes()),
        artifact_size_bytes: size,
    }
}

#[test]
fn positive_baseline_cases_admit_and_bind_the_bundle_identity() {
    let profile = v1_profile();
    validate_profile(&profile).expect("v1 profile must validate");

    let component = fixture_case(
        "baseline-component-0001",
        CorpusClass::BaselineComponent,
        GeneratorKind::WasmToolsComponent,
    );
    let wit_package = fixture_case(
        "baseline-wit-package-0001",
        CorpusClass::BaselineWitPackage,
        GeneratorKind::WitSmith,
    );
    validate_case(&component, &profile).expect("baseline component admits");
    validate_case(&wit_package, &profile).expect("baseline WIT package admits");
    assert_ne!(
        case_identity(&component).expect("identity"),
        case_identity(&wit_package).expect("identity")
    );

    let report = exploration_report(&profile, vec![], vec![], vec![]).expect("report");
    assert_eq!(
        report.profile_identity_blake3,
        profile_identity(&profile).expect("profile identity")
    );
}

#[test]
fn positive_valid_case_matches_the_v1_profile_but_stays_diagnostic() {
    let profile = v1_profile();
    let case = generated_case();
    validate_case(&case, &profile).expect("generated valid case admits");
    assert!(!proof_eligible(&profile));
    check_claim(DIAGNOSTIC_ONLY_CLAIM).expect("diagnostic-only claim is in vocabulary");
    check_claim(BOUNDED_EVIDENCE_CLAIM).expect("bounded evidence claim is in vocabulary");
}

#[test]
fn positive_corpus_replay_of_identical_regeneration_is_reproducible() {
    let declared = generated_case();
    let regenerated = declared.clone();
    match verify_corpus_replay(&declared, &regenerated).expect("verdict") {
        CorpusReplayVerdict::Reproducible {
            case_identity_blake3,
        } => {
            assert_eq!(
                case_identity_blake3,
                case_identity(&declared).expect("identity")
            );
        }
        CorpusReplayVerdict::ReplayMismatch { .. } => {
            panic!("identical regeneration must be reproducible");
        }
    }
}

#[test]
fn positive_strategy_match_receipt_binds_both_executions() {
    let comparison = compare_strategies(
        "component-case-0001",
        &exploration::blake3_hex(b"case-identity"),
        run(
            Strategy::Cranelift,
            trapped_observation("wasm trap: unreachable", "1"),
        ),
        run(
            Strategy::Pulley,
            trapped_observation("wasm trap: unreachable", "1"),
        ),
    )
    .expect("comparison");
    assert_eq!(comparison.outcome, OutcomeClass::StrategyMatch);
    assert!(comparison.first_difference.is_none());
    assert_eq!(comparison.left.strategy, Strategy::Cranelift);
    assert_eq!(comparison.right.strategy, Strategy::Pulley);
    assert_eq!(
        comparison.left.observation.trap_class,
        comparison.right.observation.trap_class
    );
}

#[test]
fn negative_incomplete_materialization_bundle_identity_is_rejected() {
    let mut profile = v1_profile();
    profile.materialization.mantle_revision = "a141fcba".to_string();
    let error = validate_profile(&profile).expect_err("truncated revision must be rejected");
    assert!(error.contains("40 lowercase hex"));

    let mut profile = v1_profile();
    profile.materialization.bundle_profile_id = "Not A Bundle Id".to_string();
    let error = validate_profile(&profile).expect_err("invalid bundle id must be rejected");
    assert!(error.contains("bounded identifier"));
}

#[test]
fn negative_cohort_mismatch_denies_cases_and_profiles() {
    let mut profile = v1_profile();
    profile.tool_cohort.generators = vec!["wasm-smith".to_string()];
    let case = fixture_case(
        "component-case-0001",
        CorpusClass::GeneratedValid,
        GeneratorKind::WitSmith,
    );
    let error = validate_case(&case, &profile).expect_err("unadmitted generator must be denied");
    assert!(error.contains("not admitted by the profile"));

    let mut profile = v1_profile();
    profile.runtime_cohort.strategies = vec!["cranelift".to_string()];
    let error = validate_profile(&profile).expect_err("strategy drift must be rejected");
    assert!(error.contains("must be exactly"));
}

#[test]
fn negative_unmaterialized_promotion_stays_exploration_only() {
    let profile = v1_profile();
    let identity = exploration::blake3_hex(b"case-identity");

    let decision = admit_promotion(&profile, &identity, Some(&identity));
    assert_eq!(
        decision.outcome,
        PromotionOutcome::Denied {
            denial: PromotionDenial::BundleNotMaterialized,
        }
    );

    let decision = admit_promotion(&profile, &identity, None);
    assert_eq!(
        decision.outcome,
        PromotionOutcome::Denied {
            denial: PromotionDenial::BundleNotMaterialized,
        }
    );
}

#[test]
fn negative_malformed_case_facts_fail_admission() {
    let profile = v1_profile();

    let mut case = generated_case();
    case.artifact_blake3 = "deadbeef".to_string();
    let error = validate_case(&case, &profile).expect_err("bad digest must be rejected");
    assert!(error.contains("BLAKE3"));

    let mut case = generated_case();
    case.seed_hex = "abc".to_string();
    assert!(validate_case(&case, &profile).is_err());

    let mut case = generated_case();
    case.artifact_size_bytes = 0;
    assert!(validate_case(&case, &profile).is_err());

    let mut case = generated_case();
    case.wit_blake3 = None;
    assert!(validate_case(&case, &profile).is_err());
}

#[test]
fn negative_boundary_rejections_keep_four_distinct_pre_execution_classes() {
    let classes = [
        classify_boundary_rejection(AdmissionBoundary::Static),
        classify_boundary_rejection(AdmissionBoundary::Profile),
        classify_boundary_rejection(AdmissionBoundary::Compile),
        classify_boundary_rejection(AdmissionBoundary::Link),
    ];
    for (index, class) in classes.iter().enumerate() {
        for other in &classes[index + 1..] {
            assert_ne!(class, other, "each boundary keeps its own class");
        }
        assert!(class.is_run_terminal());
        assert!(REQUIRED_OUTCOME_CLASSES.contains(&class.as_str()));
    }
}

#[test]
fn negative_trap_classifications_stay_normalized_and_divergences_name_the_field() {
    assert!(OutcomeClass::DeterministicTrap.is_run_terminal());

    let comparison = compare_strategies(
        "component-case-0001",
        &exploration::blake3_hex(b"case-identity"),
        run(
            Strategy::Cranelift,
            trapped_observation("wasm trap: unreachable", "1"),
        ),
        run(
            Strategy::Pulley,
            trapped_observation("wasm trap: call stack exhausted", "1"),
        ),
    )
    .expect("comparison");
    assert_eq!(comparison.outcome, OutcomeClass::StrategyDivergence);
    let difference = comparison
        .first_difference
        .as_ref()
        .expect("divergence must retain its first difference");
    assert_eq!(difference.field, "trap-class");

    let comparison = diverging_comparison();
    assert_eq!(comparison.outcome, OutcomeClass::StrategyDivergence);
    let difference = comparison
        .first_difference
        .as_ref()
        .expect("divergence must retain its first difference");
    assert_eq!(difference.field, "result-values");
}

#[test]
fn negative_replay_mismatch_is_classified_and_never_promotable() {
    let profile = v1_profile();
    let declared = generated_case();
    let mut regenerated = declared.clone();
    regenerated.artifact_blake3 = exploration::blake3_hex(b"regenerated-different-bytes");
    regenerated.artifact_size_bytes = 640;

    let verdict = verify_corpus_replay(&declared, &regenerated).expect("verdict");
    match &verdict {
        CorpusReplayVerdict::ReplayMismatch {
            declared_identity_blake3,
            regenerated_identity_blake3,
        } => {
            assert_ne!(declared_identity_blake3, regenerated_identity_blake3);
        }
        CorpusReplayVerdict::Reproducible { .. } => {
            panic!("divergent bytes must classify as replay mismatch");
        }
    }
    assert!(REQUIRED_OUTCOME_CLASSES.contains(&OutcomeClass::ReplayMismatch.as_str()));

    let regenerated_identity = case_identity(&regenerated).expect("identity");
    let decision = admit_promotion(&profile, &regenerated_identity, Some(&regenerated_identity));
    assert_eq!(
        decision.outcome,
        PromotionOutcome::Denied {
            denial: PromotionDenial::BundleNotMaterialized,
        }
    );
}

#[test]
fn negative_shrink_class_drift_rejects_the_step_and_keeps_the_ledger() {
    let comparison = diverging_comparison();
    let predicate = divergence_predicate(&comparison).expect("predicate");
    let mut ledger = open_shrink_ledger(
        "component-case-0001",
        predicate.clone(),
        shrink_candidate(100),
    );

    let mut drifted = diverging_comparison();
    drifted.outcome = OutcomeClass::StrategyMatch;
    assert!(!predicate_holds(&predicate, &drifted));
    let error = admit_shrink_step(
        &predicate,
        &ledger.original_candidate,
        &shrink_candidate(90),
        &drifted,
        ledger.accepted_steps.len(),
        4096,
    )
    .expect_err("class drift must be rejected");
    assert_eq!(error, ShrinkRejection::PredicateChanged);
    assert_eq!(error.outcome_class(), OutcomeClass::ShrinkComplete);
    assert!(ledger.accepted_steps.is_empty());
    assert_eq!(ledger.final_candidate.artifact_size_bytes, 100);

    let step = admit_shrink_step(
        &predicate,
        &ledger.original_candidate,
        &shrink_candidate(90),
        &comparison,
        ledger.accepted_steps.len(),
        4096,
    )
    .expect("unchanged predicate admits the smaller candidate");
    record_accepted_step(&mut ledger, step);
    assert_eq!(ledger.accepted_steps.len(), 1);
    assert_eq!(ledger.final_candidate.artifact_size_bytes, 90);
}

#[test]
fn negative_bound_skips_name_the_field_without_raising_limits() {
    let profile = v1_profile();

    match decide_bound("maximum_samples", 10, 11) {
        BoundDecision::Skipped { skip } => {
            assert_eq!(
                skip,
                BoundSkip {
                    field: "maximum_samples".to_string(),
                    limit: 10,
                    actual: 11,
                }
            );
        }
        BoundDecision::Admitted => panic!("beyond the bound must skip"),
    }

    let mut case = generated_case();
    case.artifact_size_bytes = profile.bounds.maximum_artifact_bytes + 1;
    let skip = match exploration::check_artifact_bytes(&profile, case.artifact_size_bytes) {
        BoundDecision::Skipped { skip } => skip,
        BoundDecision::Admitted => panic!("beyond the artifact bound must skip"),
    };
    assert!(validate_case(&case, &profile).is_err());

    let with_skip = exploration_report(&profile, vec![], vec![], vec![skip]).expect("report");
    assert_eq!(with_skip.bound_skips.len(), 1);
    assert_eq!(with_skip.bound_skips[0].field, "maximum_artifact_bytes");
    let without_skip = exploration_report(&profile, vec![], vec![], vec![]).expect("report");
    assert_ne!(
        with_skip.report_identity_blake3,
        without_skip.report_identity_blake3
    );
}

#[test]
fn negative_evidence_overclaims_are_rejected_before_emission() {
    for overclaim in [
        "proves-vm-replay",
        "universal-wasm-correctness",
        "sandbox-proof",
        "release-eligible",
        "assertion-ready",
        "semantically-equivalent",
        "package-trusted",
    ] {
        let error = check_claim(overclaim).expect_err("overclaim must be rejected");
        assert!(error.contains("bounded exploration evidence vocabulary"));
    }
    for claim in REQUIRED_NON_CLAIMS {
        check_claim(claim).expect("non-claims stay allowed");
    }

    let profile = v1_profile();
    let report =
        exploration_report(&profile, vec![diverging_comparison()], vec![], vec![]).expect("report");
    assert_eq!(
        report.non_claims,
        REQUIRED_NON_CLAIMS
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
    );
    assert!(!report.proof_eligible);
}
