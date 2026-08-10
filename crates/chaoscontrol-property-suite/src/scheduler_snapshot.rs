// r[impl chaoscontrol.property_coverage.scheduler_snapshot]
use std::collections::BTreeMap;
use std::sync::Arc;

use chaoscontrol_vmm::scheduler::core::{
    HostEventKind, ProgressMode, ProgressSource, RunnableChange, ScheduleEvent,
};
use chaoscontrol_vmm::scheduler::{
    SchedulerConfig, SchedulerSnapshot, SchedulingStrategy, VcpuScheduler,
};
use chaoscontrol_vmm::snapshot::{SnapshotMemory, PAGE_SIZE};
use serde::{Deserialize, Serialize};

use crate::framework::{run_generated, DeterministicRng, Failure, PropertyProfile, SuiteReport};

const SUITE: &str = "scheduler-snapshot";
const VCPU_COUNT: usize = 2;
const QUANTUM: u64 = 4;
const SCHEDULER_SEED: u64 = 0x5343_4845_4455_4c45;
const COMMAND_VARIANTS: usize = 9;
const COMMAND_VALID_PROGRESS: usize = 0;
const COMMAND_STALE_PROGRESS: usize = 1;
const COMMAND_HOST_EVENT: usize = 2;
const COMMAND_SAVE: usize = 3;
const COMMAND_RESTORE: usize = 4;
const COMMAND_TAMPERED_RESTORE: usize = 5;
const COMMAND_BLOCK_ACTIVE: usize = 6;
const COMMAND_WAKE: usize = 7;
const COMMAND_OVERLAY_WRITE: usize = 8;
const OVERLAY_PAGES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    ValidProgress,
    StaleProgress,
    HostEvent,
    Save,
    Restore,
    TamperedRestore,
    BlockActive,
    Wake,
    OverlayWrite { page: usize, byte: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    active: usize,
    progress: Vec<u64>,
    boundary: u64,
    sequence: u64,
    runnable: Vec<bool>,
    halted: bool,
    saved: Option<ModelSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelSnapshot {
    active: usize,
    progress: Vec<u64>,
    boundary: u64,
    sequence: u64,
    runnable: Vec<bool>,
    halted: bool,
}

impl Model {
    fn new() -> Self {
        Self {
            active: 0,
            progress: vec![0; VCPU_COUNT],
            boundary: QUANTUM,
            sequence: 0,
            runnable: vec![true; VCPU_COUNT],
            halted: false,
            saved: None,
        }
    }

    fn accept_progress(&mut self) {
        self.progress[self.active] += 1;
        self.sequence += 1;
        if self.progress[self.active] == self.boundary {
            self.select_next_or_halt();
        }
    }

    fn snapshot(&self) -> ModelSnapshot {
        ModelSnapshot {
            active: self.active,
            progress: self.progress.clone(),
            boundary: self.boundary,
            sequence: self.sequence,
            runnable: self.runnable.clone(),
            halted: self.halted,
        }
    }

    fn restore(&mut self, snapshot: &ModelSnapshot) {
        self.active = snapshot.active;
        self.progress.clone_from(&snapshot.progress);
        self.boundary = snapshot.boundary;
        self.sequence = snapshot.sequence;
        self.runnable.clone_from(&snapshot.runnable);
        self.halted = snapshot.halted;
    }

    fn block_active(&mut self) {
        self.runnable[self.active] = false;
        self.sequence += 1;
        self.select_next_or_halt();
    }

    fn wake(&mut self) {
        let target = (self.active + 1) % VCPU_COUNT;
        self.runnable[target] = true;
        self.sequence += 1;
        self.select_next_or_halt();
    }

    fn select_next_or_halt(&mut self) {
        for offset in 1..=VCPU_COUNT {
            let candidate = (self.active + offset) % VCPU_COUNT;
            if self.runnable[candidate] {
                self.active = candidate;
                self.boundary = self.progress[candidate] + QUANTUM;
                self.halted = false;
                return;
            }
        }
        self.halted = true;
    }
}

pub fn run(selected: &PropertyProfile) -> Result<SuiteReport, crate::AnyCounterexample> {
    run_generated(SUITE, selected, generate, check)
        .map_err(crate::AnyCounterexample::scheduler_snapshot)
}

fn generate(rng: &mut DeterministicRng) -> Command {
    match rng.index(COMMAND_VARIANTS) {
        COMMAND_VALID_PROGRESS => Command::ValidProgress,
        COMMAND_STALE_PROGRESS => Command::StaleProgress,
        COMMAND_HOST_EVENT => Command::HostEvent,
        COMMAND_SAVE => Command::Save,
        COMMAND_RESTORE => Command::Restore,
        COMMAND_TAMPERED_RESTORE => Command::TamperedRestore,
        COMMAND_BLOCK_ACTIVE => Command::BlockActive,
        COMMAND_WAKE => Command::Wake,
        COMMAND_OVERLAY_WRITE => Command::OverlayWrite {
            page: rng.index(OVERLAY_PAGES),
            byte: rng.next_u64().to_le_bytes()[0],
        },
        _ => unreachable!("bounded command selector must produce a known scheduler command"),
    }
}

fn scheduler() -> VcpuScheduler {
    let config = SchedulerConfig {
        num_vcpus: VCPU_COUNT,
        quantum: QUANTUM,
        strategy: SchedulingStrategy::RoundRobin,
        seed: SCHEDULER_SEED,
    };
    VcpuScheduler::try_new(
        &config,
        ProgressMode::ExactSingleStep,
        vec![true; VCPU_COUNT],
    )
    .expect("the bounded scheduler profile must be admitted")
}

fn check(commands: &[Command]) -> Result<usize, Failure> {
    let mut actual = scheduler();
    let mut model = Model::new();
    let mut saved_actual: Option<SchedulerSnapshot> = None;
    let overlay_base = Arc::new(vec![0_u8; PAGE_SIZE * OVERLAY_PAGES]);
    let mut actual_overlay = SnapshotMemory::Overlay {
        base: Arc::clone(&overlay_base),
        dirty_pages: BTreeMap::new(),
    };
    let mut model_overlay = overlay_base.as_ref().clone();
    let mut saved_overlay: Option<(SnapshotMemory, Vec<u8>)> = None;
    let mut rejected = 0_usize;

    for (step, command) in commands.iter().enumerate() {
        if matches!(command, Command::ValidProgress) && model.halted {
            rejected += 1;
            compare(step, &actual, &model)?;
            compare_overlay(step, &actual_overlay, &model_overlay)?;
            continue;
        }
        match command {
            Command::ValidProgress => {
                let state = actual.state();
                let event = ScheduleEvent::GuestProgress {
                    expected_state_id: state.identity(),
                    vcpu: state.active_vcpu,
                    observed_progress: state.instruction_progress[state.active_vcpu] + 1,
                    runnable_changes: Vec::new(),
                    source: ProgressSource::ExactSingleStep,
                };
                let before = actual.state_id();
                let planned = actual.plan(&event).map_err(|error| {
                    Failure::new("valid-progress-admission", step, format!("{error:?}"))
                })?;
                let reservation = actual.reserve_transition().map_err(|error| {
                    Failure::new("valid-progress-reservation", step, format!("{error:?}"))
                })?;
                let record = actual.commit(reservation, planned).map_err(|error| {
                    Failure::new("valid-progress-commit", step, format!("{error:?}"))
                })?;
                if record.pre_state_id != before || record.post_state_id != actual.state_id() {
                    return Err(Failure::new(
                        "exact-transition-commit",
                        step,
                        "committed transition identities do not bind the exact state change",
                    ));
                }
                model.accept_progress();
            }
            Command::StaleProgress => {
                let before = actual.state().clone();
                let mut stale = actual.state_id();
                stale.0[0] ^= 1;
                let event = ScheduleEvent::GuestProgress {
                    expected_state_id: stale,
                    vcpu: actual.active(),
                    observed_progress: actual.state().instruction_progress[actual.active()] + 1,
                    runnable_changes: Vec::new(),
                    source: ProgressSource::ExactSingleStep,
                };
                if actual.plan(&event).is_ok() || actual.state() != &before {
                    return Err(Failure::new(
                        "stale-command-no-mutation",
                        step,
                        "a stale scheduler command succeeded or changed state",
                    ));
                }
                rejected += 1;
            }
            Command::HostEvent => {
                let before = actual.state().clone();
                let event = ScheduleEvent::HostEvent {
                    expected_state_id: actual.state_id(),
                    kind: HostEventKind::SignalInterrupt,
                };
                if actual.plan(&event).is_ok() || actual.state() != &before {
                    return Err(Failure::new(
                        "host-input-no-authority",
                        step,
                        "a host event changed deterministic scheduler state",
                    ));
                }
                rejected += 1;
            }
            Command::Save => {
                saved_actual = Some(actual.snapshot());
                model.saved = Some(model.snapshot());
                saved_overlay = Some((actual_overlay.clone(), model_overlay.clone()));
            }
            Command::Restore => match (&saved_actual, model.saved.clone(), saved_overlay.clone()) {
                (Some(snapshot), Some(model_snapshot), Some((overlay, overlay_model))) => {
                    actual.restore(snapshot).map_err(|error| {
                        Failure::new("valid-snapshot-restore", step, format!("{error:?}"))
                    })?;
                    model.restore(&model_snapshot);
                    actual_overlay = overlay;
                    model_overlay = overlay_model;
                }
                (None, None, None) => rejected += 1,
                _ => {
                    return Err(Failure::new(
                        "snapshot-model-alignment",
                        step,
                        "actual and model snapshot availability diverged",
                    ));
                }
            },
            Command::TamperedRestore => {
                let before = actual.state().clone();
                if let Some(snapshot) = &saved_actual {
                    let mut tampered = snapshot.clone();
                    tampered.state_id.0[0] ^= 1;
                    if actual.restore(&tampered).is_ok() || actual.state() != &before {
                        return Err(Failure::new(
                            "invalid-snapshot-no-mutation",
                            step,
                            "a tampered scheduler snapshot succeeded or changed state",
                        ));
                    }
                }
                rejected += 1;
            }
            Command::BlockActive => {
                if model.halted {
                    rejected += 1;
                } else {
                    let active = actual.active();
                    let event = ScheduleEvent::RunnableObservation {
                        expected_state_id: actual.state_id(),
                        runnable_changes: vec![RunnableChange {
                            vcpu: active,
                            runnable: false,
                        }],
                    };
                    commit_event(step, &mut actual, &event)?;
                    model.block_active();
                }
            }
            Command::Wake => {
                if !model.halted {
                    rejected += 1;
                } else {
                    let target = (actual.active() + 1) % VCPU_COUNT;
                    let event = ScheduleEvent::RunnableObservation {
                        expected_state_id: actual.state_id(),
                        runnable_changes: vec![RunnableChange {
                            vcpu: target,
                            runnable: true,
                        }],
                    };
                    commit_event(step, &mut actual, &event)?;
                    model.wake();
                }
            }
            Command::OverlayWrite { page, byte } => {
                let page_data = Box::new([*byte; PAGE_SIZE]);
                let SnapshotMemory::Overlay { dirty_pages, .. } = &mut actual_overlay else {
                    unreachable!("the property snapshot remains an overlay")
                };
                dirty_pages.insert(*page, page_data);
                let start = *page * PAGE_SIZE;
                let end = start + PAGE_SIZE;
                model_overlay[start..end].fill(*byte);
            }
        }
        compare(step, &actual, &model)?;
        compare_overlay(step, &actual_overlay, &model_overlay)?;
    }
    Ok(rejected)
}

fn commit_event(
    step: usize,
    actual: &mut VcpuScheduler,
    event: &ScheduleEvent,
) -> Result<(), Failure> {
    let planned = actual.plan(event).map_err(|error| {
        Failure::new("runnable-transition-admission", step, format!("{error:?}"))
    })?;
    let reservation = actual.reserve_transition().map_err(|error| {
        Failure::new(
            "runnable-transition-reservation",
            step,
            format!("{error:?}"),
        )
    })?;
    actual
        .commit(reservation, planned)
        .map_err(|error| Failure::new("runnable-transition-commit", step, format!("{error:?}")))?;
    Ok(())
}

fn compare(step: usize, actual: &VcpuScheduler, model: &Model) -> Result<(), Failure> {
    let state = actual.state();
    if state.active_vcpu != model.active
        || state.instruction_progress != model.progress
        || state.quantum_boundary != model.boundary
        || state.sequence != model.sequence
        || state.runnable_vcpus != model.runnable
        || state.halted != model.halted
    {
        return Err(Failure::new(
            "scheduler-reference-agreement",
            step,
            format!("actual={state:?}, model={model:?}"),
        ));
    }
    Ok(())
}

fn compare_overlay(step: usize, actual: &SnapshotMemory, model: &[u8]) -> Result<(), Failure> {
    if actual.memory_size() != model.len() || actual.materialize() != model {
        return Err(Failure::new(
            "snapshot-overlay-reference-agreement",
            step,
            "the materialized overlay differs from the independent byte model",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_stale_event_regression() {
        let commands: Vec<Command> = serde_json::from_str(include_str!(
            "../../../contracts/property-coverage/fixtures/regressions/scheduler-stale-event.json"
        ))
        .expect("the scheduler regression fixture must be valid JSON");
        assert!(check(&commands).is_ok());
    }
}
