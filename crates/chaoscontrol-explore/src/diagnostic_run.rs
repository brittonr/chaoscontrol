//! Bounded host-side diagnostic operation over the guest supervisor.
//!
//! Boots an admitted supervisor guest until setup completes, queues exactly one
//! admitted diagnostic request for the supervisor to poll, runs a bounded number
//! of ticks, and retains the canonical receipts the guest reported.
//!
//! This is a live guest operation, not replay evidence or an executable-trust
//! claim. It does not prove the diagnostic output is correct.

use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_protocol::diagnostic::{
    decode_receipt, encode_request, DiagnosticReceipt, DiagnosticReceiptStatus, DiagnosticRequest,
    DIAGNOSTIC_PROTOCOL_VERSION,
};
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::VmConfig;
use serde::{Deserialize, Serialize};

/// Session receipt schema identifier.
pub const DIAGNOSTIC_SESSION_SCHEMA: &str = "chaoscontrol.diagnostic-session.v1";
/// Largest accepted executable identity string.
const MAX_DIGEST_TEXT: usize = 80;

/// One bounded diagnostic session configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSessionConfig {
    /// Kernel path.
    pub kernel: String,
    /// Supervisor initrd path.
    pub initrd: String,
    /// VM count.
    pub vms: usize,
    /// Seed.
    pub seed: u64,
    /// Bootstrap budget in ticks.
    pub bootstrap_budget: u64,
    /// Ticks to run after queueing the request.
    pub ticks: u64,
    /// Supervisor-chosen guest executable path.
    pub executable_path: String,
    /// Observed BLAKE3 identity of the guest executable (`blake3:<hex>`).
    pub executable_digest: String,
    /// Request arguments, excluding the executable.
    pub argv: Vec<String>,
    /// Wall-clock timeout for the guest execution.
    pub timeout_ms: u32,
}

/// One canonical receipt reported by the guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticReceiptFact {
    /// BLAKE3 identity of the canonical receipt bytes.
    pub receipt_digest: String,
    /// BLAKE3 identity of the canonical request encoding.
    pub request_digest: String,
    /// BLAKE3 identity of the executed file.
    pub executable: String,
    /// Termination class.
    pub status: String,
    /// Process exit code when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Captured stdout bytes.
    pub stdout_bytes: u32,
    /// Captured stdout digest.
    pub stdout_digest: String,
    /// Captured stderr bytes.
    pub stderr_bytes: u32,
    /// Captured stderr digest.
    pub stderr_digest: String,
    /// Whether either stream hit the capture bound.
    pub truncated: bool,
    /// Receipts are always counterfactual observations.
    pub counterfactual: bool,
}

/// Session receipt with its canonical identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSessionReceipt {
    /// Schema identifier.
    pub schema: String,
    /// Configuration in force.
    pub config: DiagnosticSessionConfig,
    /// Canonical request digest.
    pub request_digest: String,
    /// Receipts reported by the guest.
    pub receipts: Vec<DiagnosticReceiptFact>,
    /// Requests still queued at the end of the run.
    pub pending_requests: usize,
    /// Per-VM count of reported receipts.
    pub receipts_by_vm: Vec<usize>,
    /// BLAKE3 identity of the canonical receipt body (excluding this field).
    pub session_digest: String,
    /// Explicit boundaries.
    pub non_claims: Vec<String>,
}

/// Diagnostic session failure.
#[derive(Debug)]
pub enum DiagnosticSessionError {
    /// Configuration is invalid.
    Config(String),
    /// The VM could not be created or run.
    Vm(String),
    /// The request could not be admitted or decoded.
    Protocol(String),
    /// The receipt could not be encoded.
    Receipt(String),
}

impl core::fmt::Display for DiagnosticSessionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Config(message) => write!(formatter, "diagnostic config: {message}"),
            Self::Vm(message) => write!(formatter, "diagnostic VM: {message}"),
            Self::Protocol(message) => write!(formatter, "diagnostic protocol: {message}"),
            Self::Receipt(message) => write!(formatter, "diagnostic receipt: {message}"),
        }
    }
}

impl std::error::Error for DiagnosticSessionError {}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    text
}

/// Parse a `blake3:<64 hex>` identity.
pub fn parse_digest(value: &str) -> Result<[u8; 32], DiagnosticSessionError> {
    if value.len() > MAX_DIGEST_TEXT {
        return Err(DiagnosticSessionError::Config(
            "executable digest is too long".to_string(),
        ));
    }
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(DiagnosticSessionError::Config(
            "executable digest must use blake3:".to_string(),
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DiagnosticSessionError::Config(
            "executable digest must be 64 lowercase hex characters".to_string(),
        ));
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(|_| {
            DiagnosticSessionError::Config("executable digest is not lowercase hex".to_string())
        })?;
    }
    Ok(digest)
}

fn receipt_fact(bytes: &[u8]) -> Result<DiagnosticReceiptFact, DiagnosticSessionError> {
    let receipt: DiagnosticReceipt = decode_receipt(bytes)
        .map_err(|error| DiagnosticSessionError::Protocol(format!("{error:?}")))?;
    Ok(DiagnosticReceiptFact {
        receipt_digest: format!("blake3:{}", hex(&blake3::hash(bytes).as_bytes()[..])),
        request_digest: format!("blake3:{}", hex(&receipt.request_digest)),
        executable: format!("blake3:{}", hex(&receipt.executable)),
        status: match receipt.status {
            DiagnosticReceiptStatus::Completed => "completed".to_string(),
            DiagnosticReceiptStatus::TimedOut => "timed-out".to_string(),
        },
        exit_code: receipt.exit_code,
        stdout_bytes: receipt.stdout_bytes,
        stdout_digest: format!("blake3:{}", hex(&receipt.stdout_digest)),
        stderr_bytes: receipt.stderr_bytes,
        stderr_digest: format!("blake3:{}", hex(&receipt.stderr_digest)),
        truncated: receipt.truncated,
        counterfactual: receipt.counterfactual,
    })
}

fn body_digest(receipt: &DiagnosticSessionReceipt) -> Result<String, DiagnosticSessionError> {
    let mut body = receipt.clone();
    body.session_digest.clear();
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| DiagnosticSessionError::Receipt(format!("{error}")))?;
    Ok(format!("blake3:{}", hex(blake3::hash(&bytes).as_bytes())))
}

/// Run one bounded diagnostic session and return its receipt.
pub fn run_session(
    config: &DiagnosticSessionConfig,
) -> Result<DiagnosticSessionReceipt, DiagnosticSessionError> {
    if config.vms == 0 || config.vms > 256 {
        return Err(DiagnosticSessionError::Config(
            "VM count must be within 1..=256".to_string(),
        ));
    }
    if config.ticks == 0 {
        return Err(DiagnosticSessionError::Config(
            "tick bound must be nonzero".to_string(),
        ));
    }
    if config.executable_path.is_empty() || !config.executable_path.starts_with('/') {
        return Err(DiagnosticSessionError::Config(
            "executable path must be absolute".to_string(),
        ));
    }
    let request = DiagnosticRequest {
        version: DIAGNOSTIC_PROTOCOL_VERSION,
        executable: parse_digest(&config.executable_digest)?,
        argv: config.argv.clone(),
        stdin: Vec::new(),
        timeout_ms: config.timeout_ms,
    };
    let encoded = encode_request(&request)
        .map_err(|error| DiagnosticSessionError::Protocol(format!("{error:?}")))?;
    let request_digest = format!("blake3:{}", hex(blake3::hash(&encoded).as_bytes()));

    let mut vm_config = VmConfig::default();
    crate::explorer::admit_guest_determinism_policy(&mut vm_config);
    let simulation = SimulationConfig {
        num_vms: config.vms,
        vm_config,
        kernel_path: config.kernel.clone(),
        initrd_path: Some(config.initrd.clone()),
        seed: config.seed,
        quantum: 100,
        schedule: FaultSchedule::new(),
        disk_image_path: None,
        bootstrap_budget: Some(config.bootstrap_budget),
        base_core: None,
        dlog_dir: None,
        random_faults: false,
    };
    let mut controller = SimulationController::new(simulation)
        .map_err(|error| DiagnosticSessionError::Vm(error.to_string()))?;
    controller
        .run_until_setup_complete(config.bootstrap_budget)
        .map_err(|error| DiagnosticSessionError::Vm(error.to_string()))?;
    let engine = controller
        .vm_slot_mut(0)
        .ok_or_else(|| DiagnosticSessionError::Vm("missing VM 0".to_string()))?
        .vm
        .fault_engine_mut();
    engine
        .enqueue_diagnostic_request(&request)
        .map_err(|error| DiagnosticSessionError::Protocol(format!("{error:?}")))?;
    controller
        .run(config.ticks)
        .map_err(|error| DiagnosticSessionError::Vm(error.to_string()))?;

    let mut receipts = Vec::new();
    let mut receipts_by_vm = Vec::new();
    let mut pending_requests = 0;
    for index in 0..controller.num_vms() {
        let engine = controller
            .vm_slot(index)
            .ok_or_else(|| DiagnosticSessionError::Vm(format!("missing VM {index}")))?
            .vm
            .fault_engine();
        let mut vm_receipts = 0;
        for bytes in engine.diagnostic_receipts() {
            receipts.push(receipt_fact(bytes)?);
            vm_receipts += 1;
        }
        receipts_by_vm.push(vm_receipts);
        pending_requests += engine.pending_diagnostic_requests();
    }

    let mut receipt = DiagnosticSessionReceipt {
        schema: DIAGNOSTIC_SESSION_SCHEMA.to_string(),
        config: config.clone(),
        request_digest,
        receipts,
        pending_requests,
        receipts_by_vm,
        session_digest: String::new(),
        non_claims: vec![
            "A diagnostic receipt reports bounded execution facts, not executable trust."
                .to_string(),
            "Captured output digests do not prove output correctness.".to_string(),
            "The operation is counterfactual and does not change replay or assertion verdicts."
                .to_string(),
            "No guest isolation, sandbox, or release-eligibility claim follows.".to_string(),
        ],
    };
    receipt.session_digest = body_digest(&receipt)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DiagnosticSessionConfig {
        DiagnosticSessionConfig {
            kernel: "kernel".into(),
            initrd: "initrd".into(),
            vms: 1,
            seed: 42,
            bootstrap_budget: 100,
            ticks: 10,
            executable_path: "/init".into(),
            executable_digest: format!("blake3:{}", "a".repeat(64)),
            argv: vec!["--fixture-worker".into()],
            timeout_ms: 1000,
        }
    }

    #[test]
    fn digests_are_admitted_exactly() {
        assert_eq!(
            parse_digest(&config().executable_digest).unwrap(),
            [0xaa; 32]
        );
        for value in [
            "".to_string(),
            "sha256:".to_string() + &"a".repeat(64),
            "blake3:".to_string() + &"a".repeat(63),
            "blake3:".to_string() + &"A".repeat(64),
            "blake3:".to_string() + &"g".repeat(64),
        ] {
            assert!(parse_digest(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn invalid_session_bounds_are_rejected() {
        let mut invalid = config();
        invalid.vms = 0;
        assert!(run_session(&invalid).is_err());
        let mut invalid = config();
        invalid.ticks = 0;
        assert!(run_session(&invalid).is_err());
        let mut invalid = config();
        invalid.executable_path = "init".into();
        assert!(run_session(&invalid).is_err());
    }

    #[test]
    fn receipt_digest_is_stable_and_field_sensitive() {
        let mut receipt = DiagnosticSessionReceipt {
            schema: DIAGNOSTIC_SESSION_SCHEMA.to_string(),
            config: config(),
            request_digest: "blake3:req".into(),
            receipts: Vec::new(),
            pending_requests: 0,
            receipts_by_vm: vec![0],
            session_digest: String::new(),
            non_claims: vec!["claim".into()],
        };
        receipt.session_digest = body_digest(&receipt).unwrap();
        let first = receipt.session_digest.clone();
        receipt.session_digest = body_digest(&receipt).unwrap();
        assert_eq!(first, receipt.session_digest);
        receipt.pending_requests = 1;
        assert_ne!(first, body_digest(&receipt).unwrap());
    }

    #[test]
    fn invalid_request_arguments_are_rejected_before_boot() {
        let mut invalid = config();
        invalid.argv = vec!["a".repeat(2048)];
        assert!(matches!(
            run_session(&invalid),
            Err(DiagnosticSessionError::Protocol(_))
        ));
    }
}
