use chaoscontrol_evidence::{
    bind_causality_request, read_causality_receipt_path, read_causality_request_path,
    run_causality_analysis, validate_causality_receipt, CausalityExecutor, CausalityRequest,
    EvidenceError, EvidenceResult, ExecutionBinding,
};
use std::os::unix::fs::symlink;

use chaoscontrol_sim_core::causality::{
    AnalysisBudget, CauseCandidate, CauseClass, InterleavingStep, MinimizationCandidate,
};

const REPLAY_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SNAPSHOT_DIGEST: &str =
    "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const POLICY_DIGEST: &str =
    "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const EVIDENCE_DIGEST: &str =
    "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const FULL_MINIMIZATION_BUDGET: u64 = 32;
const FULL_ATTRIBUTION_BUDGET: u64 = 8;
const PARTIAL_BUDGET: u64 = 1;

fn request(budget: AnalysisBudget) -> CausalityRequest {
    let steps = ["setup", "cause", "noise"]
        .into_iter()
        .enumerate()
        .map(|(sequence, step_id)| InterleavingStep {
            step_id: step_id.to_string(),
            sequence: u64::try_from(sequence).expect("sequence fits u64"),
            policy_blake3: POLICY_DIGEST.to_string(),
        })
        .collect();
    let candidates = vec![
        CauseCandidate {
            candidate_id: "variant-policy".to_string(),
            class: CauseClass::VariantPolicy,
            evidence_blake3: EVIDENCE_DIGEST.to_string(),
        },
        CauseCandidate {
            candidate_id: "declared-event".to_string(),
            class: CauseClass::DeclaredEvent,
            evidence_blake3: EVIDENCE_DIGEST.to_string(),
        },
    ];
    bind_causality_request(
        REPLAY_DIGEST,
        vec![SNAPSHOT_DIGEST.to_string()],
        steps,
        candidates,
        budget,
    )
    .expect("request binds")
}

struct FixtureExecutor {
    drift_identity: bool,
    fail_execution: bool,
}

impl FixtureExecutor {
    fn binding(&self, reproduced: bool) -> EvidenceResult<ExecutionBinding> {
        if self.fail_execution {
            return Err(EvidenceError::new("fixture executor failure"));
        }
        Ok(ExecutionBinding {
            replay_verdict_blake3: if self.drift_identity {
                POLICY_DIGEST.to_string()
            } else {
                REPLAY_DIGEST.to_string()
            },
            snapshot_blake3s: vec![SNAPSHOT_DIGEST.to_string()],
            reproduced,
        })
    }
}

impl CausalityExecutor for FixtureExecutor {
    fn execute_interleaving(
        &mut self,
        candidate: &MinimizationCandidate,
    ) -> EvidenceResult<ExecutionBinding> {
        self.binding(candidate.steps.iter().any(|step| step.step_id == "cause"))
    }

    fn execute_neutralization(
        &mut self,
        candidate: &CauseCandidate,
    ) -> EvidenceResult<ExecutionBinding> {
        self.binding(candidate.class != CauseClass::DeclaredEvent)
    }
}

#[test]
fn shell_drives_minimization_and_attribution_with_bound_receipt() {
    let request = request(AnalysisBudget {
        minimization_executions: FULL_MINIMIZATION_BUDGET,
        attribution_executions: FULL_ATTRIBUTION_BUDGET,
    });
    let mut executor = FixtureExecutor {
        drift_identity: false,
        fail_execution: false,
    };
    let receipt = run_causality_analysis(&request, &mut executor).expect("analysis receipt");
    assert!(receipt.minimization.complete);
    assert_eq!(receipt.minimization.minimized_steps.len(), 1);
    assert_eq!(receipt.minimization.minimized_steps[0].step_id, "cause");
    assert_eq!(receipt.attribution.probable_causes, vec!["declared-event"]);
    validate_causality_receipt(&request, &receipt).expect("receipt validates");

    let directory = tempfile::tempdir().expect("temporary directory");
    let request_path = directory.path().join("request.json");
    let receipt_path = directory.path().join("receipt.json");
    let request_link = directory.path().join("request-link.json");
    std::fs::write(
        &request_path,
        serde_json::to_vec_pretty(&request).expect("request serializes"),
    )
    .expect("request writes");
    std::fs::write(
        &receipt_path,
        serde_json::to_vec_pretty(&receipt).expect("receipt serializes"),
    )
    .expect("receipt writes");
    let loaded_request = read_causality_request_path(&request_path).expect("request reads");
    read_causality_receipt_path(&loaded_request, &receipt_path).expect("receipt reads");
    symlink(&request_path, &request_link).expect("request symlink");
    assert!(read_causality_request_path(&request_link).is_err());
}

#[test]
fn budget_exhaustion_produces_partial_results() {
    let request = request(AnalysisBudget {
        minimization_executions: PARTIAL_BUDGET,
        attribution_executions: PARTIAL_BUDGET,
    });
    let mut executor = FixtureExecutor {
        drift_identity: false,
        fail_execution: false,
    };
    let receipt = run_causality_analysis(&request, &mut executor).expect("partial receipt");
    assert!(receipt.minimization.budget_exhausted);
    assert!(!receipt.minimization.complete);
    assert!(receipt.attribution.partial);
}

#[test]
fn shell_rejects_identity_drift_executor_failure_and_receipt_tamper() {
    let request = request(AnalysisBudget {
        minimization_executions: FULL_MINIMIZATION_BUDGET,
        attribution_executions: FULL_ATTRIBUTION_BUDGET,
    });
    let mut drifted = FixtureExecutor {
        drift_identity: true,
        fail_execution: false,
    };
    assert!(run_causality_analysis(&request, &mut drifted).is_err());

    let mut failed = FixtureExecutor {
        drift_identity: false,
        fail_execution: true,
    };
    assert!(run_causality_analysis(&request, &mut failed).is_err());

    let mut valid = FixtureExecutor {
        drift_identity: false,
        fail_execution: false,
    };
    let mut receipt = run_causality_analysis(&request, &mut valid).expect("receipt");
    receipt.snapshot_blake3s[0] = POLICY_DIGEST.to_string();
    assert!(validate_causality_receipt(&request, &receipt).is_err());
}
