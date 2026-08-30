use chaoscontrol_sim_core::causality::{
    rank_candidates, AttributionObservation, CauseCandidate, CauseClass, DdminState,
    InterleavingStep,
};

const POLICY_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EVIDENCE_DIGEST: &str =
    "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const COMPLETE_BUDGET: u64 = 32;
const EXHAUSTED_BUDGET: u64 = 1;
const STEP_COUNT: usize = 4;

fn steps() -> Vec<InterleavingStep> {
    ["setup", "noise-a", "cause", "noise-b"]
        .into_iter()
        .enumerate()
        .map(|(sequence, step_id)| InterleavingStep {
            step_id: step_id.to_string(),
            sequence: u64::try_from(sequence).expect("sequence fits u64"),
            policy_blake3: POLICY_DIGEST.to_string(),
        })
        .collect()
}

fn candidates() -> Vec<CauseCandidate> {
    let mut candidates = vec![
        CauseCandidate {
            candidate_id: "declared-event".to_string(),
            class: CauseClass::DeclaredEvent,
            evidence_blake3: EVIDENCE_DIGEST.to_string(),
        },
        CauseCandidate {
            candidate_id: "fault-schedule".to_string(),
            class: CauseClass::FaultSchedule,
            evidence_blake3: EVIDENCE_DIGEST.to_string(),
        },
        CauseCandidate {
            candidate_id: "seed".to_string(),
            class: CauseClass::Seed,
            evidence_blake3: EVIDENCE_DIGEST.to_string(),
        },
        CauseCandidate {
            candidate_id: "variant-policy".to_string(),
            class: CauseClass::VariantPolicy,
            evidence_blake3: EVIDENCE_DIGEST.to_string(),
        },
    ];
    candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    candidates
}

#[test]
fn ddmin_reduces_to_the_only_reproducing_step() {
    let mut state = DdminState::new(steps(), COMPLETE_BUDGET).expect("state");
    while let Some(candidate) = state.next_candidate().expect("candidate") {
        let reproduced = candidate.steps.iter().any(|step| step.step_id == "cause");
        state
            .record_outcome(&candidate.candidate_blake3, reproduced)
            .expect("outcome");
    }
    let result = state.result();
    assert!(result.complete);
    assert!(!result.budget_exhausted);
    assert_eq!(result.minimized_steps.len(), 1);
    assert_eq!(result.minimized_steps[0].step_id, "cause");
}

#[test]
fn ddmin_budget_exhaustion_is_partial_and_non_mutating_on_identity_drift() {
    let original = steps();
    let mut state = DdminState::new(original.clone(), EXHAUSTED_BUDGET).expect("state");
    let candidate = state
        .next_candidate()
        .expect("candidate result")
        .expect("empty probe");
    assert!(state.record_outcome(POLICY_DIGEST, false).is_err());
    state
        .record_outcome(&candidate.candidate_blake3, false)
        .expect("matching outcome");
    assert!(state.next_candidate().expect("budget outcome").is_none());
    let result = state.result();
    assert!(result.budget_exhausted);
    assert!(!result.complete);
    assert_eq!(result.minimized_steps, original);
}

#[test]
fn attribution_ranks_the_discriminating_declared_event() {
    let candidates = candidates();
    let observations = candidates
        .iter()
        .map(|candidate| AttributionObservation {
            candidate_id: candidate.candidate_id.clone(),
            attempt: 0,
            neutralized_reproduced: candidate.class != CauseClass::DeclaredEvent,
        })
        .collect::<Vec<_>>();
    let report =
        rank_candidates(&candidates, &observations, COMPLETE_BUDGET).expect("attribution report");
    assert_eq!(report.probable_causes, vec!["declared-event"]);
    assert!(!report.partial);
    assert!(!report.equivalent_without_discriminating_cause);
}

#[test]
fn equivalent_candidates_do_not_invent_a_cause() {
    let candidates = candidates();
    let observations = candidates
        .iter()
        .map(|candidate| AttributionObservation {
            candidate_id: candidate.candidate_id.clone(),
            attempt: 0,
            neutralized_reproduced: true,
        })
        .collect::<Vec<_>>();
    let report =
        rank_candidates(&candidates, &observations, COMPLETE_BUDGET).expect("attribution report");
    assert!(report.probable_causes.is_empty());
    assert!(report.equivalent_without_discriminating_cause);
}

#[test]
fn malformed_attempt_order_and_budget_fail_closed() {
    let candidates = candidates();
    let invalid = vec![AttributionObservation {
        candidate_id: candidates[0].candidate_id.clone(),
        attempt: 1,
        neutralized_reproduced: false,
    }];
    assert!(rank_candidates(&candidates, &invalid, COMPLETE_BUDGET).is_err());
    assert!(rank_candidates(&candidates, &[], 0).is_err());
    assert_eq!(steps().len(), STEP_COUNT);
}
