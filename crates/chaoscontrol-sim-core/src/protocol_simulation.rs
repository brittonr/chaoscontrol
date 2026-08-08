//! Pure data-transfer types for adapter-based protocol simulation.
//!
//! These types bind deterministic run inputs and receipt facts. They do not
//! read configuration, execute a protocol, write receipts, or claim VM replay.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema for an admitted adapter-based protocol-simulation run configuration.
pub const PROTOCOL_SIMULATION_CONFIG_SCHEMA: &str = "chaoscontrol.protocol-simulation-config.v1";
/// Schema for a runtime-derived adapter-based protocol-simulation receipt.
pub const PROTOCOL_SIMULATION_RECEIPT_SCHEMA: &str = "chaoscontrol.protocol-simulation-receipt.v1";

/// Evidence class for this rail. It is separate from VM and in-process evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolSimulationEvidenceClass {
    #[serde(rename = "adapter-protocol-simulation")]
    AdapterProtocolSimulation,
}

/// r[protocol-fault-sim.contract]
/// r[protocol-fault-sim.contract.config]
/// Complete deterministic input binding for one protocol-simulation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolSimulationConfig {
    pub schema: String,
    pub seed: u64,
    pub schedule: ProtocolScheduleRef,
    pub scheduler: ProtocolSchedulerPolicy,
    pub virtual_clock: ProtocolVirtualClockPolicy,
    pub rng: ProtocolRngPolicy,
    pub protocol: ProtocolIdentity,
    pub artifact_digests: BTreeMap<String, String>,
}

/// Identity of the exact deterministic schedule supplied to the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolScheduleRef {
    pub schedule_id: String,
    pub digest: String,
}

/// Deterministic scheduler policy selected for the protocol adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolSchedulerPolicy {
    pub policy_id: String,
    pub maximum_steps: u64,
}

/// Virtual clock policy selected for the protocol adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolVirtualClockPolicy {
    pub policy_id: String,
    pub initial_tick: u64,
    pub tick_quantum: u64,
}

/// Deterministic random-number policy selected for the protocol adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRngPolicy {
    pub algorithm: String,
    pub seed_derivation: String,
}

/// Identity of the protocol and the adapter that supplies its transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolIdentity {
    pub protocol_id: String,
    pub protocol_version: String,
    pub adapter_id: String,
    pub adapter_version: String,
}

/// Runtime-derived receipt facts for one bounded protocol-simulation run.
///
/// The shell emits this DTO. The embedded configuration makes the seed,
/// schedule, clock, RNG, protocol, adapter, and artifact bindings explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolSimulationReceipt {
    pub schema: String,
    pub run_id: String,
    pub config_digest: String,
    pub config: ProtocolSimulationConfig,
    pub history_digest: String,
    pub output_digest: String,
    pub evidence_class: ProtocolSimulationEvidenceClass,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u64 = 41;
    const TEST_MAXIMUM_STEPS: u64 = 128;
    const TEST_INITIAL_TICK: u64 = 7;
    const TEST_TICK_QUANTUM: u64 = 3;
    const TEST_ARTIFACT_COUNT: usize = 2;
    const TEST_DIGEST_HEX: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn digest() -> String {
        format!("blake3:{TEST_DIGEST_HEX}")
    }

    fn config() -> ProtocolSimulationConfig {
        ProtocolSimulationConfig {
            schema: PROTOCOL_SIMULATION_CONFIG_SCHEMA.to_string(),
            seed: TEST_SEED,
            schedule: ProtocolScheduleRef {
                schedule_id: "ownership-reacquisition".to_string(),
                digest: digest(),
            },
            scheduler: ProtocolSchedulerPolicy {
                policy_id: "deterministic-round-robin-v1".to_string(),
                maximum_steps: TEST_MAXIMUM_STEPS,
            },
            virtual_clock: ProtocolVirtualClockPolicy {
                policy_id: "simulation-ticks-v1".to_string(),
                initial_tick: TEST_INITIAL_TICK,
                tick_quantum: TEST_TICK_QUANTUM,
            },
            rng: ProtocolRngPolicy {
                algorithm: "chacha20-v1".to_string(),
                seed_derivation: "config-seed-v1".to_string(),
            },
            protocol: ProtocolIdentity {
                protocol_id: "lease-replication".to_string(),
                protocol_version: "v1".to_string(),
                adapter_id: "lease-replication-test-adapter".to_string(),
                adapter_version: "v1".to_string(),
            },
            artifact_digests: BTreeMap::from([
                ("adapter".to_string(), digest()),
                ("protocol".to_string(), digest()),
            ]),
        }
    }

    fn receipt() -> ProtocolSimulationReceipt {
        ProtocolSimulationReceipt {
            schema: PROTOCOL_SIMULATION_RECEIPT_SCHEMA.to_string(),
            run_id: "protocol-run-1".to_string(),
            config_digest: digest(),
            config: config(),
            history_digest: digest(),
            output_digest: digest(),
            evidence_class: ProtocolSimulationEvidenceClass::AdapterProtocolSimulation,
        }
    }

    #[test]
    fn config_and_receipt_round_trip_without_losing_bindings() {
        let expected_config = config();
        let encoded_config = serde_json::to_vec(&expected_config).expect("encode config");
        let decoded_config: ProtocolSimulationConfig =
            serde_json::from_slice(&encoded_config).expect("decode config");
        assert_eq!(decoded_config, expected_config);

        let expected_receipt = receipt();
        let encoded_receipt = serde_json::to_vec(&expected_receipt).expect("encode receipt");
        let decoded_receipt: ProtocolSimulationReceipt =
            serde_json::from_slice(&encoded_receipt).expect("decode receipt");
        assert_eq!(decoded_receipt, expected_receipt);
        assert_eq!(decoded_receipt.config.seed, TEST_SEED);
        assert_eq!(decoded_receipt.config.schedule.digest, digest());
        assert_eq!(
            decoded_receipt.config.artifact_digests.len(),
            TEST_ARTIFACT_COUNT
        );
    }

    #[test]
    fn unknown_missing_and_mistyped_fields_fail_closed() {
        let mut unknown = serde_json::to_value(config()).expect("config value");
        unknown
            .as_object_mut()
            .expect("config object")
            .insert("host_clock".to_string(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ProtocolSimulationConfig>(unknown).is_err());

        let mut missing = serde_json::to_value(receipt()).expect("receipt value");
        missing
            .as_object_mut()
            .expect("receipt object")
            .remove("history_digest");
        assert!(serde_json::from_value::<ProtocolSimulationReceipt>(missing).is_err());

        let mut mistyped = serde_json::to_value(config()).expect("config value");
        mistyped["rng"]["algorithm"] = serde_json::Value::Bool(false);
        assert!(serde_json::from_value::<ProtocolSimulationConfig>(mistyped).is_err());

        let mut overclaim = serde_json::to_value(receipt()).expect("receipt value");
        overclaim["evidence_class"] = serde_json::Value::String("vm_snapshot_replay".to_string());
        assert!(serde_json::from_value::<ProtocolSimulationReceipt>(overclaim).is_err());
    }
}
