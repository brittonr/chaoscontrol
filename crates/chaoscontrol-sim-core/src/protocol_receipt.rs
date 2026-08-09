//! Pure construction and validation for protocol-simulation receipts.
//!
//! The caller supplies canonical history and output bytes. This module binds
//! those bytes but does not assign their protocol meaning or write files.

use crate::protocol_simulation::{
    ProtocolSimulationConfig, ProtocolSimulationEvidenceClass, ProtocolSimulationReceipt,
    PROTOCOL_SIMULATION_CONFIG_SCHEMA, PROTOCOL_SIMULATION_RECEIPT_SCHEMA,
};
use std::fmt;

const CONFIG_DIGEST_DOMAIN: &[u8] = b"chaoscontrol.protocol-simulation.config.v1\0";
const HISTORY_DIGEST_DOMAIN: &[u8] = b"chaoscontrol.protocol-simulation.history.v1\0";
const OUTPUT_DIGEST_DOMAIN: &[u8] = b"chaoscontrol.protocol-simulation.output.v1\0";
const RUN_ID_DOMAIN: &[u8] = b"chaoscontrol.protocol-simulation.run-id.v1\0";
const BLAKE3_PREFIX: &str = "blake3:";
const BLAKE3_HEX_LENGTH: usize = 64;
const RUN_ID_PREFIX: &str = "protocol-run-";

/// Fail-closed receipt construction or validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolReceiptError {
    UnsupportedConfigSchema { found: String },
    UnsupportedReceiptSchema { found: String },
    EmptyField { field: String },
    EmptyArtifactDigests,
    InvalidBound { field: String },
    InvalidDigest { field: String },
    ConfigDigestMismatch { expected: String, found: String },
    FaultScheduleDigestMismatch { expected: String, found: String },
    RunIdMismatch { expected: String, found: String },
}

impl fmt::Display for ProtocolReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProtocolReceiptError {}

/// r[protocol-fault-sim.replay]
/// Build a deterministic receipt from supplied config, history, and output.
pub fn build_protocol_simulation_receipt(
    config: ProtocolSimulationConfig,
    history_bytes: &[u8],
    output_bytes: &[u8],
) -> Result<ProtocolSimulationReceipt, ProtocolReceiptError> {
    let config_digest = protocol_simulation_config_digest(&config)?;
    let fault_schedule_digest = config.schedule.digest.clone();
    let history_digest = framed_digest(HISTORY_DIGEST_DOMAIN, history_bytes);
    let output_digest = framed_digest(OUTPUT_DIGEST_DOMAIN, output_bytes);
    let run_id = derive_run_id(
        &config_digest,
        &fault_schedule_digest,
        &history_digest,
        &output_digest,
    );
    let receipt = ProtocolSimulationReceipt {
        schema: PROTOCOL_SIMULATION_RECEIPT_SCHEMA.to_string(),
        run_id,
        config_digest,
        config,
        fault_schedule_digest,
        history_digest,
        output_digest,
        evidence_class: ProtocolSimulationEvidenceClass::AdapterProtocolSimulation,
    };
    validate_protocol_simulation_receipt(&receipt)?;
    Ok(receipt)
}

/// Validate all self-contained receipt bindings.
///
/// This function verifies receipt structure and internal identity links. It
/// cannot verify history or output bytes that the caller does not supply.
pub fn validate_protocol_simulation_receipt(
    receipt: &ProtocolSimulationReceipt,
) -> Result<(), ProtocolReceiptError> {
    if receipt.schema != PROTOCOL_SIMULATION_RECEIPT_SCHEMA {
        return Err(ProtocolReceiptError::UnsupportedReceiptSchema {
            found: receipt.schema.clone(),
        });
    }
    validate_digest("history_digest", &receipt.history_digest)?;
    validate_digest("output_digest", &receipt.output_digest)?;
    validate_digest("fault_schedule_digest", &receipt.fault_schedule_digest)?;

    let expected_config_digest = protocol_simulation_config_digest(&receipt.config)?;
    if receipt.config_digest != expected_config_digest {
        return Err(ProtocolReceiptError::ConfigDigestMismatch {
            expected: expected_config_digest,
            found: receipt.config_digest.clone(),
        });
    }
    if receipt.fault_schedule_digest != receipt.config.schedule.digest {
        return Err(ProtocolReceiptError::FaultScheduleDigestMismatch {
            expected: receipt.config.schedule.digest.clone(),
            found: receipt.fault_schedule_digest.clone(),
        });
    }

    let expected_run_id = derive_run_id(
        &receipt.config_digest,
        &receipt.fault_schedule_digest,
        &receipt.history_digest,
        &receipt.output_digest,
    );
    if receipt.run_id != expected_run_id {
        return Err(ProtocolReceiptError::RunIdMismatch {
            expected: expected_run_id,
            found: receipt.run_id.clone(),
        });
    }
    Ok(())
}

/// Compute the canonical identity of a validated protocol-simulation config.
pub fn protocol_simulation_config_digest(
    config: &ProtocolSimulationConfig,
) -> Result<String, ProtocolReceiptError> {
    validate_config(config)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(CONFIG_DIGEST_DOMAIN);
    hash_string(&mut hasher, &config.schema);
    hash_u64(&mut hasher, config.seed);
    hash_string(&mut hasher, &config.schedule.schedule_id);
    hash_string(&mut hasher, &config.schedule.digest);
    hash_string(&mut hasher, &config.scheduler.policy_id);
    hash_u64(&mut hasher, config.scheduler.maximum_steps);
    hash_string(&mut hasher, &config.virtual_clock.policy_id);
    hash_u64(&mut hasher, config.virtual_clock.initial_tick);
    hash_u64(&mut hasher, config.virtual_clock.tick_quantum);
    hash_string(&mut hasher, &config.rng.algorithm);
    hash_string(&mut hasher, &config.rng.seed_derivation);
    hash_string(&mut hasher, &config.protocol.protocol_id);
    hash_string(&mut hasher, &config.protocol.protocol_version);
    hash_string(&mut hasher, &config.protocol.adapter_id);
    hash_string(&mut hasher, &config.protocol.adapter_version);
    hash_usize(&mut hasher, config.artifact_digests.len());
    for (artifact_name, digest) in &config.artifact_digests {
        hash_string(&mut hasher, artifact_name);
        hash_string(&mut hasher, digest);
    }
    Ok(format_digest(hasher.finalize()))
}

fn validate_config(config: &ProtocolSimulationConfig) -> Result<(), ProtocolReceiptError> {
    if config.schema != PROTOCOL_SIMULATION_CONFIG_SCHEMA {
        return Err(ProtocolReceiptError::UnsupportedConfigSchema {
            found: config.schema.clone(),
        });
    }
    require_non_empty("schedule.schedule_id", &config.schedule.schedule_id)?;
    validate_digest("schedule.digest", &config.schedule.digest)?;
    require_non_empty("scheduler.policy_id", &config.scheduler.policy_id)?;
    if config.scheduler.maximum_steps == 0 {
        return Err(ProtocolReceiptError::InvalidBound {
            field: "scheduler.maximum_steps".to_string(),
        });
    }
    require_non_empty("virtual_clock.policy_id", &config.virtual_clock.policy_id)?;
    if config.virtual_clock.tick_quantum == 0 {
        return Err(ProtocolReceiptError::InvalidBound {
            field: "virtual_clock.tick_quantum".to_string(),
        });
    }
    require_non_empty("rng.algorithm", &config.rng.algorithm)?;
    require_non_empty("rng.seed_derivation", &config.rng.seed_derivation)?;
    require_non_empty("protocol.protocol_id", &config.protocol.protocol_id)?;
    require_non_empty(
        "protocol.protocol_version",
        &config.protocol.protocol_version,
    )?;
    require_non_empty("protocol.adapter_id", &config.protocol.adapter_id)?;
    require_non_empty("protocol.adapter_version", &config.protocol.adapter_version)?;
    if config.artifact_digests.is_empty() {
        return Err(ProtocolReceiptError::EmptyArtifactDigests);
    }
    for (artifact_name, digest) in &config.artifact_digests {
        require_non_empty("artifact_digests key", artifact_name)?;
        validate_digest(&format!("artifact_digests.{artifact_name}"), digest)?;
    }
    Ok(())
}

fn require_non_empty(field: &str, value: &str) -> Result<(), ProtocolReceiptError> {
    if value.is_empty() {
        return Err(ProtocolReceiptError::EmptyField {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn validate_digest(field: &str, digest: &str) -> Result<(), ProtocolReceiptError> {
    let Some(hex) = digest.strip_prefix(BLAKE3_PREFIX) else {
        return Err(ProtocolReceiptError::InvalidDigest {
            field: field.to_string(),
        });
    };
    let valid = hex.len() == BLAKE3_HEX_LENGTH
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !valid {
        return Err(ProtocolReceiptError::InvalidDigest {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn derive_run_id(
    config_digest: &str,
    fault_schedule_digest: &str,
    history_digest: &str,
    output_digest: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(RUN_ID_DOMAIN);
    hash_string(&mut hasher, config_digest);
    hash_string(&mut hasher, fault_schedule_digest);
    hash_string(&mut hasher, history_digest);
    hash_string(&mut hasher, output_digest);
    format!("{RUN_ID_PREFIX}{}", hasher.finalize().to_hex())
}

fn framed_digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hash_usize(&mut hasher, bytes.len());
    hasher.update(bytes);
    format_digest(hasher.finalize())
}

fn format_digest(digest: blake3::Hash) -> String {
    format!("{BLAKE3_PREFIX}{}", digest.to_hex())
}

fn hash_string(hasher: &mut blake3::Hasher, value: &str) {
    hash_usize(hasher, value.len());
    hasher.update(value.as_bytes());
}

fn hash_u64(hasher: &mut blake3::Hasher, value: u64) {
    hasher.update(&value.to_le_bytes());
}

fn hash_usize(hasher: &mut blake3::Hasher, value: usize) {
    let canonical = u64::try_from(value).expect("usize fits in u64 on supported targets");
    hash_u64(hasher, canonical);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol_simulation::{
        ProtocolIdentity, ProtocolRngPolicy, ProtocolScheduleRef, ProtocolSchedulerPolicy,
        ProtocolVirtualClockPolicy,
    };
    use std::collections::BTreeMap;

    const TEST_SEED: u64 = 31;
    const TEST_MAXIMUM_STEPS: u64 = 64;
    const TEST_INITIAL_TICK: u64 = 3;
    const TEST_TICK_QUANTUM: u64 = 2;
    const TEST_ARTIFACT_COUNT: usize = 2;
    const TEST_DIGEST_HEX: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn supplied_digest() -> String {
        format!("blake3:{TEST_DIGEST_HEX}")
    }

    fn config() -> ProtocolSimulationConfig {
        ProtocolSimulationConfig {
            schema: PROTOCOL_SIMULATION_CONFIG_SCHEMA.to_string(),
            seed: TEST_SEED,
            schedule: ProtocolScheduleRef {
                schedule_id: "lease-fault-schedule".to_string(),
                digest: supplied_digest(),
            },
            scheduler: ProtocolSchedulerPolicy {
                policy_id: "deterministic-ready-order-v1".to_string(),
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
                adapter_id: "lease-test-adapter".to_string(),
                adapter_version: "v1".to_string(),
            },
            artifact_digests: BTreeMap::from([
                ("adapter".to_string(), supplied_digest()),
                ("protocol".to_string(), supplied_digest()),
            ]),
        }
    }

    #[test]
    fn receipt_binds_config_schedule_history_output_and_artifacts() {
        let first =
            build_protocol_simulation_receipt(config(), b"canonical-history", b"canonical-output")
                .expect("receipt builds");
        let second =
            build_protocol_simulation_receipt(config(), b"canonical-history", b"canonical-output")
                .expect("same receipt builds");
        assert_eq!(first, second);
        assert_eq!(first.fault_schedule_digest, supplied_digest());
        assert_eq!(first.config.artifact_digests.len(), TEST_ARTIFACT_COUNT);
        assert!(first.config_digest.starts_with(BLAKE3_PREFIX));
        assert!(first.history_digest.starts_with(BLAKE3_PREFIX));
        assert!(first.output_digest.starts_with(BLAKE3_PREFIX));
        validate_protocol_simulation_receipt(&first).expect("receipt validates");
    }

    #[test]
    fn mutated_bindings_and_invalid_inputs_fail_closed() {
        let receipt = build_protocol_simulation_receipt(config(), b"history", b"output")
            .expect("receipt builds");

        let mut changed_config = receipt.clone();
        changed_config.config.seed = changed_config
            .config
            .seed
            .checked_add(1)
            .expect("seed bound");
        assert!(matches!(
            validate_protocol_simulation_receipt(&changed_config),
            Err(ProtocolReceiptError::ConfigDigestMismatch { .. })
        ));

        let mut changed_schedule = receipt.clone();
        changed_schedule.fault_schedule_digest =
            format!("blake3:{}", "b".repeat(BLAKE3_HEX_LENGTH));
        assert!(matches!(
            validate_protocol_simulation_receipt(&changed_schedule),
            Err(ProtocolReceiptError::FaultScheduleDigestMismatch { .. })
        ));

        let mut invalid_config = config();
        invalid_config.artifact_digests.clear();
        assert_eq!(
            build_protocol_simulation_receipt(invalid_config, b"history", b"output"),
            Err(ProtocolReceiptError::EmptyArtifactDigests)
        );
    }
}
