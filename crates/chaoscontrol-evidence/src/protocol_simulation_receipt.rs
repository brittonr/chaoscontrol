//! Filesystem shell for adapter-based protocol-simulation receipts.

use crate::replay_readiness_publication::write_bytes;
use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_sim_core::{
    build_protocol_simulation_receipt, validate_protocol_simulation_receipt,
    ProtocolSimulationConfig, ProtocolSimulationReceipt,
};
use std::path::Path;

const MAX_PROTOCOL_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;

/// Build, validate, and write one protocol-simulation receipt as JSON.
pub fn emit_protocol_simulation_receipt_path(
    output_path: impl AsRef<Path>,
    config: ProtocolSimulationConfig,
    history_bytes: &[u8],
    output_bytes: &[u8],
) -> EvidenceResult<ProtocolSimulationReceipt> {
    let receipt = build_protocol_simulation_receipt(config, history_bytes, output_bytes)
        .map_err(protocol_receipt_error)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    write_bytes(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt)
}

/// Read and validate the self-contained bindings in one receipt file.
pub fn validate_protocol_simulation_receipt_path(
    receipt_path: impl AsRef<Path>,
) -> EvidenceResult<ProtocolSimulationReceipt> {
    let bytes = crate::bounded_file::read_bounded_regular_bytes(
        receipt_path.as_ref(),
        MAX_PROTOCOL_RECEIPT_BYTES,
    )?;
    let receipt: ProtocolSimulationReceipt = serde_json::from_slice(&bytes)?;
    validate_protocol_simulation_receipt(&receipt).map_err(protocol_receipt_error)?;
    Ok(receipt)
}

fn protocol_receipt_error(error: impl std::fmt::Display) -> EvidenceError {
    EvidenceError::new(format!("invalid protocol-simulation receipt: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_validation_rejects_symlinks_and_fifos() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        std::fs::write(&target, b"{}").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(validate_protocol_simulation_receipt_path(&link).is_err());

        let fifo = root.path().join("receipt.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(validate_protocol_simulation_receipt_path(&fifo).is_err());
    }
    use chaoscontrol_sim_core::{
        ProtocolIdentity, ProtocolRngPolicy, ProtocolScheduleRef, ProtocolSchedulerPolicy,
        ProtocolVirtualClockPolicy, PROTOCOL_SIMULATION_CONFIG_SCHEMA,
    };
    use std::collections::BTreeMap;

    const TEST_SEED: u64 = 37;
    const TEST_MAXIMUM_STEPS: u64 = 32;
    const TEST_TICK_QUANTUM: u64 = 1;
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
                schedule_id: "partition-recovery".to_string(),
                digest: digest(),
            },
            scheduler: ProtocolSchedulerPolicy {
                policy_id: "deterministic-ready-order-v1".to_string(),
                maximum_steps: TEST_MAXIMUM_STEPS,
            },
            virtual_clock: ProtocolVirtualClockPolicy {
                policy_id: "simulation-ticks-v1".to_string(),
                initial_tick: 0,
                tick_quantum: TEST_TICK_QUANTUM,
            },
            rng: ProtocolRngPolicy {
                algorithm: "chacha20-v1".to_string(),
                seed_derivation: "config-seed-v1".to_string(),
            },
            protocol: ProtocolIdentity {
                protocol_id: "replicated-lease".to_string(),
                protocol_version: "v1".to_string(),
                adapter_id: "replicated-lease-test-adapter".to_string(),
                adapter_version: "v1".to_string(),
            },
            artifact_digests: BTreeMap::from([("adapter".to_string(), digest())]),
        }
    }

    #[test]
    fn shell_emits_and_reads_the_same_validated_receipt() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("nested/protocol-receipt.json");
        let emitted = emit_protocol_simulation_receipt_path(
            &path,
            config(),
            b"canonical-history",
            b"canonical-output",
        )
        .expect("receipt emits");
        let validated =
            validate_protocol_simulation_receipt_path(&path).expect("written receipt validates");
        assert_eq!(validated, emitted);
        assert_eq!(validated.config.schedule.digest, digest());
    }

    #[test]
    fn shell_rejects_a_receipt_with_a_mutated_config_binding() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("protocol-receipt.json");
        let receipt = emit_protocol_simulation_receipt_path(
            &path,
            config(),
            b"canonical-history",
            b"canonical-output",
        )
        .expect("receipt emits");
        let mut value = serde_json::to_value(receipt).expect("receipt JSON value");
        value["config"]["seed"] = serde_json::json!(TEST_SEED + 1);
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&value).expect("receipt JSON"),
        )
        .expect("mutated receipt writes");

        let error = validate_protocol_simulation_receipt_path(&path)
            .expect_err("mutated config binding is rejected");
        assert!(error.message().contains("ConfigDigestMismatch"));
    }
}
