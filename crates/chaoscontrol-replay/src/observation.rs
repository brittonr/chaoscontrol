//! Declared, bounded observations on an exact restored guest branch.
//!
//! Even a read-only command perturbs scheduling. These receipts never certify
//! the original history, replay promotion, causality, or release eligibility.

pub use chaoscontrol_protocol::diagnostic::ObservationProfile;
use chaoscontrol_protocol::diagnostic::{
    decode_receipt, encode_request, DiagnosticReceiptStatus, MAX_OBSERVATION_TICKS,
};
use serde::{Deserialize, Serialize};

pub const OBSERVATION_RECEIPT_SCHEMA: &str = "chaoscontrol.observation-receipt.v1";
pub const MAX_OBSERVATION_RECEIPT_BYTES: usize = 16 * 1024;
pub const MAX_ADMITTED_OBSERVATION_PROFILES: usize = 32;
pub const OBSERVATION_NONCLAIM: &str = "local guest-branch observation only; read-only commands may perturb scheduling; not original-history certification, complete capture, causal attribution, source correctness, arbitrary or remote execution, replay promotion, or release eligibility";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationBinding {
    pub session_identity: String,
    pub checkpoint_identity: String,
    pub checkpoint_tick: u64,
    /// Exact admitted moment, which can be later than its restore checkpoint.
    pub selected_tick: u64,
    pub vm_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationReceipt {
    pub schema: String,
    pub binding: ObservationBinding,
    pub profile_identity: String,
    pub request_digest: String,
    pub deadline_tick: u64,
    pub start_tick: u64,
    pub end_tick: u64,
    pub executed_rounds: u64,
    pub stdout_limit: u32,
    pub stderr_limit: u32,
    pub stdout_bytes: u32,
    pub stderr_bytes: u32,
    pub stdout_digest: String,
    pub stderr_digest: String,
    /// Canonical existing diagnostic receipt; no unbounded logs are embedded.
    pub diagnostic_receipt: Option<Vec<u8>>,
    pub output_digest: String,
    pub incomplete: bool,
    /// Always true: even read-only observation changes the guest branch schedule.
    pub counterfactual: bool,
    pub mutates_guest: bool,
    pub certifies_original_history: bool,
    pub nonclaim: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationError {
    Profile(String),
    UnadmittedCommand,
    MissingSession,
    SnapshotMismatch,
    InvalidVm,
    Deadline,
    BusyDiagnosticPort,
    GuestExecution(String),
    Receipt(String),
    Drift,
    Accounting,
    OutputOverflow,
    Overclaim,
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ObservationError {}

fn identity(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Admission is pure and must run before restoring or stepping a VM. Identities
/// come from the operator-reviewed command set also retained in the guest initrd.
// r[impl chaoscontrol.debug_query.guest_observation]
pub fn admit_observation(
    profile: &ObservationProfile,
    admitted_profile_identities: &[String],
) -> Result<String, ObservationError> {
    if admitted_profile_identities.len() > MAX_ADMITTED_OBSERVATION_PROFILES
        || admitted_profile_identities.iter().any(|id| !identity(id))
        || admitted_profile_identities
            .iter()
            .enumerate()
            .any(|(index, id)| admitted_profile_identities[..index].contains(id))
    {
        return Err(ObservationError::Profile(
            "invalid admitted command set".into(),
        ));
    }
    let profile_identity = profile.identity().map_err(ObservationError::Profile)?;
    profile.request().map_err(ObservationError::Profile)?;
    if !admitted_profile_identities.contains(&profile_identity) {
        return Err(ObservationError::UnadmittedCommand);
    }
    Ok(profile_identity)
}

/// Polling transfers a request to in-flight state; it does not make the port
/// idle. This state is retained in exact checkpoints, including identical digests.
fn require_idle_diagnostic_port(
    engine: &chaoscontrol_fault::engine::FaultEngine,
) -> Result<(), ObservationError> {
    if engine.pending_diagnostic_requests() != 0 {
        return Err(ObservationError::BusyDiagnosticPort);
    }
    Ok(())
}

/// Execute at most the declared virtual-tick horizon on a validated exact restore.
/// This never boots, executes a host command, or substitutes retained log state.
/// A deadline without a guest receipt is explicitly incomplete; the private
/// restored branch must not be used to certify the original recording.
pub fn run_observation(
    restored: &mut crate::session::RestoredController,
    vm_index: usize,
    profile: &ObservationProfile,
    admitted_profile_identities: &[String],
) -> Result<ObservationReceipt, ObservationError> {
    admit_observation(profile, admitted_profile_identities)?;
    let binding = ObservationBinding {
        session_identity: restored.session_identity().to_string(),
        checkpoint_identity: restored.checkpoint_identity().to_string(),
        checkpoint_tick: restored.checkpoint_tick(),
        selected_tick: restored.selected_tick(),
        vm_index,
    };
    if !crate::session::valid_identity(&binding.session_identity) {
        return Err(ObservationError::MissingSession);
    }
    if !crate::session::valid_identity(&binding.checkpoint_identity)
        || binding.selected_tick < binding.checkpoint_tick
        || restored.tick() != binding.selected_tick
    {
        return Err(ObservationError::SnapshotMismatch);
    }
    let start_tick = restored.tick();
    let horizon = profile
        .deadline_tick
        .checked_sub(start_tick)
        .ok_or(ObservationError::Deadline)?;
    if horizon == 0 || horizon > MAX_OBSERVATION_TICKS {
        return Err(ObservationError::Deadline);
    }
    if vm_index >= restored.num_vms() {
        return Err(ObservationError::InvalidVm);
    }
    let request = profile.request().map_err(ObservationError::Profile)?;
    let request_digest = *blake3::hash(
        &encode_request(&request).map_err(|e| ObservationError::Profile(format!("{e:?}")))?,
    )
    .as_bytes();
    let engine = restored.vm(vm_index).fault_engine();
    require_idle_diagnostic_port(engine)?;
    let initial_receipts = engine.diagnostic_receipts().count();
    for index in 0..restored.num_vms() {
        restored.vm_mut(index).bind_diagnostic_tick(start_tick);
    }
    restored
        .vm_mut(vm_index)
        .fault_engine_mut()
        .enqueue_diagnostic_request(&request)
        .map_err(|e| ObservationError::GuestExecution(format!("{e:?}")))?;
    let mut diagnostic_receipt = None;
    while restored.tick() < profile.deadline_tick {
        restored
            .step_round()
            .map_err(|e| ObservationError::GuestExecution(e.to_string()))?;
        for bytes in restored
            .vm(vm_index)
            .fault_engine()
            .diagnostic_receipts()
            .skip(initial_receipts)
        {
            let receipt =
                decode_receipt(bytes).map_err(|e| ObservationError::Receipt(format!("{e:?}")))?;
            if receipt.request_digest != request_digest || diagnostic_receipt.is_some() {
                return Err(ObservationError::Drift);
            }
            diagnostic_receipt = Some(bytes.to_vec());
        }
        if diagnostic_receipt.is_some() {
            break;
        }
    }
    let receipt = make_receipt(
        &binding,
        profile,
        start_tick,
        restored.tick(),
        diagnostic_receipt,
    )?;
    validate_observation_receipt(&receipt, &binding, profile)?;
    Ok(receipt)
}

fn make_receipt(
    binding: &ObservationBinding,
    profile: &ObservationProfile,
    start_tick: u64,
    end_tick: u64,
    diagnostic_receipt: Option<Vec<u8>>,
) -> Result<ObservationReceipt, ObservationError> {
    let request = profile.request().map_err(ObservationError::Profile)?;
    let encoded =
        encode_request(&request).map_err(|e| ObservationError::Profile(format!("{e:?}")))?;
    let decoded = diagnostic_receipt
        .as_deref()
        .map(decode_receipt)
        .transpose()
        .map_err(|e| ObservationError::Receipt(format!("{e:?}")))?;
    let empty = blake3::hash(b"").to_hex().to_string();
    Ok(ObservationReceipt {
        schema: OBSERVATION_RECEIPT_SCHEMA.into(),
        binding: binding.clone(),
        profile_identity: profile.identity().map_err(ObservationError::Profile)?,
        request_digest: blake3::hash(&encoded).to_hex().to_string(),
        deadline_tick: profile.deadline_tick,
        start_tick,
        end_tick,
        executed_rounds: end_tick
            .checked_sub(start_tick)
            .ok_or(ObservationError::Accounting)?,
        stdout_limit: profile.stdout_limit,
        stderr_limit: profile.stderr_limit,
        stdout_bytes: decoded.as_ref().map_or(0, |r| r.stdout_bytes),
        stderr_bytes: decoded.as_ref().map_or(0, |r| r.stderr_bytes),
        stdout_digest: decoded.as_ref().map_or_else(
            || empty.clone(),
            |r| blake3::Hash::from(r.stdout_digest).to_hex().to_string(),
        ),
        stderr_digest: decoded.as_ref().map_or_else(
            || empty.clone(),
            |r| blake3::Hash::from(r.stderr_digest).to_hex().to_string(),
        ),
        output_digest: blake3::hash(diagnostic_receipt.as_deref().unwrap_or(b""))
            .to_hex()
            .to_string(),
        incomplete: decoded
            .as_ref()
            .is_none_or(|r| r.truncated || r.status != DiagnosticReceiptStatus::Completed),
        diagnostic_receipt,
        counterfactual: true,
        mutates_guest: profile.mutates_guest,
        certifies_original_history: false,
        nonclaim: OBSERVATION_NONCLAIM.into(),
    })
}

/// Validate against re-admitted current session/checkpoint/profile identities.
/// The caller obtains `binding` from the exact session, never from this receipt.
// r[impl chaoscontrol.debug_query.evidence]
// r[impl chaoscontrol.debug_query.nonclaims]
pub fn validate_observation_receipt(
    receipt: &ObservationReceipt,
    binding: &ObservationBinding,
    profile: &ObservationProfile,
) -> Result<(), ObservationError> {
    profile.validate().map_err(ObservationError::Profile)?;
    if !crate::session::valid_identity(&binding.session_identity) {
        return Err(ObservationError::MissingSession);
    }
    if !crate::session::valid_identity(&binding.checkpoint_identity) || receipt.binding != *binding
    {
        return Err(ObservationError::SnapshotMismatch);
    }
    if receipt.schema != OBSERVATION_RECEIPT_SCHEMA
        || receipt.profile_identity != profile.identity().map_err(ObservationError::Profile)?
        || receipt.deadline_tick != profile.deadline_tick
        || receipt.mutates_guest != profile.mutates_guest
        || receipt.stdout_limit != profile.stdout_limit
        || receipt.stderr_limit != profile.stderr_limit
    {
        return Err(ObservationError::Drift);
    }
    if !receipt.counterfactual
        || receipt.certifies_original_history
        || receipt.nonclaim != OBSERVATION_NONCLAIM
    {
        return Err(ObservationError::Overclaim);
    }
    let horizon = receipt
        .deadline_tick
        .checked_sub(receipt.start_tick)
        .ok_or(ObservationError::Deadline)?;
    if horizon == 0 || horizon > MAX_OBSERVATION_TICKS || receipt.end_tick > receipt.deadline_tick {
        return Err(ObservationError::Deadline);
    }
    if binding.selected_tick < binding.checkpoint_tick
        || receipt.start_tick != binding.selected_tick
        || receipt.end_tick <= receipt.start_tick
        || receipt.end_tick.checked_sub(receipt.start_tick) != Some(receipt.executed_rounds)
    {
        return Err(ObservationError::Accounting);
    }
    if receipt.stdout_bytes > profile.stdout_limit || receipt.stderr_bytes > profile.stderr_limit {
        return Err(ObservationError::OutputOverflow);
    }
    if let Some(bytes) = &receipt.diagnostic_receipt {
        let diagnostic =
            decode_receipt(bytes).map_err(|e| ObservationError::Receipt(format!("{e:?}")))?;
        let request = profile.request().map_err(ObservationError::Profile)?;
        let encoded =
            encode_request(&request).map_err(|e| ObservationError::Profile(format!("{e:?}")))?;
        if diagnostic.request_digest != *blake3::hash(&encoded).as_bytes()
            || diagnostic.executable != profile.executable_blake3
        {
            return Err(ObservationError::Drift);
        }
        if !diagnostic.counterfactual {
            return Err(ObservationError::Overclaim);
        }
    } else if !receipt.incomplete || receipt.end_tick != receipt.deadline_tick {
        return Err(ObservationError::Accounting);
    }
    let expected = make_receipt(
        binding,
        profile,
        receipt.start_tick,
        receipt.end_tick,
        receipt.diagnostic_receipt.clone(),
    )?;
    if expected != *receipt {
        return Err(ObservationError::Accounting);
    }
    if serde_json::to_vec(receipt)
        .map_err(|e| ObservationError::Receipt(e.to_string()))?
        .len()
        > MAX_OBSERVATION_RECEIPT_BYTES
    {
        return Err(ObservationError::Receipt("receipt byte bound".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_protocol::diagnostic::{
        encode_receipt, DiagnosticReceipt, DIAGNOSTIC_RECEIPT_VERSION,
    };

    fn profile() -> ObservationProfile {
        ObservationProfile {
            executable: "init".into(),
            executable_blake3: [7; 32],
            argv: vec!["--observe-shared-state".into()],
            environment: Default::default(),
            working_directory: "data".into(),
            deadline_tick: 5000,
            stdout_limit: 1024,
            stderr_limit: 1024,
            mutates_guest: false,
        }
    }
    fn binding() -> ObservationBinding {
        ObservationBinding {
            session_identity: format!("b3:{}", "1".repeat(64)),
            checkpoint_identity: format!("b3:{}", "2".repeat(64)),
            checkpoint_tick: 1170,
            selected_tick: 1170,
            vm_index: 0,
        }
    }
    fn wire(profile: &ObservationProfile, truncated: bool) -> Vec<u8> {
        let request = encode_request(&profile.request().unwrap()).unwrap();
        encode_receipt(&DiagnosticReceipt {
            version: DIAGNOSTIC_RECEIPT_VERSION,
            request_digest: *blake3::hash(&request).as_bytes(),
            executable: profile.executable_blake3,
            status: DiagnosticReceiptStatus::Completed,
            exit_code: Some(0),
            truncated,
            counterfactual: true,
            stdout_bytes: 10,
            stdout_digest: *blake3::hash(b"committed\n").as_bytes(),
            stderr_bytes: 0,
            stderr_digest: *blake3::hash(b"").as_bytes(),
        })
        .unwrap()
    }
    fn complete() -> (ObservationProfile, ObservationBinding, ObservationReceipt) {
        let profile = profile();
        let binding = binding();
        let receipt =
            make_receipt(&binding, &profile, 1170, 1200, Some(wire(&profile, false))).unwrap();
        (profile, binding, receipt)
    }

    // r[verify chaoscontrol.debug_query.guest_observation]
    // r[verify chaoscontrol.debug_query.evidence]
    // r[verify chaoscontrol.debug_query.fixtures]
    #[test]
    fn admitted_declared_observation_receipt_roundtrips_deterministically() {
        let (profile, binding, receipt) = complete();
        let admitted = vec![profile.identity().unwrap()];
        assert_eq!(admit_observation(&profile, &admitted).unwrap(), admitted[0]);
        validate_observation_receipt(&receipt, &binding, &profile).unwrap();
        assert_eq!(
            receipt,
            make_receipt(&binding, &profile, 1170, 1200, Some(wire(&profile, false))).unwrap()
        );
        assert_eq!(receipt.stdout_bytes, 10);
        assert!(!receipt.incomplete);
        assert!(receipt.counterfactual);
        assert!(!receipt.certifies_original_history);
        assert!(serde_json::to_vec(&receipt).unwrap().len() <= MAX_OBSERVATION_RECEIPT_BYTES);
    }

    #[test]
    fn identical_in_flight_request_blocks_new_observation_after_restore() {
        use chaoscontrol_fault::engine::{EngineConfig, EngineSnapshot, FaultEngine};
        use chaoscontrol_protocol::HypercallPage;
        let profile = profile();
        let request = profile.request().unwrap();
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.enqueue_diagnostic_request(&request).unwrap();
        assert_eq!(
            require_idle_diagnostic_port(&engine),
            Err(ObservationError::BusyDiagnosticPort)
        );
        let mut page = HypercallPage::zeroed();
        assert!(engine.write_diagnostic_request(&mut page).unwrap());
        let snapshot: EngineSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&engine.snapshot()).unwrap()).unwrap();
        let mut restored = FaultEngine::new(EngineConfig::default());
        restored.restore(&snapshot).unwrap();
        // No queued bytes remain, but the old command is still running. The
        // exact same profile must not be enqueued against that active receipt.
        assert!(!restored.write_diagnostic_request(&mut page).unwrap());
        assert_eq!(
            require_idle_diagnostic_port(&restored),
            Err(ObservationError::BusyDiagnosticPort)
        );
        restored
            .record_diagnostic_receipt(&wire(&profile, false))
            .unwrap();
        require_idle_diagnostic_port(&restored).unwrap();
        let initial_receipts = restored.diagnostic_receipts().count();
        assert_eq!(initial_receipts, 1);
        restored.enqueue_diagnostic_request(&request).unwrap();
        assert_eq!(
            restored
                .diagnostic_receipts()
                .skip(initial_receipts)
                .count(),
            0
        );
    }

    // r[verify chaoscontrol.debug_query.moment]
    #[test]
    fn selected_moment_is_distinct_from_its_restore_checkpoint() {
        let profile = profile();
        let mut binding = binding();
        binding.selected_tick = 1190;
        let receipt =
            make_receipt(&binding, &profile, 1190, 1200, Some(wire(&profile, false))).unwrap();
        validate_observation_receipt(&receipt, &binding, &profile).unwrap();
        assert_eq!(receipt.binding.checkpoint_tick, 1170);
        assert_eq!(receipt.start_tick, 1190);
        assert_eq!(receipt.executed_rounds, 10);
        let mislabeled =
            make_receipt(&binding, &profile, 1170, 1200, Some(wire(&profile, false))).unwrap();
        assert_eq!(
            validate_observation_receipt(&mislabeled, &binding, &profile),
            Err(ObservationError::Accounting)
        );
        binding.selected_tick = 1160;
        let reversed =
            make_receipt(&binding, &profile, 1160, 1200, Some(wire(&profile, false))).unwrap();
        assert_eq!(
            validate_observation_receipt(&reversed, &binding, &profile),
            Err(ObservationError::Accounting)
        );
    }

    #[test]
    fn missing_and_unadmitted_commands_fail_pure_admission() {
        let mut profile = profile();
        assert_eq!(
            admit_observation(&profile, &[]),
            Err(ObservationError::UnadmittedCommand)
        );
        profile.executable_blake3 = [0; 32];
        assert!(matches!(
            admit_observation(&profile, &[]),
            Err(ObservationError::Profile(_))
        ));
    }

    #[test]
    fn paths_arguments_and_environment_fail_before_effects() {
        for path in [
            "",
            "/init",
            "../init",
            "bin/../init",
            "bin//init",
            "./init",
            "bin\\init",
        ] {
            let mut profile = profile();
            profile.executable = path.into();
            assert!(profile.request().is_err(), "{path}");
        }
        for path in ["", "/data", "../data", "data/..", "data//child"] {
            let mut profile = profile();
            profile.working_directory = path.into();
            assert!(profile.request().is_err(), "{path}");
        }
        for arg in ["/etc/passwd", "../escape", "--path=foo/../bar", "x\0y"] {
            let mut profile = profile();
            profile.argv = vec![arg.into()];
            assert!(profile.request().is_err(), "{arg}");
        }
        for key in ["PATH", "LD_PRELOAD", "BASH_ENV", "bad-key"] {
            let mut profile = profile();
            profile.environment.insert(key.into(), "x".into());
            assert!(profile.request().is_err(), "{key}");
        }
        let mut profile = profile();
        profile.environment.insert("DATA".into(), "x".repeat(2049));
        assert!(profile.request().is_err());
    }

    #[test]
    fn every_profile_bound_is_enforced() {
        let mut candidate = profile();
        candidate.argv = vec!["x".into(); 17];
        assert!(candidate.request().is_err());
        let mut candidate = profile();
        candidate.argv = vec!["x".repeat(1025)];
        assert!(candidate.request().is_err());
        let mut candidate = profile();
        candidate.argv = vec!["x".repeat(1024); 5];
        assert!(candidate.request().is_err());
        let mut candidate = profile();
        candidate.environment = (0..17).map(|i| (format!("K{i}"), "x".into())).collect();
        assert!(candidate.request().is_err());
        for limit in [0, 65537] {
            let mut candidate = profile();
            candidate.stdout_limit = limit;
            assert!(candidate.request().is_err());
            let mut candidate = profile();
            candidate.stderr_limit = limit;
            assert!(candidate.request().is_err());
        }
        let mut candidate = profile();
        candidate.deadline_tick = 0;
        assert!(candidate.request().is_err());
    }

    #[test]
    fn wire_profile_drift_and_trailing_bytes_fail() {
        let profile = profile();
        let request = profile.request().unwrap();
        assert_eq!(
            chaoscontrol_protocol::diagnostic::decode_request(&encode_request(&request).unwrap())
                .unwrap(),
            request
        );
        let mut drift = request.clone();
        drift.argv.push("unadmitted".into());
        assert!(encode_request(&drift).is_err());
        let mut drift = request.clone();
        drift.executable = [8; 32];
        assert!(encode_request(&drift).is_err());
        let mut drift = request;
        drift.stdin.push(b' ');
        assert!(encode_request(&drift).is_err());
    }

    #[test]
    fn source_snapshot_profile_and_accounting_drift_fail() {
        let (profile, binding, receipt) = complete();
        let mut source = binding.clone();
        source.session_identity = format!("b3:{}", "3".repeat(64));
        assert_eq!(
            validate_observation_receipt(&receipt, &source, &profile),
            Err(ObservationError::SnapshotMismatch)
        );
        let mut source = binding.clone();
        source.checkpoint_identity = format!("b3:{}", "3".repeat(64));
        assert_eq!(
            validate_observation_receipt(&receipt, &source, &profile),
            Err(ObservationError::SnapshotMismatch)
        );
        let mut missing = binding.clone();
        missing.session_identity.clear();
        assert_eq!(
            validate_observation_receipt(&receipt, &missing, &profile),
            Err(ObservationError::MissingSession)
        );
        let mut command = profile.clone();
        command.argv = vec!["changed".into()];
        assert_eq!(
            validate_observation_receipt(&receipt, &binding, &command),
            Err(ObservationError::Drift)
        );
        let mut mismatch = receipt.clone();
        mismatch.executed_rounds += 1;
        assert_eq!(
            validate_observation_receipt(&mismatch, &binding, &profile),
            Err(ObservationError::Accounting)
        );
        let mut mismatch = receipt.clone();
        mismatch.stdout_digest = "0".repeat(64);
        assert_eq!(
            validate_observation_receipt(&mismatch, &binding, &profile),
            Err(ObservationError::Accounting)
        );
        let mut mismatch = receipt;
        mismatch.output_digest = "0".repeat(64);
        assert_eq!(
            validate_observation_receipt(&mismatch, &binding, &profile),
            Err(ObservationError::Accounting)
        );
    }

    #[test]
    fn output_overflow_and_silent_completion_fail() {
        let (profile, binding, receipt) = complete();
        let mut overflow = receipt.clone();
        overflow.stdout_bytes = 1025;
        assert_eq!(
            validate_observation_receipt(&overflow, &binding, &profile),
            Err(ObservationError::OutputOverflow)
        );
        let incomplete =
            make_receipt(&binding, &profile, 1170, 1200, Some(wire(&profile, true))).unwrap();
        assert!(incomplete.incomplete);
        validate_observation_receipt(&incomplete, &binding, &profile).unwrap();
        let mut forged = incomplete;
        forged.incomplete = false;
        assert_eq!(
            validate_observation_receipt(&forged, &binding, &profile),
            Err(ObservationError::Accounting)
        );
        let timed_out = make_receipt(&binding, &profile, 1170, 5000, None).unwrap();
        validate_observation_receipt(&timed_out, &binding, &profile).unwrap();
        let mut forged = timed_out;
        forged.incomplete = false;
        assert_eq!(
            validate_observation_receipt(&forged, &binding, &profile),
            Err(ObservationError::Accounting)
        );
    }

    // r[verify chaoscontrol.debug_query.nonclaims]
    #[test]
    fn mutating_and_read_only_observations_cannot_certify_history() {
        let (mut profile, binding, mut receipt) = complete();
        receipt.counterfactual = false;
        assert_eq!(
            validate_observation_receipt(&receipt, &binding, &profile),
            Err(ObservationError::Overclaim)
        );
        profile.mutates_guest = true;
        let mut receipt =
            make_receipt(&binding, &profile, 1170, 1200, Some(wire(&profile, false))).unwrap();
        validate_observation_receipt(&receipt, &binding, &profile).unwrap();
        receipt.certifies_original_history = true;
        assert_eq!(
            validate_observation_receipt(&receipt, &binding, &profile),
            Err(ObservationError::Overclaim)
        );
    }
}
