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
            adapter_version: "register-adapter-v1".to_string(),
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
}
