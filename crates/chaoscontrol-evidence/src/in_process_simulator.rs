use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{EvidenceError, EvidenceResult};

pub const SIMULATOR_CONFIG_SCHEMA_VERSION: u64 = 1;
pub const SIMULATOR_RECEIPT_SCHEMA_VERSION: u64 = 1;
pub const DEFAULT_SIMULATOR_SCOPE: &str = "adapter-based in-process simulator evidence; not VM replay proof, not arbitrary binary support, not full FoundationDB parity";
const REQUIRED_SCOPE_FRAGMENT: &str = "not VM replay proof";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorConfig {
    pub schema_version: u64,
    pub run_id: String,
    pub seed: u64,
    pub workload: WorkloadIdentity,
    pub scheduler: SchedulerPolicy,
    pub virtual_clock: VirtualClockPolicy,
    pub rng: RngPolicy,
    pub network: NetworkProfile,
    pub disk: DiskProfile,
    pub fault_schedule: FaultScheduleRef,
    pub artifacts: BTreeMap<String, String>,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkloadIdentity {
    pub name: String,
    pub adapter_version: String,
    pub scenario_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReceiptBridgeMetadata {
    pub workload: WorkloadIdentity,
    pub seed_or_schedule_ref: String,
    pub artifact_digests: BTreeMap<String, String>,
    pub evidence_class: EvidenceClass,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    SimulatorLocal,
    VmSnapshotReplay,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct VmReplayReceiptBridgeMetadata {
    pub workload: WorkloadIdentity,
    pub seed_or_schedule_ref: String,
    pub artifact_digests: BTreeMap<String, String>,
    pub evidence_class: EvidenceClass,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorVmReceiptBridgeComparison {
    pub comparable: bool,
    pub workload_match: bool,
    pub adapter_version_match: bool,
    pub scenario_match: bool,
    pub seed_or_schedule_match: bool,
    pub artifact_digest_matches: BTreeMap<String, bool>,
    pub simulator_evidence_class: EvidenceClass,
    pub vm_evidence_class: EvidenceClass,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SchedulerPolicy {
    pub name: String,
    pub max_steps: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct VirtualClockPolicy {
    pub start_tick: u64,
    pub tick_quantum: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RngPolicy {
    pub algorithm: String,
    pub seed_derivation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkProfile {
    pub profile_id: String,
    pub simulated: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiskProfile {
    pub profile_id: String,
    pub simulated: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FaultScheduleRef {
    pub schedule_id: String,
    pub digest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorReceipt {
    pub schema_version: u64,
    pub run_id: String,
    pub config_sha256: String,
    pub schedule_sha256: String,
    pub history_sha256: String,
    pub output_sha256: String,
    pub bridge: ReceiptBridgeMetadata,
    pub observations: Vec<SimulatorObservation>,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorObservation {
    pub tick: u64,
    pub task_id: String,
    pub event: String,
    pub entropy: EntropySource,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntropySource {
    SimulatorClock,
    SimulatorRng,
    SimulatorIo,
    HostWallClock,
    HostRandom,
    ExternalIo,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SchedulerStep {
    pub step_index: u64,
    pub task_id: String,
    pub tick: u64,
    pub rng_value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicSimulatorCore {
    config: SimulatorConfig,
    clock: DeterministicClock,
    rng: DeterministicRng,
    scheduler: DeterministicScheduler,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicClock {
    current_tick: u64,
    tick_quantum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicRng {
    state: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicScheduler {
    max_steps: u64,
    emitted_steps: u64,
    runnable_tasks: VecDeque<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorRunEvidence {
    pub events: Vec<SimulatorAdapterEvent>,
    pub receipt: SimulatorReceipt,
    pub summary: SimulatorRunSummary,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorRunSummary {
    pub run_id: String,
    pub adapter_id: String,
    pub event_count: usize,
    pub fault_count: usize,
    pub network_deliveries: usize,
    pub disk_writes: usize,
    pub receipt_summary: String,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorAdapterEvent {
    pub step: SchedulerStep,
    pub operation: SimulatorOperation,
    pub result: SimulatorOperationResult,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SimulatorOperation {
    RegisterWrite { key: String, value: i64 },
    RegisterRead { key: String },
    NetworkSend { to: String, payload: String },
    DiskWrite { path: String, value: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SimulatorOperationResult {
    WriteOk,
    ReadOk { value: Option<i64> },
    NetworkDelivered,
    DiskWriteOk,
    FaultInjected { fault_id: String, reason: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RegisterSimulatorAdapter {
    adapter_id: String,
    state: BTreeMap<String, i64>,
    script: VecDeque<SimulatorOperation>,
    events: Vec<SimulatorAdapterEvent>,
}

pub trait InProcessWorkloadAdapter {
    fn adapter_id(&self) -> &str;
    fn runnable_tasks(&self) -> Vec<String>;
    fn apply_step(
        &mut self,
        step: SchedulerStep,
        hooks: &mut SimulatedFaultHooks,
    ) -> EvidenceResult<SimulatorAdapterEvent>;
    fn history_bytes(&self) -> EvidenceResult<Vec<u8>>;
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatedFaultHooks {
    pub network: SimulatedNetwork,
    pub disk: SimulatedDisk,
    pub faults: Vec<SimulatorFault>,
    pub unsupported_environment_hooks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatedNetwork {
    pub profile_id: String,
    pub delivered: Vec<NetworkMessage>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkMessage {
    pub from: String,
    pub to: String,
    pub payload: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatedDisk {
    pub profile_id: String,
    pub writes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorFault {
    pub fault_id: String,
    pub step_index: u64,
    pub action: FaultAction,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FaultAction {
    DropNetwork { to: String },
    FailDiskWrite { path: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorReceiptComparison {
    pub matched: bool,
    pub mismatch: Option<SimulatorReceiptMismatch>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimulatorReceiptMismatch {
    pub class: String,
    pub left: String,
    pub right: String,
}

impl DeterministicSimulatorCore {
    pub fn new(config: SimulatorConfig, task_ids: Vec<String>) -> EvidenceResult<Self> {
        validate_simulator_config(&config)?;
        Ok(Self {
            clock: DeterministicClock::new(config.virtual_clock.clone())?,
            rng: DeterministicRng::new(config.seed),
            scheduler: DeterministicScheduler::new(config.scheduler.clone(), task_ids)?,
            config,
        })
    }

    pub fn run_steps(&mut self) -> Vec<SchedulerStep> {
        let mut steps = Vec::new();
        while let Some(task_id) = self.scheduler.next_task() {
            let tick = self.clock.advance();
            let rng_value = self.rng.next_u64();
            steps.push(SchedulerStep {
                step_index: self.scheduler.emitted_steps - 1,
                task_id,
                tick,
                rng_value,
            });
        }
        steps
    }

    pub fn receipt_for_history(
        &self,
        history_bytes: &[u8],
        output_bytes: &[u8],
        observations: Vec<SimulatorObservation>,
    ) -> EvidenceResult<SimulatorReceipt> {
        let receipt = SimulatorReceipt {
            schema_version: SIMULATOR_RECEIPT_SCHEMA_VERSION,
            run_id: self.config.run_id.clone(),
            config_sha256: digest_json(&self.config)?,
            schedule_sha256: self.config.fault_schedule.digest.clone(),
            history_sha256: digest_bytes(history_bytes),
            output_sha256: digest_bytes(output_bytes),
            bridge: simulator_bridge_metadata(&self.config),
            observations,
            scope: DEFAULT_SIMULATOR_SCOPE.to_string(),
        };
        validate_simulator_receipt(&receipt)?;
        Ok(receipt)
    }
}

impl DeterministicClock {
    pub fn new(policy: VirtualClockPolicy) -> EvidenceResult<Self> {
        require(
            policy.tick_quantum > 0,
            "virtual clock tick_quantum must be > 0",
        )?;
        Ok(Self {
            current_tick: policy.start_tick,
            tick_quantum: policy.tick_quantum,
        })
    }

    pub fn advance(&mut self) -> u64 {
        let tick = self.current_tick;
        self.current_tick = self.current_tick.saturating_add(self.tick_quantum);
        tick
    }
}

impl DeterministicRng {
    pub fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        };
        Self { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

impl DeterministicScheduler {
    pub fn new(policy: SchedulerPolicy, task_ids: Vec<String>) -> EvidenceResult<Self> {
        require(policy.max_steps > 0, "scheduler max_steps must be > 0")?;
        require(!task_ids.is_empty(), "scheduler requires at least one task")?;
        for task_id in &task_ids {
            require(!task_id.is_empty(), "scheduler task_id must be non-empty")?;
        }
        Ok(Self {
            max_steps: policy.max_steps,
            emitted_steps: 0,
            runnable_tasks: task_ids.into(),
        })
    }

    fn next_task(&mut self) -> Option<String> {
        if self.emitted_steps >= self.max_steps {
            return None;
        }
        let task_id = self.runnable_tasks.pop_front()?;
        self.runnable_tasks.push_back(task_id.clone());
        self.emitted_steps += 1;
        Some(task_id)
    }
}

impl RegisterSimulatorAdapter {
    pub fn new(
        adapter_id: impl Into<String>,
        script: Vec<SimulatorOperation>,
    ) -> EvidenceResult<Self> {
        let adapter_id = adapter_id.into();
        require(
            !adapter_id.is_empty(),
            "simulator adapter_id must be non-empty",
        )?;
        require(
            !script.is_empty(),
            "simulator adapter script must contain at least one operation",
        )?;
        Ok(Self {
            adapter_id,
            state: BTreeMap::new(),
            script: script.into(),
            events: Vec::new(),
        })
    }

    pub fn sample() -> EvidenceResult<Self> {
        Self::new(
            "register-simulator-adapter-v1",
            vec![
                SimulatorOperation::RegisterWrite {
                    key: "counter".to_string(),
                    value: 1,
                },
                SimulatorOperation::NetworkSend {
                    to: "replica-b".to_string(),
                    payload: "counter=1".to_string(),
                },
                SimulatorOperation::DiskWrite {
                    path: "/register/counter".to_string(),
                    value: "1".to_string(),
                },
                SimulatorOperation::RegisterRead {
                    key: "counter".to_string(),
                },
            ],
        )
    }

    pub fn events(&self) -> &[SimulatorAdapterEvent] {
        &self.events
    }
}

impl InProcessWorkloadAdapter for RegisterSimulatorAdapter {
    fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    fn runnable_tasks(&self) -> Vec<String> {
        vec![self.adapter_id.clone()]
    }

    fn apply_step(
        &mut self,
        step: SchedulerStep,
        hooks: &mut SimulatedFaultHooks,
    ) -> EvidenceResult<SimulatorAdapterEvent> {
        let operation = self
            .script
            .pop_front()
            .ok_or_else(|| EvidenceError::new("simulator adapter script exhausted"))?;
        let result = hooks.apply(&step, &operation, &mut self.state)?;
        let event = SimulatorAdapterEvent {
            step,
            operation,
            result,
        };
        self.events.push(event.clone());
        Ok(event)
    }

    fn history_bytes(&self) -> EvidenceResult<Vec<u8>> {
        Ok(serde_json::to_vec(&self.events)?)
    }
}

impl SimulatedFaultHooks {
    pub fn new(
        network_profile_id: impl Into<String>,
        disk_profile_id: impl Into<String>,
    ) -> EvidenceResult<Self> {
        let network_profile_id = network_profile_id.into();
        let disk_profile_id = disk_profile_id.into();
        require(
            !network_profile_id.is_empty(),
            "simulated network profile_id must be non-empty",
        )?;
        require(
            !disk_profile_id.is_empty(),
            "simulated disk profile_id must be non-empty",
        )?;
        Ok(Self {
            network: SimulatedNetwork {
                profile_id: network_profile_id,
                delivered: Vec::new(),
            },
            disk: SimulatedDisk {
                profile_id: disk_profile_id,
                writes: BTreeMap::new(),
            },
            faults: Vec::new(),
            unsupported_environment_hooks: Vec::new(),
        })
    }

    pub fn with_faults(mut self, faults: Vec<SimulatorFault>) -> EvidenceResult<Self> {
        for fault in &faults {
            validate_simulator_fault(fault)?;
        }
        self.faults = faults;
        Ok(self)
    }

    fn apply(
        &mut self,
        step: &SchedulerStep,
        operation: &SimulatorOperation,
        state: &mut BTreeMap<String, i64>,
    ) -> EvidenceResult<SimulatorOperationResult> {
        match operation {
            SimulatorOperation::RegisterWrite { key, value } => {
                require(!key.is_empty(), "register write key must be non-empty")?;
                state.insert(key.clone(), *value);
                Ok(SimulatorOperationResult::WriteOk)
            }
            SimulatorOperation::RegisterRead { key } => {
                require(!key.is_empty(), "register read key must be non-empty")?;
                Ok(SimulatorOperationResult::ReadOk {
                    value: state.get(key).copied(),
                })
            }
            SimulatorOperation::NetworkSend { to, payload } => {
                require(!step.task_id.is_empty(), "network sender must be non-empty")?;
                require(!to.is_empty(), "network recipient must be non-empty")?;
                require(!payload.is_empty(), "network payload must be non-empty")?;
                if let Some(fault) = self.matching_network_fault(step.step_index, to) {
                    return Ok(SimulatorOperationResult::FaultInjected {
                        fault_id: fault.fault_id.clone(),
                        reason: format!("dropped network message to {to}"),
                    });
                }
                self.network.delivered.push(NetworkMessage {
                    from: step.task_id.clone(),
                    to: to.clone(),
                    payload: payload.clone(),
                });
                Ok(SimulatorOperationResult::NetworkDelivered)
            }
            SimulatorOperation::DiskWrite { path, value } => {
                require(!path.is_empty(), "disk write path must be non-empty")?;
                if let Some(fault) = self.matching_disk_fault(step.step_index, path) {
                    return Ok(SimulatorOperationResult::FaultInjected {
                        fault_id: fault.fault_id.clone(),
                        reason: format!("failed disk write to {path}"),
                    });
                }
                self.disk.writes.insert(path.clone(), value.clone());
                Ok(SimulatorOperationResult::DiskWriteOk)
            }
        }
    }

    fn matching_network_fault(&self, step_index: u64, to: &str) -> Option<&SimulatorFault> {
        self.faults.iter().find(|fault| match &fault.action {
            FaultAction::DropNetwork { to: target } => {
                fault.step_index == step_index && target == to
            }
            FaultAction::FailDiskWrite { .. } => false,
        })
    }

    fn matching_disk_fault(&self, step_index: u64, path: &str) -> Option<&SimulatorFault> {
        self.faults.iter().find(|fault| match &fault.action {
            FaultAction::FailDiskWrite { path: target } => {
                fault.step_index == step_index && target == path
            }
            FaultAction::DropNetwork { .. } => false,
        })
    }
}

pub fn run_simulator_adapter<A: InProcessWorkloadAdapter>(
    config: SimulatorConfig,
    adapter: &mut A,
    hooks: &mut SimulatedFaultHooks,
) -> EvidenceResult<Vec<SimulatorAdapterEvent>> {
    require(
        adapter.adapter_id() == config.workload.adapter_version,
        format!(
            "simulator adapter version mismatch: config {:?}, adapter {:?}",
            config.workload.adapter_version,
            adapter.adapter_id()
        ),
    )?;
    require(
        hooks.network.profile_id == config.network.profile_id,
        "simulated network profile must match config",
    )?;
    require(
        hooks.disk.profile_id == config.disk.profile_id,
        "simulated disk profile must match config",
    )?;
    require(
        hooks.unsupported_environment_hooks.is_empty(),
        format!(
            "unsupported environment hooks are not allowed in simulator evidence: {:?}",
            hooks.unsupported_environment_hooks
        ),
    )?;
    let mut core = DeterministicSimulatorCore::new(config, adapter.runnable_tasks())?;
    let mut events = Vec::new();
    for step in core.run_steps() {
        events.push(adapter.apply_step(step, hooks)?);
    }
    Ok(events)
}

pub fn run_simulator_adapter_receipt<A: InProcessWorkloadAdapter>(
    config: SimulatorConfig,
    adapter: &mut A,
    hooks: &mut SimulatedFaultHooks,
) -> EvidenceResult<SimulatorRunEvidence> {
    let config_for_receipt = config.clone();
    let adapter_id = adapter.adapter_id().to_string();
    let events = run_simulator_adapter(config, adapter, hooks)?;
    let history_bytes = adapter.history_bytes()?;
    let output_bytes = serde_json::to_vec(&serde_json::json!({
        "events": events,
        "network": hooks.network,
        "disk": hooks.disk,
        "scope": DEFAULT_SIMULATOR_SCOPE,
    }))?;
    let observations = events
        .iter()
        .flat_map(|event| {
            [
                SimulatorObservation {
                    tick: event.step.tick,
                    task_id: event.step.task_id.clone(),
                    event: format!("{:?}", event.result),
                    entropy: EntropySource::SimulatorClock,
                },
                SimulatorObservation {
                    tick: event.step.tick,
                    task_id: event.step.task_id.clone(),
                    event: "scheduler-rng-bound".to_string(),
                    entropy: EntropySource::SimulatorRng,
                },
            ]
        })
        .collect::<Vec<_>>();
    let core = DeterministicSimulatorCore::new(config_for_receipt, vec![adapter_id.clone()])?;
    let receipt = core.receipt_for_history(&history_bytes, &output_bytes, observations)?;
    let summary = summarize_simulator_run(&adapter_id, &events, hooks, &receipt)?;
    Ok(SimulatorRunEvidence {
        events,
        receipt,
        summary,
    })
}

pub fn sample_simulator_run_evidence() -> EvidenceResult<SimulatorRunEvidence> {
    let config = sample_simulator_config();
    let mut adapter = RegisterSimulatorAdapter::sample()?;
    let mut hooks = sample_simulated_fault_hooks(&config)?;
    run_simulator_adapter_receipt(config, &mut adapter, &mut hooks)
}

pub fn write_sample_simulator_receipt_path(
    path: impl AsRef<std::path::Path>,
) -> EvidenceResult<()> {
    let path = path.as_ref();
    let evidence = sample_simulator_run_evidence()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&evidence.receipt)?)?;
    Ok(())
}

pub fn validate_simulator_receipt_path(
    path: impl AsRef<std::path::Path>,
) -> EvidenceResult<String> {
    let bytes = std::fs::read(path)?;
    let receipt: SimulatorReceipt = serde_json::from_slice(&bytes)?;
    validate_simulator_receipt(&receipt)?;
    Ok(summarize_simulator_receipt(&receipt))
}

pub fn summarize_simulator_receipt(receipt: &SimulatorReceipt) -> String {
    format!(
        "in-process-simulator run={} observations={} history={} output={} scope=adapter-simulator-not-vm-replay",
        receipt.run_id,
        receipt.observations.len(),
        receipt.history_sha256,
        receipt.output_sha256
    )
}

fn summarize_simulator_run(
    adapter_id: &str,
    events: &[SimulatorAdapterEvent],
    hooks: &SimulatedFaultHooks,
    receipt: &SimulatorReceipt,
) -> EvidenceResult<SimulatorRunSummary> {
    validate_simulator_receipt(receipt)?;
    let fault_count = events
        .iter()
        .filter(|event| matches!(event.result, SimulatorOperationResult::FaultInjected { .. }))
        .count();
    Ok(SimulatorRunSummary {
        run_id: receipt.run_id.clone(),
        adapter_id: adapter_id.to_string(),
        event_count: events.len(),
        fault_count,
        network_deliveries: hooks.network.delivered.len(),
        disk_writes: hooks.disk.writes.len(),
        receipt_summary: summarize_simulator_receipt(receipt),
        scope: DEFAULT_SIMULATOR_SCOPE.to_string(),
    })
}

pub fn simulator_bridge_metadata(config: &SimulatorConfig) -> ReceiptBridgeMetadata {
    ReceiptBridgeMetadata {
        workload: config.workload.clone(),
        seed_or_schedule_ref: format!(
            "seed:{} schedule:{}",
            config.seed, config.fault_schedule.schedule_id
        ),
        artifact_digests: config.artifacts.clone(),
        evidence_class: EvidenceClass::SimulatorLocal,
    }
}

pub fn sample_vm_replay_bridge_metadata() -> VmReplayReceiptBridgeMetadata {
    let config = sample_simulator_config();
    VmReplayReceiptBridgeMetadata {
        workload: config.workload.clone(),
        seed_or_schedule_ref: format!(
            "seed:{} schedule:{}",
            config.seed, config.fault_schedule.schedule_id
        ),
        artifact_digests: config.artifacts,
        evidence_class: EvidenceClass::VmSnapshotReplay,
    }
}

pub fn validate_receipt_bridge_metadata(metadata: &ReceiptBridgeMetadata) -> EvidenceResult<()> {
    validate_bridge_workload_identity(&metadata.workload)?;
    validate_bridge_digest_map(&metadata.artifact_digests)?;
    require(
        !metadata.seed_or_schedule_ref.is_empty(),
        "receipt bridge seed_or_schedule_ref must be non-empty",
    )?;
    Ok(())
}

pub fn validate_vm_replay_bridge_metadata(
    metadata: &VmReplayReceiptBridgeMetadata,
) -> EvidenceResult<()> {
    validate_bridge_workload_identity(&metadata.workload)?;
    validate_bridge_digest_map(&metadata.artifact_digests)?;
    require(
        !metadata.seed_or_schedule_ref.is_empty(),
        "VM bridge seed_or_schedule_ref must be non-empty",
    )?;
    require(
        metadata.evidence_class == EvidenceClass::VmSnapshotReplay,
        "VM bridge evidence_class must be vm_snapshot_replay",
    )?;
    Ok(())
}

pub fn compare_simulator_vm_receipt_bridge(
    simulator: &ReceiptBridgeMetadata,
    vm: &VmReplayReceiptBridgeMetadata,
) -> EvidenceResult<SimulatorVmReceiptBridgeComparison> {
    validate_receipt_bridge_metadata(simulator)?;
    validate_vm_replay_bridge_metadata(vm)?;
    require(
        simulator.evidence_class == EvidenceClass::SimulatorLocal,
        "simulator bridge evidence_class must remain simulator_local",
    )?;
    let workload_match = simulator.workload.name == vm.workload.name;
    let adapter_version_match = simulator.workload.adapter_version == vm.workload.adapter_version;
    let scenario_match = simulator.workload.scenario_id == vm.workload.scenario_id;
    let seed_or_schedule_match = simulator.seed_or_schedule_ref == vm.seed_or_schedule_ref;
    let mut artifact_digest_matches = BTreeMap::new();
    for key in simulator
        .artifact_digests
        .keys()
        .chain(vm.artifact_digests.keys())
    {
        artifact_digest_matches
            .entry(key.clone())
            .or_insert_with(|| simulator.artifact_digests.get(key) == vm.artifact_digests.get(key));
    }
    let artifacts_match = artifact_digest_matches.values().all(|matched| *matched);
    let comparable = workload_match
        && adapter_version_match
        && scenario_match
        && seed_or_schedule_match
        && artifacts_match;
    let summary = format!(
        "sim-vm-bridge workload={} adapter={} scenario={} seed_or_schedule={} artifacts={} classes=simulator-local,vm-snapshot-replay comparable={} (simulator evidence is not VM replay proof)",
        workload_match,
        adapter_version_match,
        scenario_match,
        seed_or_schedule_match,
        artifacts_match,
        comparable
    );
    Ok(SimulatorVmReceiptBridgeComparison {
        comparable,
        workload_match,
        adapter_version_match,
        scenario_match,
        seed_or_schedule_match,
        artifact_digest_matches,
        simulator_evidence_class: simulator.evidence_class.clone(),
        vm_evidence_class: vm.evidence_class.clone(),
        summary,
    })
}

fn validate_bridge_workload_identity(workload: &WorkloadIdentity) -> EvidenceResult<()> {
    require(
        !workload.name.is_empty(),
        "bridge workload name must be non-empty",
    )?;
    require(
        !workload.adapter_version.is_empty(),
        "bridge workload adapter_version must be non-empty",
    )?;
    require(
        !workload.scenario_id.is_empty(),
        "bridge workload scenario_id must be non-empty",
    )?;
    Ok(())
}

fn validate_bridge_digest_map(digests: &BTreeMap<String, String>) -> EvidenceResult<()> {
    require(
        !digests.is_empty(),
        "bridge artifact digests must be non-empty",
    )?;
    for (name, digest) in digests {
        require(!name.is_empty(), "bridge artifact name must be non-empty")?;
        require(
            digest.starts_with("sha256:"),
            format!("bridge artifact {name:?} digest must be sha256-bound"),
        )?;
    }
    Ok(())
}

pub fn sample_simulated_fault_hooks(
    config: &SimulatorConfig,
) -> EvidenceResult<SimulatedFaultHooks> {
    SimulatedFaultHooks::new(
        config.network.profile_id.clone(),
        config.disk.profile_id.clone(),
    )
}

pub fn validate_simulator_config(config: &SimulatorConfig) -> EvidenceResult<()> {
    require(
        config.schema_version == SIMULATOR_CONFIG_SCHEMA_VERSION,
        format!("simulator config schema_version must be {SIMULATOR_CONFIG_SCHEMA_VERSION}"),
    )?;
    require(
        !config.run_id.is_empty(),
        "simulator run_id must be non-empty",
    )?;
    require(
        !config.workload.name.is_empty(),
        "simulator workload name must be non-empty",
    )?;
    require(
        !config.workload.adapter_version.is_empty(),
        "simulator workload adapter_version must be non-empty",
    )?;
    require(
        !config.workload.scenario_id.is_empty(),
        "simulator workload scenario_id must be non-empty",
    )?;
    require(
        !config.scheduler.name.is_empty(),
        "simulator scheduler policy name must be non-empty",
    )?;
    require(
        config.scheduler.max_steps > 0,
        "simulator scheduler max_steps must be > 0",
    )?;
    require(
        config.virtual_clock.tick_quantum > 0,
        "simulator virtual clock tick_quantum must be > 0",
    )?;
    require(
        config.rng.algorithm == "xorshift64-v1",
        format!(
            "unsupported simulator rng algorithm {:?}",
            config.rng.algorithm
        ),
    )?;
    require(
        !config.rng.seed_derivation.is_empty(),
        "simulator rng seed_derivation must be non-empty",
    )?;
    require(
        config.network.simulated,
        "simulator network profile must be simulated",
    )?;
    require(
        config.disk.simulated,
        "simulator disk profile must be simulated",
    )?;
    require(
        !config.fault_schedule.schedule_id.is_empty(),
        "simulator fault schedule_id must be non-empty",
    )?;
    require(
        config.fault_schedule.digest.starts_with("sha256:"),
        "simulator fault schedule digest must be sha256-bound",
    )?;
    require(
        !config.artifacts.is_empty(),
        "simulator config must bind at least one artifact digest",
    )?;
    for (name, digest) in &config.artifacts {
        require(
            !name.is_empty(),
            "simulator artifact name must be non-empty",
        )?;
        require(
            digest.starts_with("sha256:"),
            format!("simulator artifact {name:?} digest must be sha256-bound"),
        )?;
    }
    require(
        config.scope.contains(REQUIRED_SCOPE_FRAGMENT),
        "simulator config scope must state it is not VM replay proof",
    )?;
    Ok(())
}

pub fn validate_simulator_receipt(receipt: &SimulatorReceipt) -> EvidenceResult<()> {
    require(
        receipt.schema_version == SIMULATOR_RECEIPT_SCHEMA_VERSION,
        format!("simulator receipt schema_version must be {SIMULATOR_RECEIPT_SCHEMA_VERSION}"),
    )?;
    require(
        !receipt.run_id.is_empty(),
        "simulator receipt run_id must be non-empty",
    )?;
    require(
        receipt.config_sha256.starts_with("sha256:"),
        "simulator receipt config_sha256 must be sha256-bound",
    )?;
    require(
        receipt.schedule_sha256.starts_with("sha256:"),
        "simulator receipt schedule_sha256 must be sha256-bound",
    )?;
    require(
        receipt.history_sha256.starts_with("sha256:"),
        "simulator receipt history_sha256 must be sha256-bound",
    )?;
    require(
        receipt.output_sha256.starts_with("sha256:"),
        "simulator receipt output_sha256 must be sha256-bound",
    )?;
    validate_receipt_bridge_metadata(&receipt.bridge)?;
    require(
        receipt.bridge.evidence_class == EvidenceClass::SimulatorLocal,
        "simulator receipt bridge evidence_class must be simulator_local",
    )?;
    require(
        receipt.scope.contains(REQUIRED_SCOPE_FRAGMENT),
        "simulator receipt scope must state it is not VM replay proof",
    )?;
    for observation in &receipt.observations {
        validate_simulator_observation(observation)?;
    }
    Ok(())
}

pub fn compare_simulator_receipts(
    left: &SimulatorReceipt,
    right: &SimulatorReceipt,
) -> EvidenceResult<SimulatorReceiptComparison> {
    validate_simulator_receipt(left)?;
    validate_simulator_receipt(right)?;
    for (class, left_value, right_value) in [
        ("run_id", &left.run_id, &right.run_id),
        ("config", &left.config_sha256, &right.config_sha256),
        ("schedule", &left.schedule_sha256, &right.schedule_sha256),
        ("history", &left.history_sha256, &right.history_sha256),
        ("output", &left.output_sha256, &right.output_sha256),
    ] {
        if left_value != right_value {
            return Ok(SimulatorReceiptComparison {
                matched: false,
                mismatch: Some(SimulatorReceiptMismatch {
                    class: class.to_string(),
                    left: left_value.clone(),
                    right: right_value.clone(),
                }),
            });
        }
    }
    Ok(SimulatorReceiptComparison {
        matched: true,
        mismatch: None,
    })
}

pub fn sample_simulator_config() -> SimulatorConfig {
    let mut artifacts = BTreeMap::new();
    artifacts.insert(
        "workload-adapter".to_string(),
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
    );
    SimulatorConfig {
        schema_version: SIMULATOR_CONFIG_SCHEMA_VERSION,
        run_id: "sample-in-process-simulator".to_string(),
        seed: 42,
        workload: WorkloadIdentity {
            name: "register-model".to_string(),
            adapter_version: "register-simulator-adapter-v1".to_string(),
            scenario_id: "register-smoke".to_string(),
        },
        scheduler: SchedulerPolicy {
            name: "round-robin-v1".to_string(),
            max_steps: 4,
        },
        virtual_clock: VirtualClockPolicy {
            start_tick: 10,
            tick_quantum: 5,
        },
        rng: RngPolicy {
            algorithm: "xorshift64-v1".to_string(),
            seed_derivation: "config.seed".to_string(),
        },
        network: NetworkProfile {
            profile_id: "loopback-simulated-network".to_string(),
            simulated: true,
        },
        disk: DiskProfile {
            profile_id: "memory-backed-simulated-disk".to_string(),
            simulated: true,
        },
        fault_schedule: FaultScheduleRef {
            schedule_id: "no-faults".to_string(),
            digest: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        },
        artifacts,
        scope: DEFAULT_SIMULATOR_SCOPE.to_string(),
    }
}

fn validate_simulator_fault(fault: &SimulatorFault) -> EvidenceResult<()> {
    require(
        !fault.fault_id.is_empty(),
        "simulator fault_id must be non-empty",
    )?;
    match &fault.action {
        FaultAction::DropNetwork { to } => require(
            !to.is_empty(),
            "drop-network fault recipient must be non-empty",
        ),
        FaultAction::FailDiskWrite { path } => require(
            !path.is_empty(),
            "fail-disk-write fault path must be non-empty",
        ),
    }
}

fn validate_simulator_observation(observation: &SimulatorObservation) -> EvidenceResult<()> {
    require(
        !observation.task_id.is_empty(),
        "simulator observation task_id must be non-empty",
    )?;
    require(
        !observation.event.is_empty(),
        "simulator observation event must be non-empty",
    )?;
    match observation.entropy {
        EntropySource::SimulatorClock
        | EntropySource::SimulatorRng
        | EntropySource::SimulatorIo => Ok(()),
        EntropySource::HostWallClock => Err(EvidenceError::new(
            "unbound nondeterminism: host wall-clock time is unsupported simulator evidence",
        )),
        EntropySource::HostRandom => Err(EvidenceError::new(
            "unbound nondeterminism: host randomness is unsupported simulator evidence",
        )),
        EntropySource::ExternalIo => Err(EvidenceError::new(
            "unbound nondeterminism: external I/O is unsupported simulator evidence",
        )),
    }
}

fn digest_json<T: Serialize>(value: &T) -> EvidenceResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(digest_bytes(&bytes))
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn require(condition: bool, message: impl Into<String>) -> EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(EvidenceError::new(message.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulator_config_binds_deterministic_sources() {
        let config = sample_simulator_config();
        validate_simulator_config(&config).expect("sample config validates");
        assert_eq!(config.seed, 42);
        assert_eq!(config.scheduler.name, "round-robin-v1");
        assert_eq!(config.virtual_clock.tick_quantum, 5);
        assert!(config.network.simulated);
        assert!(config.disk.simulated);
    }

    #[test]
    fn scheduler_clock_and_rng_are_repeatable() {
        let config = sample_simulator_config();
        let task_ids = vec!["client-a".to_string(), "client-b".to_string()];
        let mut left = DeterministicSimulatorCore::new(config.clone(), task_ids.clone())
            .expect("left simulator builds");
        let mut right =
            DeterministicSimulatorCore::new(config, task_ids).expect("right simulator builds");

        let left_steps = left.run_steps();
        let right_steps = right.run_steps();
        assert_eq!(left_steps, right_steps);
        assert_eq!(
            left_steps
                .iter()
                .map(|step| step.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["client-a", "client-b", "client-a", "client-b"]
        );
        assert_eq!(
            left_steps.iter().map(|step| step.tick).collect::<Vec<_>>(),
            vec![10, 15, 20, 25]
        );
    }

    #[test]
    fn receipts_compare_reproducible_runs_and_bounded_mismatches() {
        let config = sample_simulator_config();
        let simulator = DeterministicSimulatorCore::new(config, vec!["client-a".to_string()])
            .expect("simulator builds");
        let observations = vec![SimulatorObservation {
            tick: 10,
            task_id: "client-a".to_string(),
            event: "write-ok".to_string(),
            entropy: EntropySource::SimulatorClock,
        }];
        let left = simulator
            .receipt_for_history(b"history-a", b"output-a", observations.clone())
            .expect("left receipt validates");
        let right = simulator
            .receipt_for_history(b"history-a", b"output-a", observations)
            .expect("right receipt validates");
        let comparison = compare_simulator_receipts(&left, &right).expect("compare receipts");
        assert!(comparison.matched);

        let divergent = simulator
            .receipt_for_history(b"history-b", b"output-a", vec![])
            .expect("divergent receipt validates");
        let mismatch = compare_simulator_receipts(&left, &divergent)
            .expect("compare divergent receipts")
            .mismatch
            .expect("mismatch reported");
        assert_eq!(mismatch.class, "history");
    }

    #[test]
    fn unbound_nondeterminism_fails_closed() {
        let config = sample_simulator_config();
        let simulator = DeterministicSimulatorCore::new(config, vec!["client-a".to_string()])
            .expect("simulator builds");
        for (entropy, expected) in [
            (EntropySource::HostWallClock, "host wall-clock"),
            (EntropySource::HostRandom, "host randomness"),
            (EntropySource::ExternalIo, "external I/O"),
        ] {
            let err = simulator
                .receipt_for_history(
                    b"history",
                    b"output",
                    vec![SimulatorObservation {
                        tick: 10,
                        task_id: "client-a".to_string(),
                        event: "bad".to_string(),
                        entropy,
                    }],
                )
                .expect_err("unbound nondeterminism rejected");
            assert!(err.message().contains(expected));
        }
    }

    #[test]
    fn simulator_evidence_overclaims_fail_closed() {
        let mut config = sample_simulator_config();
        config.scope = "full FoundationDB parity and VM replay proof".to_string();
        assert!(validate_simulator_config(&config)
            .expect_err("overclaimed config rejected")
            .message()
            .contains("not VM replay proof"));

        let mut receipt = SimulatorReceipt {
            schema_version: SIMULATOR_RECEIPT_SCHEMA_VERSION,
            run_id: "sample".to_string(),
            config_sha256:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            schedule_sha256:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            history_sha256:
                "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    .to_string(),
            output_sha256:
                "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                    .to_string(),
            bridge: simulator_bridge_metadata(&sample_simulator_config()),
            observations: vec![],
            scope: "proves arbitrary binaries".to_string(),
        };
        assert!(validate_simulator_receipt(&receipt)
            .expect_err("overclaimed receipt rejected")
            .message()
            .contains("not VM replay proof"));
        receipt.scope = DEFAULT_SIMULATOR_SCOPE.to_string();
        validate_simulator_receipt(&receipt).expect("bounded receipt validates");
    }

    #[test]
    fn register_adapter_runs_on_simulated_network_and_disk() {
        let mut config = sample_simulator_config();
        config.scheduler.max_steps = 4;
        let mut adapter = RegisterSimulatorAdapter::sample().expect("sample adapter builds");
        let mut hooks = sample_simulated_fault_hooks(&config).expect("hooks build");

        let events = run_simulator_adapter(config, &mut adapter, &mut hooks).expect("adapter runs");

        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[0].result,
            SimulatorOperationResult::WriteOk
        ));
        assert!(matches!(
            events[1].result,
            SimulatorOperationResult::NetworkDelivered
        ));
        assert!(matches!(
            events[2].result,
            SimulatorOperationResult::DiskWriteOk
        ));
        assert!(matches!(
            events[3].result,
            SimulatorOperationResult::ReadOk { value: Some(1) }
        ));
        assert_eq!(hooks.network.delivered.len(), 1);
        assert_eq!(
            hooks.disk.writes.get("/register/counter"),
            Some(&"1".to_string())
        );
        assert_eq!(
            adapter.history_bytes().expect("history serializes"),
            serde_json::to_vec(adapter.events()).expect("events serialize")
        );
    }

    #[test]
    fn simulated_fault_hooks_are_bounded_and_deterministic() {
        let mut config = sample_simulator_config();
        config.scheduler.max_steps = 4;
        let fault = SimulatorFault {
            fault_id: "drop-replica-b".to_string(),
            step_index: 1,
            action: FaultAction::DropNetwork {
                to: "replica-b".to_string(),
            },
        };
        let mut left_adapter = RegisterSimulatorAdapter::sample().expect("left adapter builds");
        let mut right_adapter = RegisterSimulatorAdapter::sample().expect("right adapter builds");
        let mut left_hooks = sample_simulated_fault_hooks(&config)
            .expect("hooks build")
            .with_faults(vec![fault.clone()])
            .expect("fault validates");
        let mut right_hooks = sample_simulated_fault_hooks(&config)
            .expect("hooks build")
            .with_faults(vec![fault])
            .expect("fault validates");

        let left = run_simulator_adapter(config.clone(), &mut left_adapter, &mut left_hooks)
            .expect("left run succeeds");
        let right = run_simulator_adapter(config, &mut right_adapter, &mut right_hooks)
            .expect("right run succeeds");

        assert_eq!(left, right);
        assert!(matches!(
            left[1].result,
            SimulatorOperationResult::FaultInjected { ref fault_id, .. } if fault_id == "drop-replica-b"
        ));
        assert!(left_hooks.network.delivered.is_empty());
    }

    #[test]
    fn simulator_run_evidence_emits_reproducibility_receipt_and_summary() {
        let left = sample_simulator_run_evidence().expect("left evidence emits");
        let right = sample_simulator_run_evidence().expect("right evidence emits");

        assert_eq!(left.receipt, right.receipt);
        assert_eq!(left.summary, right.summary);
        assert_eq!(left.summary.event_count, 4);
        assert_eq!(left.summary.network_deliveries, 1);
        assert_eq!(left.summary.disk_writes, 1);
        assert!(left.summary.receipt_summary.contains("not-vm-replay"));
        assert!(
            compare_simulator_receipts(&left.receipt, &right.receipt)
                .expect("receipts compare")
                .matched
        );
    }

    #[test]
    fn simulator_receipt_summary_keeps_boundary_wording() {
        let evidence = sample_simulator_run_evidence().expect("evidence emits");
        assert!(evidence.summary.scope.contains("not VM replay proof"));
        assert!(evidence
            .summary
            .scope
            .contains("not full FoundationDB parity"));
        assert!(summarize_simulator_receipt(&evidence.receipt)
            .contains("adapter-simulator-not-vm-replay"));
    }

    #[test]
    fn adapter_and_hook_mismatches_fail_closed() {
        let mut config = sample_simulator_config();
        config.scheduler.max_steps = 1;
        let mut adapter = RegisterSimulatorAdapter::new(
            "wrong-adapter-version",
            vec![SimulatorOperation::RegisterRead {
                key: "counter".to_string(),
            }],
        )
        .expect("adapter builds");
        let mut hooks = sample_simulated_fault_hooks(&config).expect("hooks build");
        assert!(
            run_simulator_adapter(config.clone(), &mut adapter, &mut hooks)
                .expect_err("adapter mismatch rejected")
                .message()
                .contains("adapter version mismatch")
        );

        let mut adapter = RegisterSimulatorAdapter::sample().expect("sample adapter builds");
        let mut wrong_hooks =
            SimulatedFaultHooks::new("wrong-network", config.disk.profile_id.clone())
                .expect("wrong hooks build");
        assert!(
            run_simulator_adapter(config.clone(), &mut adapter, &mut wrong_hooks)
                .expect_err("network mismatch rejected")
                .message()
                .contains("network profile")
        );

        assert!(SimulatedFaultHooks::new("net", "disk")
            .expect("hooks build")
            .with_faults(vec![SimulatorFault {
                fault_id: "".to_string(),
                step_index: 0,
                action: FaultAction::DropNetwork {
                    to: "replica-b".to_string(),
                },
            }])
            .expect_err("bad fault rejected")
            .message()
            .contains("fault_id"));

        let mut adapter = RegisterSimulatorAdapter::sample().expect("sample adapter builds");
        let mut unsupported_hooks = sample_simulated_fault_hooks(&config).expect("hooks build");
        unsupported_hooks
            .unsupported_environment_hooks
            .push("host-wall-clock".to_string());
        assert!(
            run_simulator_adapter(config, &mut adapter, &mut unsupported_hooks)
                .expect_err("unsupported env hook rejected")
                .message()
                .contains("unsupported environment hooks")
        );
    }
    #[test]
    fn bridge_metadata_compares_simulator_and_vm_receipts_without_merging_evidence_classes() {
        let evidence = sample_simulator_run_evidence().expect("simulator evidence emits");
        let vm = sample_vm_replay_bridge_metadata();
        let comparison = compare_simulator_vm_receipt_bridge(&evidence.receipt.bridge, &vm)
            .expect("bridge comparison succeeds");
        assert!(comparison.comparable);
        assert_eq!(
            comparison.simulator_evidence_class,
            EvidenceClass::SimulatorLocal
        );
        assert_eq!(
            comparison.vm_evidence_class,
            EvidenceClass::VmSnapshotReplay
        );
        assert!(comparison.summary.contains("not VM replay proof"));
    }

    #[test]
    fn bridge_metadata_reports_identity_mismatches_and_rejects_replay_overclaim() {
        let evidence = sample_simulator_run_evidence().expect("simulator evidence emits");
        let mut vm = sample_vm_replay_bridge_metadata();
        vm.workload.scenario_id = "different-scenario".to_string();
        let comparison = compare_simulator_vm_receipt_bridge(&evidence.receipt.bridge, &vm)
            .expect("bridge comparison succeeds");
        assert!(!comparison.comparable);
        assert!(!comparison.scenario_match);

        let mut bad_vm = sample_vm_replay_bridge_metadata();
        bad_vm.evidence_class = EvidenceClass::SimulatorLocal;
        assert!(validate_vm_replay_bridge_metadata(&bad_vm)
            .expect_err("VM bridge class rejected")
            .message()
            .contains("vm_snapshot_replay"));
    }
}
