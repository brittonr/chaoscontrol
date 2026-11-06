use crate::engine::{EngineConfig, FaultEngine};
use crate::evidence::*;
use crate::faults::{Fault, FaultVariant};
use crate::mechanisms::{
    CustomFaultEffect, MechanismError, NetworkDirection, NetworkDisruption, PauseControlRequest,
};
use crate::outcomes::*;
use crate::schedule::FaultScheduleBuilder;
use std::collections::BTreeMap;

const SEED: u64 = 42;
const PROFILE: &str = "fault-ledger-fixture";

fn selected(ledger: &mut FaultOutcomeLedger, fault: Fault, tick: u64) -> FaultAttemptId {
    let schedule_id = FaultScheduleId([0; 32]);
    let attempt = FaultAttempt::new(
        fault_run_id(SEED, 1, schedule_id),
        1,
        schedule_id,
        ledger.attempts.len() as u64,
        tick * NANOSECONDS_PER_SIMULATION_TICK,
        fault,
    );
    *ledger =
        transition_fault_outcome(ledger, Some(&attempt), attempt.id, FaultStageKind::Selected)
            .unwrap();
    attempt.id
}

fn stage(ledger: &mut FaultOutcomeLedger, id: FaultAttemptId, kind: FaultStageKind) {
    *ledger = transition_fault_outcome(ledger, None, id, kind).unwrap();
}

fn applied(ledger: &mut FaultOutcomeLedger, id: FaultAttemptId, effect: FaultPlanEffect) {
    stage(
        ledger,
        id,
        FaultStageKind::Applicable {
            effect: effect.clone(),
        },
    );
    stage(ledger, id, FaultStageKind::Applied { effect });
}

#[test]
fn receipt_preserves_every_stage_without_promoting_selection_or_application() {
    let mut ledger = FaultOutcomeLedger::default();
    let pending = selected(
        &mut ledger,
        Fault::CustomEffect {
            effect_id: "fixture.custom".into(),
            version: 1,
            parameters: BTreeMap::new(),
        },
        0,
    );
    let custom = selected(
        &mut ledger,
        Fault::CustomEffect {
            effect_id: "fixture.custom".into(),
            version: 1,
            parameters: BTreeMap::new(),
        },
        0,
    );
    applied(
        &mut ledger,
        custom,
        FaultPlanEffect::CustomEffect {
            effect: CustomFaultEffect {
                effect_id: "fixture.custom".into(),
                version: 1,
                parameters: BTreeMap::new(),
            },
        },
    );
    let throttle = selected(
        &mut ledger,
        Fault::CpuThrottle {
            target: 0,
            budget_units: 1,
            duration_ticks: 5,
        },
        0,
    );
    applied(
        &mut ledger,
        throttle,
        FaultPlanEffect::CpuThrottle {
            target: 0,
            budget_units: 1,
            release_at_tick: 5,
        },
    );
    stage(
        &mut ledger,
        throttle,
        FaultStageKind::Observed {
            observation: FaultObservation::new(
                throttle,
                FaultObservationSubsystem::Cpu,
                0,
                FaultObservationEffect::CpuThrottleActivated,
            ),
        },
    );
    let rejected = selected(
        &mut ledger,
        Fault::CpuThrottle {
            target: 0,
            budget_units: 0,
            duration_ticks: 5,
        },
        0,
    );
    stage(
        &mut ledger,
        rejected,
        FaultStageKind::Rejected {
            reason: FaultRejectionReason::InvalidThrottleBudget {
                budget_units: 0,
                max: 10,
            },
        },
    );
    let failed = selected(
        &mut ledger,
        Fault::CpuThrottle {
            target: 0,
            budget_units: 2,
            duration_ticks: 5,
        },
        0,
    );
    stage(
        &mut ledger,
        failed,
        FaultStageKind::Applicable {
            effect: FaultPlanEffect::CpuThrottle {
                target: 0,
                budget_units: 2,
                release_at_tick: 5,
            },
        },
    );
    stage(
        &mut ledger,
        failed,
        FaultStageKind::ApplicationFailed {
            reason: FaultApplicationFailureReason::ActiveEffectOverlap,
            disposition: FaultApplicationFailureDisposition::RolledBack,
        },
    );
    let receipt = fault_mechanism_receipt(&ledger, SEED, PROFILE).unwrap();
    assert_eq!(
        receipt.counts,
        FaultStageCounters {
            selected: 5,
            applicable: 3,
            applied: 2,
            observed: 1,
            rejected: 1,
            application_failed: 1,
        }
    );
    let pending = receipt
        .attempts
        .iter()
        .find(|entry| entry.attempt.id == pending)
        .unwrap();
    assert_eq!(pending.counts.selected, 1);
    assert_eq!(pending.counts.applied, 0);
    assert_eq!(pending.counts.observed, 0);
    assert!(pending.plan.is_none());
    let custom = receipt
        .attempts
        .iter()
        .find(|entry| entry.attempt.id == custom)
        .unwrap();
    assert_eq!(custom.counts.applied, 1);
    assert_eq!(custom.counts.observed, 0);
    assert!(
        matches!(&custom.effective_target, FaultEffectiveTarget::CustomEffect { effect_id, version: 1 } if effect_id == "fixture.custom")
    );
    let overlap = receipt
        .target_decisions
        .iter()
        .find(|entry| entry.attempt_id == failed)
        .unwrap();
    assert_eq!(overlap.effective_attempt, Some(throttle));
    assert_eq!(overlap.stage, FaultAuthoritativeStage::ApplicationFailed);
    validate_fault_mechanism_receipt(&ledger, SEED, PROFILE, &receipt).unwrap();
}

fn directional(
    ledger: &mut FaultOutcomeLedger,
    node: usize,
    peer: usize,
    mode: NetworkDisruption,
) -> FaultAttemptId {
    let delay_ns = u64::from(mode == NetworkDisruption::Slowed) * NANOSECONDS_PER_SIMULATION_TICK;
    let queue_limit = u32::from(mode == NetworkDisruption::Jammed);
    let id = selected(
        ledger,
        Fault::NetworkDirectional {
            node,
            peer,
            direction: NetworkDirection::Outbound,
            disruption: mode,
            delay_ns,
            queue_limit,
        },
        0,
    );
    applied(
        ledger,
        id,
        FaultPlanEffect::NetworkDirectional {
            node: node as u32,
            peer: peer as u32,
            direction: NetworkDirection::Outbound,
            disruption: mode,
            delay_ticks: delay_ns / NANOSECONDS_PER_SIMULATION_TICK,
            queue_limit,
        },
    );
    id
}

#[test]
fn receipt_retains_directional_candidates_precedence_and_scoped_heal() {
    let mut ledger = FaultOutcomeLedger::default();
    let slowed = directional(&mut ledger, 0, 1, NetworkDisruption::Slowed);
    let stopped = directional(&mut ledger, 0, 1, NetworkDisruption::Stopped);
    let other = directional(&mut ledger, 1, 2, NetworkDisruption::Jammed);
    let heal = selected(
        &mut ledger,
        Fault::NetworkDirectionalHeal { node: 0, peer: 1 },
        0,
    );
    applied(
        &mut ledger,
        heal,
        FaultPlanEffect::NetworkDirectionalHeal { node: 0, peer: 1 },
    );
    let receipt = fault_mechanism_receipt(&ledger, SEED, PROFILE).unwrap();
    let overlap = receipt
        .overlap_decisions
        .iter()
        .find(|entry| entry.candidates.len() == 2)
        .unwrap();
    assert_eq!(overlap.candidates, vec![slowed, stopped]);
    assert_eq!(overlap.effective_attempt, Some(stopped));
    assert_eq!(
        receipt
            .overlap_decisions
            .iter()
            .find(|entry| entry.from == 1 && entry.to == 2)
            .unwrap()
            .effective_attempt,
        Some(other)
    );
    assert!(receipt
        .overlap_decisions
        .iter()
        .rev()
        .take(2)
        .all(|entry| entry.effective_attempt.is_none()));
    assert_eq!(receipt.attempts.len(), 4);
    let encoded = serde_json::to_vec(&receipt).unwrap();
    let decoded = serde_json::from_slice(&encoded).unwrap();
    validate_fault_mechanism_receipt(&ledger, SEED, PROFILE, &decoded).unwrap();
}

#[test]
fn receipt_rejects_identity_target_count_precedence_and_stage_drift() {
    let mut ledger = FaultOutcomeLedger::default();
    directional(&mut ledger, 0, 1, NetworkDisruption::Slowed);
    directional(&mut ledger, 0, 1, NetworkDisruption::Stopped);
    let receipt = fault_mechanism_receipt(&ledger, SEED, PROFILE).unwrap();
    let mut overclaim = serde_json::to_value(&receipt).unwrap();
    overclaim["claim"] = serde_json::json!("complete fault coverage and release eligibility");
    assert!(serde_json::from_value::<FaultMechanismReceipt>(overclaim).is_err());
    let mut wrong = receipt.clone();
    wrong.attempts[0].effective_target = FaultEffectiveTarget::Vm { vm: 99 };
    assert_eq!(
        validate_fault_mechanism_receipt(&ledger, SEED, PROFILE, &wrong),
        Err(FaultTransitionError::ReceiptMismatch)
    );
    let mut wrong = receipt.clone();
    wrong.mechanisms[0].counts.observed = 1;
    assert!(validate_fault_mechanism_receipt(&ledger, SEED, PROFILE, &wrong).is_err());
    let mut wrong = receipt.clone();
    wrong
        .overlap_decisions
        .last_mut()
        .unwrap()
        .effective_attempt = None;
    assert!(validate_fault_mechanism_receipt(&ledger, SEED, PROFILE, &wrong).is_err());
    let mut wrong = receipt.clone();
    wrong.profile = "another-profile".into();
    assert!(validate_fault_mechanism_receipt(&ledger, SEED, PROFILE, &wrong).is_err());
    assert!(validate_fault_mechanism_receipt(&ledger, SEED + 1, PROFILE, &receipt).is_err());
    assert!(fault_mechanism_receipt(&ledger, SEED, "").is_err());
    ledger.counters.applicable += 1;
    assert_eq!(
        validate_fault_outcome_ledger(&ledger),
        Err(FaultTransitionError::LedgerReplayMismatch)
    );
}

fn paused_engine(due_tick: u64) -> FaultEngine {
    let mut engine = FaultEngine::new(EngineConfig {
        seed: SEED,
        schedule: Some(
            FaultScheduleBuilder::new()
                .at_ns(
                    due_tick * NANOSECONDS_PER_SIMULATION_TICK,
                    Fault::CpuThrottle {
                        target: 0,
                        budget_units: 1,
                        duration_ticks: 5,
                    },
                )
                .build(),
        ),
        ..EngineConfig::default()
    });
    engine.begin_run();
    engine.force_setup_complete();
    engine
        .request_pause_control(
            PauseControlRequest {
                requester: "liveness".into(),
                start_tick: 0,
                end_tick: 5,
            },
            0,
        )
        .unwrap();
    engine
}

#[test]
fn suppression_counts_only_due_opportunities_and_preserves_pending_work() {
    let mut engine = paused_engine(1);
    assert!(engine.poll_fault_attempts(0).unwrap().is_empty());
    assert_eq!(
        engine.fault_outcomes().control_events.len(),
        1,
        "empty poll is not a selection"
    );
    for tick in [1, 2] {
        assert!(engine
            .poll_fault_attempts(tick * NANOSECONDS_PER_SIMULATION_TICK)
            .unwrap()
            .is_empty());
    }
    let receipt = fault_mechanism_receipt(engine.fault_outcomes(), SEED, PROFILE).unwrap();
    assert_eq!(receipt.counts.selected, 0);
    assert_eq!(receipt.counts.applied, 0);
    assert_eq!(receipt.mechanisms.len(), 1);
    assert_eq!(receipt.mechanisms[0].mechanism, FaultVariant::CpuThrottle);
    assert_eq!(receipt.mechanisms[0].selection_suppression_opportunities, 2);
    assert_eq!(receipt.mechanisms[0].counts.selected, 0);
    let snapshot = engine.snapshot();
    let expected = engine
        .poll_fault_attempts(5 * NANOSECONDS_PER_SIMULATION_TICK)
        .unwrap();
    // This fixture has no admitted assertion catalog; it may restore orchestration,
    // but must not acquire assertion-authoritative snapshot status.
    assert_eq!(
        engine.restore(&snapshot),
        Err(FaultTransitionError::SnapshotAssertionIdentityMismatch)
    );
    engine.restore_orchestration(&snapshot).unwrap();
    assert_eq!(
        engine
            .poll_fault_attempts(5 * NANOSECONDS_PER_SIMULATION_TICK)
            .unwrap(),
        expected
    );
    assert_eq!(expected.len(), 1);
}

#[test]
fn suppression_snapshot_rejects_due_entry_mechanism_and_window_drift() {
    let mut engine = paused_engine(1);
    engine
        .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
        .unwrap();
    let snapshot = engine.snapshot();
    engine.validate_orchestration_snapshot(&snapshot).unwrap();
    for mutation in 0..3 {
        let mut wrong = snapshot.clone();
        let mut ledger = snapshot.outcomes().clone();
        match &mut ledger.control_events[1].kind {
            FaultControlKind::SelectionSuppressed {
                scheduled_entry_indices,
                scheduled_mechanisms,
                tick,
                ..
            } => match mutation {
                0 => scheduled_entry_indices[0] = 99,
                1 => scheduled_mechanisms[0] = FaultVariant::ProcessKill,
                _ => *tick = 5,
            },
            _ => panic!("expected suppression"),
        }
        wrong.replace_outcomes_for_validation_test(ledger);
        assert_eq!(
            engine.restore_orchestration(&wrong),
            Err(FaultTransitionError::ControlEventMismatch)
        );
        assert_eq!(engine.fault_outcomes(), snapshot.outcomes());
    }
}

#[test]
fn selected_application_is_suppressed_with_no_application_or_observation() {
    let mut engine = FaultEngine::new(EngineConfig {
        seed: SEED,
        schedule: Some(
            FaultScheduleBuilder::new()
                .at_ns(0, Fault::ProcessKill { target: 0 })
                .build(),
        ),
        ..EngineConfig::default()
    });
    engine.begin_run();
    engine.force_setup_complete();
    let attempt = engine.poll_fault_attempts(0).unwrap().remove(0);
    engine
        .request_pause_control(
            PauseControlRequest {
                requester: "operator".into(),
                start_tick: 0,
                end_tick: 5,
            },
            0,
        )
        .unwrap();
    assert!(engine.suppress_fault_application(attempt.id, 0).unwrap());
    let receipt = fault_mechanism_receipt(engine.fault_outcomes(), SEED, PROFILE).unwrap();
    assert_eq!(receipt.counts.selected, 1);
    assert_eq!(receipt.counts.rejected, 1);
    assert_eq!(receipt.counts.applicable, 0);
    assert_eq!(receipt.counts.applied, 0);
    assert_eq!(receipt.counts.observed, 0);
    assert_eq!(receipt.mechanisms[0].application_suppressions, 1);
    let mut missing = engine.fault_outcomes().clone();
    missing.control_events.pop();
    assert!(validate_fault_outcome_ledger(&missing).is_err());
}

#[test]
fn rejected_pause_requests_are_bounded_and_replay_checked() {
    let mut engine = paused_engine(1);
    let duplicate = PauseControlRequest {
        requester: "another-operator".into(),
        start_tick: 1,
        end_tick: 3,
    };
    assert_eq!(
        engine.request_pause_control(duplicate, 0),
        Err(MechanismError::DuplicatePauseRequest)
    );
    let oversized = PauseControlRequest {
        requester: "x".repeat(crate::mechanisms::MAX_CUSTOM_STRING_BYTES + 1),
        start_tick: 6,
        end_tick: 9,
    };
    assert_eq!(
        engine.request_pause_control(oversized, 0),
        Err(MechanismError::InvalidRequester)
    );
    let receipt = fault_mechanism_receipt(engine.fault_outcomes(), SEED, PROFILE).unwrap();
    assert_eq!(receipt.control_events.len(), 3);
    assert!(
        matches!(&receipt.control_events[2].kind, FaultControlKind::PauseRequested { request, decision: Err(MechanismError::InvalidRequester), .. }
        if request.requester.is_empty() && request.requester_bytes > crate::mechanisms::MAX_CUSTOM_STRING_BYTES as u64)
    );
    let mut drift = engine.fault_outcomes().clone();
    if let FaultControlKind::PauseRequested { decision, .. } = &mut drift.control_events[1].kind {
        *decision = Ok(());
    }
    assert!(validate_fault_outcome_ledger(&drift).is_err());
}

#[test]
fn pause_records_active_windows_without_rewinding_or_inventing_custom_lifetimes() {
    let mut ledger = FaultOutcomeLedger::default();
    let throttle = selected(
        &mut ledger,
        Fault::CpuThrottle {
            target: 0,
            budget_units: 1,
            duration_ticks: 5,
        },
        0,
    );
    applied(
        &mut ledger,
        throttle,
        FaultPlanEffect::CpuThrottle {
            target: 0,
            budget_units: 1,
            release_at_tick: 5,
        },
    );
    let directional = directional(&mut ledger, 0, 1, NetworkDisruption::Slowed);
    let custom = selected(
        &mut ledger,
        Fault::CustomEffect {
            effect_id: "fixture.custom".into(),
            version: 1,
            parameters: BTreeMap::new(),
        },
        0,
    );
    applied(
        &mut ledger,
        custom,
        FaultPlanEffect::CustomEffect {
            effect: CustomFaultEffect {
                effect_id: "fixture.custom".into(),
                version: 1,
                parameters: BTreeMap::new(),
            },
        },
    );
    let attempt = &ledger.attempts[&throttle].attempt;
    let (run_id, run_sequence, schedule_id) =
        (attempt.run_id, attempt.run_sequence, attempt.schedule_id);
    let request = PauseControlRequest {
        requester: "liveness".into(),
        start_tick: 1,
        end_tick: 3,
    };
    let active = active_effects(&ledger, run_id, 1);
    assert!(active.contains(&throttle));
    assert!(active.contains(&directional));
    assert!(
        !active.contains(&custom),
        "opaque adapter lifetime is not an active-window claim"
    );
    let applied_attempts = applied_attempts(&ledger);
    assert!(applied_attempts.contains(&custom));
    record_fault_control(
        &mut ledger,
        run_id,
        run_sequence,
        schedule_id,
        1,
        FaultControlKind::PauseRequested {
            request: PauseRequestEvidence::new(&request),
            decision: Ok(()),
            applied_attempts,
            active_effects: active,
        },
    )
    .unwrap();
    validate_fault_outcome_ledger(&ledger).unwrap();
    assert!(
        active_effects(&ledger, run_id, 3).contains(&throttle),
        "pause end does not release the throttle"
    );
    assert!(!active_effects(&ledger, run_id, 5).contains(&throttle));
    assert!(active_effects(&ledger, run_id, 5).contains(&directional));
    let mut tampered = ledger.clone();
    if let FaultControlKind::PauseRequested { active_effects, .. } =
        &mut tampered.control_events[0].kind
    {
        active_effects.clear();
    }
    assert!(validate_fault_outcome_ledger(&tampered).is_err());
}

#[test]
fn receipt_refuses_over_bound_selected_payloads_instead_of_copying_them() {
    let mut ledger = FaultOutcomeLedger::default();
    let id = selected(
        &mut ledger,
        Fault::CustomEffect {
            effect_id: "fixture.custom".into(),
            version: 1,
            parameters: BTreeMap::from([(
                "payload".into(),
                "x".repeat(crate::mechanisms::MAX_CUSTOM_STRING_BYTES + 1),
            )]),
        },
        0,
    );
    stage(
        &mut ledger,
        id,
        FaultStageKind::Rejected {
            reason: FaultRejectionReason::InvalidCustomEffect,
        },
    );
    assert_eq!(
        fault_mechanism_receipt(&ledger, SEED, PROFILE),
        Err(FaultTransitionError::ReceiptPayloadBoundExceeded)
    );
    validate_fault_outcome_ledger(&ledger).unwrap();
}
