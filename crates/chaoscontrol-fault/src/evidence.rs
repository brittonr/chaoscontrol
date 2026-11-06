//! Bounded, replay-checked fault control facts and receipt projections.

use crate::faults::{Fault, FaultVariant};
use crate::mechanisms::{
    admit_pause_requests, MechanismError, NetworkDirection, NetworkDisruption, PauseControlRequest,
    MAX_CUSTOM_STRING_BYTES,
};
use crate::outcomes::{
    validate_fault_outcome_ledger, FaultAttempt, FaultAttemptId, FaultAuthoritativeStage,
    FaultOutcomeLedger, FaultPlanEffect, FaultRunId, FaultScheduleId, FaultStageCounters,
    FaultStageKind, FaultTransitionError, MAX_FAULT_ATTEMPTS, MAX_FAULT_OUTCOME_EVENTS,
};
use serde::{Deserialize, Serialize};

pub const MAX_FAULT_CONTROL_EVENTS: usize = MAX_FAULT_OUTCOME_EVENTS;

/// Oversized requester text is represented by its length and digest, never retained raw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseRequestEvidence {
    pub requester: String,
    pub requester_bytes: u64,
    pub requester_digest: [u8; 32],
    pub start_tick: u64,
    pub end_tick: u64,
}

impl PauseRequestEvidence {
    pub fn new(request: &PauseControlRequest) -> Self {
        Self {
            requester: if request.requester.len() <= MAX_CUSTOM_STRING_BYTES {
                request.requester.clone()
            } else {
                String::new()
            },
            requester_bytes: request.requester.len() as u64,
            requester_digest: *blake3::hash(request.requester.as_bytes()).as_bytes(),
            start_tick: request.start_tick,
            end_tick: request.end_tick,
        }
    }

    fn request(&self) -> Result<PauseControlRequest, MechanismError> {
        if self.requester_bytes > MAX_CUSTOM_STRING_BYTES as u64 {
            return Err(MechanismError::InvalidRequester);
        }
        Ok(PauseControlRequest {
            requester: self.requester.clone(),
            start_tick: self.start_tick,
            end_tick: self.end_tick,
        })
    }

    fn valid_identity(&self) -> bool {
        if self.requester_bytes > MAX_CUSTOM_STRING_BYTES as u64 {
            self.requester.is_empty()
        } else {
            self.requester_bytes == self.requester.len() as u64
                && self.requester_digest == *blake3::hash(self.requester.as_bytes()).as_bytes()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultControlEvent {
    pub run_id: FaultRunId,
    pub run_sequence: u64,
    pub schedule_id: FaultScheduleId,
    pub tick: u64,
    /// Number of stage events committed before this fact, not a stage or application count.
    pub stage_event_count: u64,
    pub kind: FaultControlKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FaultControlKind {
    PauseRequested {
        request: PauseRequestEvidence,
        decision: Result<(), MechanismError>,
        /// Applied attempts at the request boundary; not a claim they are still affecting work.
        applied_attempts: Vec<FaultAttemptId>,
        /// Core-managed windows still active at this tick; custom adapter lifetime is not inferred.
        active_effects: Vec<FaultAttemptId>,
    },
    SelectionSuppressed {
        tick: u64,
        time_ns: u64,
        request_index: u32,
        scheduled_entry_indices: Vec<u64>,
        scheduled_mechanisms: Vec<FaultVariant>,
        random_due: bool,
    },
    ApplicationSuppressed {
        tick: u64,
        request_index: u32,
        attempt_id: FaultAttemptId,
    },
}

pub fn applied_attempts(ledger: &FaultOutcomeLedger) -> Vec<FaultAttemptId> {
    ledger
        .attempts
        .iter()
        .filter_map(|(id, state)| {
            matches!(
                state.stage,
                FaultAuthoritativeStage::Applied | FaultAuthoritativeStage::Observed
            )
            .then_some(*id)
        })
        .collect()
}

/// Append a bounded fact. Admission validates its meaning when the ledger is replayed.
pub fn record_fault_control(
    ledger: &mut FaultOutcomeLedger,
    run_id: FaultRunId,
    run_sequence: u64,
    schedule_id: FaultScheduleId,
    tick: u64,
    kind: FaultControlKind,
) -> Result<(), FaultTransitionError> {
    if ledger.control_events.len() >= MAX_FAULT_CONTROL_EVENTS {
        return Err(FaultTransitionError::ControlEventBoundExceeded);
    }
    ledger.control_events.push(FaultControlEvent {
        run_id,
        run_sequence,
        schedule_id,
        tick,
        stage_event_count: ledger.events.len() as u64,
        kind,
    });
    Ok(())
}

/// Replay a control fact at its exact stage boundary.
pub(crate) fn replay_fault_control(
    replay: &mut FaultOutcomeLedger,
    event: &FaultControlEvent,
    requests: &mut Vec<PauseControlRequest>,
) -> Result<(), FaultTransitionError> {
    let mismatch = FaultTransitionError::ControlEventMismatch;
    if event.stage_event_count != replay.events.len() as u64 {
        return Err(mismatch);
    }
    if replay.control_events.last().is_some_and(|previous| {
        previous.run_sequence > event.run_sequence
            || (previous.run_id == event.run_id
                && (previous.schedule_id != event.schedule_id
                    || previous.run_sequence != event.run_sequence
                    || previous.tick > event.tick))
    }) {
        return Err(mismatch);
    }
    if replay
        .control_events
        .last()
        .is_some_and(|previous| previous.run_id != event.run_id)
    {
        requests.clear();
    }
    match &event.kind {
        FaultControlKind::PauseRequested {
            request,
            decision,
            applied_attempts: expected,
            active_effects: active,
        } => {
            if !request.valid_identity()
                || *expected != applied_attempts(replay)
                || *active != active_effects(replay, event.run_id, event.tick)
            {
                return Err(mismatch);
            }
            let candidate = request.request();
            let actual = if requests.len() >= crate::mechanisms::MAX_PAUSE_REQUESTS {
                Err(MechanismError::TooManyPauseRequests)
            } else {
                candidate
                    .as_ref()
                    .map_err(|error| *error)
                    .and_then(|request| {
                        let mut next = requests.clone();
                        next.push(request.clone());
                        admit_pause_requests(&next)
                    })
            };
            if actual != *decision {
                return Err(mismatch);
            }
            if actual.is_ok() {
                requests.push(candidate.map_err(|_| FaultTransitionError::ControlEventMismatch)?);
            }
        }
        FaultControlKind::SelectionSuppressed {
            tick,
            request_index,
            scheduled_entry_indices,
            scheduled_mechanisms,
            time_ns,
            random_due,
        } => {
            validate_pause_window(requests, *request_index, *tick)?;
            if *tick != event.tick
                || *tick != *time_ns / crate::outcomes::NANOSECONDS_PER_SIMULATION_TICK
                || (scheduled_entry_indices.is_empty() && !random_due)
            {
                return Err(mismatch);
            }
            if scheduled_entry_indices.len() > MAX_FAULT_ATTEMPTS
                || scheduled_mechanisms.len() != scheduled_entry_indices.len()
                || scheduled_entry_indices
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return Err(mismatch);
            }
        }
        FaultControlKind::ApplicationSuppressed {
            tick,
            request_index,
            attempt_id,
        } => {
            validate_pause_window(requests, *request_index, *tick)?;
            if *tick != event.tick {
                return Err(mismatch);
            }
            if replay.attempts.get(attempt_id).map(|state| state.stage)
                != Some(FaultAuthoritativeStage::Selected)
            {
                return Err(mismatch);
            }
        }
    }
    replay.control_events.push(event.clone());
    Ok(())
}

fn validate_pause_window(
    requests: &[PauseControlRequest],
    index: u32,
    tick: u64,
) -> Result<(), FaultTransitionError> {
    if requests
        .get(index as usize)
        .is_some_and(|request| request.start_tick <= tick && tick < request.end_tick)
    {
        Ok(())
    } else {
        Err(FaultTransitionError::ControlEventMismatch)
    }
}

pub fn admitted_pause_requests(
    ledger: &FaultOutcomeLedger,
    run_id: FaultRunId,
) -> Vec<PauseControlRequest> {
    ledger
        .control_events
        .iter()
        .filter(|event| event.run_id == run_id)
        .filter_map(|event| match &event.kind {
            FaultControlKind::PauseRequested {
                request,
                decision: Ok(()),
                ..
            } => request.request().ok(),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultEffectiveTarget {
    Vm {
        vm: u64,
    },
    Vcpu {
        vm: u64,
        vcpu: u64,
    },
    Thread {
        vm: u64,
        thread: crate::thread_pause::LogicalThreadId,
    },
    DirectionalLink {
        node: u64,
        peer: u64,
        direction: NetworkDirection,
    },
    Link {
        node: u64,
        peer: u64,
    },
    Partition {
        side_a: Vec<usize>,
        side_b: Vec<usize>,
    },
    AllLinks,
    CustomEffect {
        effect_id: String,
        version: u32,
    },
}

fn effective_target(fault: &Fault) -> FaultEffectiveTarget {
    match fault {
        Fault::NetworkDirectional {
            node,
            peer,
            direction,
            ..
        } => FaultEffectiveTarget::DirectionalLink {
            node: *node as u64,
            peer: *peer as u64,
            direction: *direction,
        },
        Fault::NetworkDirectionalHeal { node, peer } => FaultEffectiveTarget::Link {
            node: *node as u64,
            peer: *peer as u64,
        },
        Fault::NetworkPartition { side_a, side_b } => FaultEffectiveTarget::Partition {
            side_a: side_a.clone(),
            side_b: side_b.clone(),
        },
        Fault::NetworkHeal => FaultEffectiveTarget::AllLinks,
        Fault::CustomEffect {
            effect_id, version, ..
        } => FaultEffectiveTarget::CustomEffect {
            effect_id: effect_id.clone(),
            version: *version,
        },
        Fault::CpuStall { target, vcpu, .. }
        | Fault::CpuBitflip { target, vcpu, .. }
        | Fault::InjectNmi { target, vcpu } => FaultEffectiveTarget::Vcpu {
            vm: *target as u64,
            vcpu: *vcpu as u64,
        },
        Fault::ThreadPause { target, thread, .. } => FaultEffectiveTarget::Thread {
            vm: *target as u64,
            thread: thread.clone(),
        },
        _ => FaultEffectiveTarget::Vm {
            vm: fault.target().expect("VM-targeted fault") as u64,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultAttemptReceipt {
    pub attempt: FaultAttempt,
    pub mechanism: FaultVariant,
    pub effective_target: FaultEffectiveTarget,
    pub plan: Option<FaultPlanEffect>,
    pub counts: FaultStageCounters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultMechanismCounts {
    pub mechanism: FaultVariant,
    pub counts: FaultStageCounters,
    pub selection_suppression_opportunities: u64,
    pub application_suppressions: u64,
}

/// One directed link decision after an application or scoped/global heal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultOverlapDecision {
    pub stage_event_sequence: u64,
    pub from: u32,
    pub to: u32,
    pub candidates: Vec<FaultAttemptId>,
    pub effective_attempt: Option<FaultAttemptId>,
}

/// Resource/process slot at an application decision. Reasons remain in the exact stage event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultTargetDecision {
    pub stage_event_sequence: u64,
    pub attempt_id: FaultAttemptId,
    pub target: FaultEffectiveTarget,
    pub stage: FaultAuthoritativeStage,
    pub effective_attempt: Option<FaultAttemptId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultMechanismReceipt {
    pub schema: String,
    pub profile: String,
    pub seed: u64,
    pub max_attempts: usize,
    pub max_stage_events: usize,
    pub max_control_events: usize,
    pub counts: FaultStageCounters,
    pub mechanisms: Vec<FaultMechanismCounts>,
    pub attempts: Vec<FaultAttemptReceipt>,
    pub stages: Vec<crate::outcomes::FaultStageEvent>,
    pub control_events: Vec<FaultControlEvent>,
    pub overlap_decisions: Vec<FaultOverlapDecision>,
    pub target_decisions: Vec<FaultTargetDecision>,
    pub random_selection_suppression_opportunities: u64,
}

/// Pure projection; never promotes selection to application, observation, or coverage.
/// r[impl chaoscontrol.fault_mechanisms.evidence]
pub fn fault_mechanism_receipt(
    ledger: &FaultOutcomeLedger,
    seed: u64,
    profile: &str,
) -> Result<FaultMechanismReceipt, FaultTransitionError> {
    validate_fault_outcome_ledger(ledger)?;
    if profile.is_empty()
        || profile.len() > MAX_CUSTOM_STRING_BYTES
        || profile.chars().any(char::is_control)
    {
        return Err(FaultTransitionError::ReceiptProfileInvalid);
    }
    if ledger.attempts.values().any(|state| {
        state.attempt.run_id
            != crate::outcomes::fault_run_id(
                seed,
                state.attempt.run_sequence,
                state.attempt.schedule_id,
            )
    }) || ledger.control_events.iter().any(|event| {
        event.run_id != crate::outcomes::fault_run_id(seed, event.run_sequence, event.schedule_id)
    }) {
        return Err(FaultTransitionError::SnapshotRunIdentityMismatch);
    }
    let mut attempts = Vec::with_capacity(ledger.attempts.len());
    let mut mechanisms: Vec<FaultMechanismCounts> = Vec::new();
    for state in ledger.attempts.values() {
        if !receipt_fault_is_bounded(&state.attempt.fault) {
            return Err(FaultTransitionError::ReceiptPayloadBoundExceeded);
        }
        let mechanism = state.attempt.fault.variant();
        attempts.push(FaultAttemptReceipt {
            attempt: state.attempt.clone(),
            mechanism,
            effective_target: effective_target(&state.attempt.fault),
            plan: state.applicable_effect.clone(),
            counts: FaultStageCounters::default(),
        });
        if !mechanisms.iter().any(|entry| entry.mechanism == mechanism) {
            mechanisms.push(FaultMechanismCounts {
                mechanism,
                counts: FaultStageCounters::default(),
                selection_suppression_opportunities: 0,
                application_suppressions: 0,
            });
        }
    }
    for event in &ledger.events {
        let attempt = attempts
            .binary_search_by_key(&event.attempt_id, |entry| entry.attempt.id)
            .map_err(|_| FaultTransitionError::UnknownAttempt)?;
        count_stage(&mut attempts[attempt].counts, &event.kind);
        let mechanism = attempts[attempt].mechanism;
        let counts = &mut mechanisms
            .iter_mut()
            .find(|entry| entry.mechanism == mechanism)
            .ok_or(FaultTransitionError::UnknownAttempt)?
            .counts;
        count_stage(counts, &event.kind);
    }
    let mut random_selection_suppression_opportunities = 0;
    for control in &ledger.control_events {
        match &control.kind {
            FaultControlKind::SelectionSuppressed {
                scheduled_mechanisms,
                random_due,
                ..
            } => {
                random_selection_suppression_opportunities += u64::from(*random_due);
                for mechanism in scheduled_mechanisms {
                    let index = if let Some(index) = mechanisms
                        .iter()
                        .position(|entry| entry.mechanism == *mechanism)
                    {
                        index
                    } else {
                        mechanisms.push(FaultMechanismCounts {
                            mechanism: *mechanism,
                            counts: FaultStageCounters::default(),
                            selection_suppression_opportunities: 0,
                            application_suppressions: 0,
                        });
                        mechanisms.len() - 1
                    };
                    mechanisms[index].selection_suppression_opportunities += 1;
                }
            }
            FaultControlKind::ApplicationSuppressed { attempt_id, .. } => {
                let mechanism = ledger.attempts[attempt_id].attempt.fault.variant();
                let entry = mechanisms
                    .iter_mut()
                    .find(|entry| entry.mechanism == mechanism)
                    .ok_or(FaultTransitionError::UnknownAttempt)?;
                entry.application_suppressions += 1;
            }
            FaultControlKind::PauseRequested { .. } => {}
        }
    }
    Ok(FaultMechanismReceipt {
        schema: "chaoscontrol.fault-mechanism-receipt.v1".into(),
        profile: profile.into(),
        seed,
        max_attempts: MAX_FAULT_ATTEMPTS,
        max_stage_events: MAX_FAULT_OUTCOME_EVENTS,
        max_control_events: MAX_FAULT_CONTROL_EVENTS,
        counts: ledger.counters,
        mechanisms,
        attempts,
        random_selection_suppression_opportunities,
        stages: ledger.events.clone(),
        control_events: ledger.control_events.clone(),
        overlap_decisions: directional_decisions(ledger)?,
        target_decisions: target_decisions(ledger),
    })
}

fn count_stage(counts: &mut FaultStageCounters, kind: &FaultStageKind) {
    match kind {
        FaultStageKind::Selected => counts.selected += 1,
        FaultStageKind::Applicable { .. } => counts.applicable += 1,
        FaultStageKind::Applied { .. } => counts.applied += 1,
        FaultStageKind::Observed { .. } => counts.observed += 1,
        FaultStageKind::Rejected { .. } => counts.rejected += 1,
        FaultStageKind::ApplicationFailed { .. } => counts.application_failed += 1,
    }
}

fn directional_decisions(
    ledger: &FaultOutcomeLedger,
) -> Result<Vec<FaultOverlapDecision>, FaultTransitionError> {
    use std::collections::BTreeMap;
    type Candidate = (FaultAttemptId, NetworkDisruption, u64, u32);
    let mut active: BTreeMap<(u32, u32), Vec<Candidate>> = BTreeMap::new();
    let mut decisions = Vec::new();
    let mut run_id = None;
    let mut candidate_count = 0;
    for event in &ledger.events {
        let event_run = ledger.attempts[&event.attempt_id].attempt.run_id;
        if run_id != Some(event_run) {
            active.clear();
            run_id = Some(event_run);
        }
        let FaultStageKind::Applied { effect } = &event.kind else {
            continue;
        };
        let mut changed = Vec::new();
        match effect {
            FaultPlanEffect::NetworkDirectional {
                node,
                peer,
                direction,
                disruption,
                delay_ticks,
                queue_limit,
            } => {
                let pairs = match direction {
                    NetworkDirection::Outbound => vec![(*node, *peer)],
                    NetworkDirection::Inbound => vec![(*peer, *node)],
                    NetworkDirection::Both => vec![(*node, *peer), (*peer, *node)],
                };
                for pair in pairs {
                    active.entry(pair).or_default().push((
                        event.attempt_id,
                        *disruption,
                        *delay_ticks,
                        *queue_limit,
                    ));
                    changed.push(pair);
                }
            }
            FaultPlanEffect::NetworkDirectionalHeal { node, peer } => {
                for pair in [(*node, *peer), (*peer, *node)] {
                    active.remove(&pair);
                    changed.push(pair);
                }
            }
            FaultPlanEffect::NetworkHeal => {
                changed.extend(active.keys().copied());
                active.clear();
            }
            _ => {}
        }
        for (from, to) in changed {
            let candidates = active.get(&(from, to)).map(Vec::as_slice).unwrap_or(&[]);
            candidate_count += candidates.len();
            if candidate_count > MAX_FAULT_OUTCOME_EVENTS {
                return Err(FaultTransitionError::EventBoundExceeded);
            }
            // Match NetworkFabric::directional_for, including equal-mode parameter ties.
            let winner = candidates
                .iter()
                .fold(None::<&Candidate>, |best, candidate| {
                    if best.is_none_or(|old| {
                        candidate.1.severity() > old.1.severity()
                            || (candidate.1.severity() == old.1.severity()
                                && (candidate.2 > old.2 || candidate.3 > old.3))
                    }) {
                        Some(candidate)
                    } else {
                        best
                    }
                });
            decisions.push(FaultOverlapDecision {
                stage_event_sequence: event.sequence,
                from,
                to,
                candidates: candidates.iter().map(|entry| entry.0).collect(),
                effective_attempt: winner.map(|entry| entry.0),
            });
        }
    }
    Ok(decisions)
}

/// Validate a receipt by rebuilding it from its exact ledger and caller-bound identity.
pub fn validate_fault_mechanism_receipt(
    ledger: &FaultOutcomeLedger,
    seed: u64,
    profile: &str,
    receipt: &FaultMechanismReceipt,
) -> Result<(), FaultTransitionError> {
    if *receipt != fault_mechanism_receipt(ledger, seed, profile)? {
        return Err(FaultTransitionError::ReceiptMismatch);
    }
    Ok(())
}

pub(crate) fn validate_control_stage(
    replay: &FaultOutcomeLedger,
    selected: Option<&FaultAttempt>,
    event: &crate::outcomes::FaultStageEvent,
) -> Result<(), FaultTransitionError> {
    if let Some(attempt) = selected {
        let tick = attempt.selected_at_ns / crate::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
        if replay
            .control_events
            .iter()
            .filter(|control| control.run_id == attempt.run_id)
            .any(|control| {
                matches!(&control.kind, FaultControlKind::PauseRequested {
                request, decision: Ok(()), ..
            } if request.start_tick <= tick && tick < request.end_tick)
            })
        {
            return Err(FaultTransitionError::ControlEventMismatch);
        }
    }
    if let FaultStageKind::Rejected {
        reason:
            crate::outcomes::FaultRejectionReason::PauseControlActive {
                start_tick,
                end_tick,
            },
    } = &event.kind
    {
        let Some(control) = replay.control_events.last() else {
            return Err(FaultTransitionError::ControlEventMismatch);
        };
        let FaultControlKind::ApplicationSuppressed {
            request_index,
            attempt_id,
            ..
        } = &control.kind
        else {
            return Err(FaultTransitionError::ControlEventMismatch);
        };
        let request = replay
            .control_events
            .iter()
            .filter(|event| event.run_id == control.run_id)
            .filter_map(|event| match &event.kind {
                FaultControlKind::PauseRequested {
                    request,
                    decision: Ok(()),
                    ..
                } => Some(request),
                _ => None,
            })
            .nth(*request_index as usize);
        if *attempt_id != event.attempt_id
            || control.stage_event_count != event.sequence
            || request.is_none_or(|request| {
                request.start_tick != *start_tick || request.end_tick != *end_tick
            })
        {
            return Err(FaultTransitionError::ControlEventMismatch);
        }
    }
    Ok(())
}

pub(crate) fn validate_control_terminals(
    ledger: &FaultOutcomeLedger,
) -> Result<(), FaultTransitionError> {
    for event in &ledger.control_events {
        if let FaultControlKind::ApplicationSuppressed { attempt_id, .. } = &event.kind {
            if ledger.attempts.get(attempt_id).is_none_or(|state| state.attempt.run_id != event.run_id)
                || ledger.events.get(event.stage_event_count as usize).is_none_or(|stage| {
                    stage.attempt_id != *attempt_id || !matches!(stage.kind,
                        FaultStageKind::Rejected { reason: crate::outcomes::FaultRejectionReason::PauseControlActive { .. } })
                })
            {
                return Err(FaultTransitionError::ControlEventMismatch);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TargetSlot {
    Process(u64),
    CpuThrottle(u64),
    CpuStall(u64, u64),
    ClockFreeze(u64),
    MemoryPressure(u64),
    ThreadGate(u64),
}

fn target_slot(fault: &Fault) -> Option<TargetSlot> {
    match fault {
        Fault::ProcessKill { target }
        | Fault::ProcessPause { target, .. }
        | Fault::ProcessRestart { target } => Some(TargetSlot::Process(*target as u64)),
        Fault::CpuThrottle { target, .. } => Some(TargetSlot::CpuThrottle(*target as u64)),
        Fault::CpuStall { target, vcpu, .. } => {
            Some(TargetSlot::CpuStall(*target as u64, *vcpu as u64))
        }
        Fault::ClockFreeze { target, .. } => Some(TargetSlot::ClockFreeze(*target as u64)),
        Fault::MemoryPressure { target, .. } => Some(TargetSlot::MemoryPressure(*target as u64)),
        Fault::ThreadPause { target, .. } => Some(TargetSlot::ThreadGate(*target as u64)),
        _ => None,
    }
}

fn release_tick(effect: &FaultPlanEffect) -> Option<u64> {
    match effect {
        FaultPlanEffect::CpuThrottle {
            release_at_tick, ..
        }
        | FaultPlanEffect::CpuStall {
            release_at_tick, ..
        }
        | FaultPlanEffect::VirtualClockFreeze {
            release_at_tick, ..
        }
        | FaultPlanEffect::MemoryPressure {
            release_at_tick, ..
        }
        | FaultPlanEffect::ThreadPause {
            release_at_tick, ..
        } => Some(*release_at_tick),
        FaultPlanEffect::ProcessPause { resume_at_tick, .. } => Some(*resume_at_tick),
        FaultPlanEffect::ProcessRestart {
            restart_at_tick, ..
        } => Some(*restart_at_tick),
        _ => None,
    }
}

fn target_decisions(ledger: &FaultOutcomeLedger) -> Vec<FaultTargetDecision> {
    let mut active = std::collections::BTreeMap::<TargetSlot, (FaultAttemptId, Option<u64>)>::new();
    let mut decisions = Vec::new();
    let mut run_id = None;
    for event in &ledger.events {
        let attempt = &ledger.attempts[&event.attempt_id].attempt;
        if run_id != Some(attempt.run_id) {
            active.clear();
            run_id = Some(attempt.run_id);
        }
        let Some(slot) = target_slot(&attempt.fault) else {
            continue;
        };
        let tick = attempt.selected_at_ns / crate::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
        if active
            .get(&slot)
            .is_some_and(|(_, release)| release.is_some_and(|release| tick >= release))
        {
            active.remove(&slot);
        }
        let stage = match &event.kind {
            FaultStageKind::Applied { effect } => {
                active.insert(slot, (attempt.id, release_tick(effect)));
                FaultAuthoritativeStage::Applied
            }
            FaultStageKind::Rejected { .. } => FaultAuthoritativeStage::Rejected,
            FaultStageKind::ApplicationFailed { .. } => FaultAuthoritativeStage::ApplicationFailed,
            _ => continue,
        };
        decisions.push(FaultTargetDecision {
            stage_event_sequence: event.sequence,
            attempt_id: attempt.id,
            target: effective_target(&attempt.fault),
            stage,
            effective_attempt: active.get(&slot).map(|(id, _)| *id),
        });
    }
    decisions
}

/// Core-managed active windows, with expiry and scoped/global heal applied.
pub(crate) fn active_effects(
    ledger: &FaultOutcomeLedger,
    run_id: FaultRunId,
    tick: u64,
) -> Vec<FaultAttemptId> {
    let mut windows =
        std::collections::BTreeMap::<TargetSlot, (FaultAttemptId, Option<u64>)>::new();
    let mut links = std::collections::BTreeMap::<FaultAttemptId, (u32, u32)>::new();
    for event in &ledger.events {
        let attempt = &ledger.attempts[&event.attempt_id].attempt;
        if attempt.run_id != run_id {
            continue;
        }
        let FaultStageKind::Applied { effect } = &event.kind else {
            continue;
        };
        if let Some(slot) = target_slot(&attempt.fault) {
            windows.insert(slot, (event.attempt_id, release_tick(effect)));
        }
        match effect {
            FaultPlanEffect::NetworkDirectional { node, peer, .. } => {
                links.insert(event.attempt_id, (*node, *peer));
            }
            FaultPlanEffect::NetworkDirectionalHeal { node, peer } => {
                links.retain(|_, (left, right)| {
                    !((*left == *node && *right == *peer) || (*left == *peer && *right == *node))
                });
            }
            FaultPlanEffect::NetworkHeal => links.clear(),
            _ => {}
        }
    }
    let mut active = links.keys().copied().collect::<Vec<_>>();
    active.extend(
        windows.values().filter_map(|(id, release)| {
            release.is_none_or(|release| tick < release).then_some(*id)
        }),
    );
    active.sort_unstable();
    active
}

fn receipt_fault_is_bounded(fault: &Fault) -> bool {
    match fault {
        Fault::CustomEffect {
            effect_id,
            parameters,
            ..
        } => {
            effect_id.len() <= MAX_CUSTOM_STRING_BYTES
                && parameters.len() <= crate::mechanisms::MAX_CUSTOM_PARAMETERS
                && parameters.iter().all(|(name, value)| {
                    name.len() <= MAX_CUSTOM_STRING_BYTES && value.len() <= MAX_CUSTOM_STRING_BYTES
                })
        }
        Fault::ThreadPause { thread, .. } => thread.process.len() <= MAX_CUSTOM_STRING_BYTES,
        Fault::NetworkPartition { side_a, side_b } => {
            side_a.len() <= crate::outcomes::MAX_PARTITION_MEMBERS
                && side_b.len() <= crate::outcomes::MAX_PARTITION_MEMBERS
        }
        _ => true,
    }
}
