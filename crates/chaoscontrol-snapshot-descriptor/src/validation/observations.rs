#![allow(
    non_trait_imports,
    reason = "receipt and locator validators compose one closed set of descriptor observation DTOs and bounds"
)]

use crate::model::{
    DigestAlgorithm, TaggedDigest, EXACT_SNAPSHOT_PROFILE, MAX_CONTINUATION_STEPS,
    MAX_LOCATOR_BYTES, MAX_LOCATOR_HINTS, MAX_RESTORE_PHASES, RESTORE_NON_CLAIMS,
};
use crate::observations::{
    ConsumerSnapshotReference, LocatorSidecar, PhaseStatus, RestoreReceipt, REQUIRED_RESTORE_PHASES,
};
use crate::validation::descriptor::{validate_content, validate_digest, validate_text};
use crate::validation::DescriptorError;

// r[impl chaoscontrol.snapshot_descriptor.restore_receipt]
pub fn validate_restore_receipt(receipt: &RestoreReceipt) -> Result<(), DescriptorError> {
    validate_blake3_id("descriptor-id", &receipt.descriptor_id)?;
    validate_blake3_id("destination-id", &receipt.destination_id)?;
    validate_blake3_id("preflight-id", &receipt.preflight_id)?;
    let required_non_claims = RESTORE_NON_CLAIMS.map(str::to_string).to_vec();
    if receipt.non_claims != required_non_claims {
        return invalid(
            "restore-non-claims",
            "restore non-claims are missing or reordered",
        );
    }
    if receipt.phases.len() > MAX_RESTORE_PHASES {
        return invalid("restore-phases", "restore phase bound exceeded");
    }
    for (index, observation) in receipt.phases.iter().enumerate() {
        if REQUIRED_RESTORE_PHASES.get(index) != Some(&observation.phase) {
            return invalid("restore-phases", "restore phases are missing or reordered");
        }
        if let Some(diagnostic) = &observation.diagnostic {
            validate_text("restore-diagnostic", diagnostic)?;
        }
        if observation.status == PhaseStatus::Failed && observation.diagnostic.is_none() {
            return invalid(
                "restore-diagnostic",
                "failed restore phase lacks a diagnostic",
            );
        }
    }
    let failure_index = receipt
        .phases
        .iter()
        .position(|phase| phase.status == PhaseStatus::Failed);
    if let Some(index) = failure_index {
        if index + 1 != receipt.phases.len() {
            return invalid(
                "restore-phases",
                "observations continue after the first failure",
            );
        }
        if receipt.completed {
            return invalid("restore-completion", "failed restore is marked complete");
        }
        if receipt.mutation_started && !receipt.poisoned {
            return invalid("restore-poison", "post-mutation failure omits poison state");
        }
    }
    if !receipt.mutation_started && receipt.poisoned {
        return invalid(
            "restore-poison",
            "pre-mutation denial cannot poison a destination",
        );
    }
    if receipt.completed {
        validate_completed_restore(receipt)?;
    } else if failure_index.is_none() && !receipt.phases.is_empty() {
        return invalid(
            "restore-completion",
            "incomplete restore lacks a terminal failure",
        );
    }
    Ok(())
}

// r[impl chaoscontrol.snapshot_descriptor.locator_boundary]
pub fn validate_locator_sidecar(sidecar: &LocatorSidecar) -> Result<(), DescriptorError> {
    validate_blake3_id("descriptor-id", &sidecar.descriptor_id)?;
    if sidecar.hints.is_empty() || sidecar.hints.len() > MAX_LOCATOR_HINTS {
        return invalid("locator-hints", "locator hints are empty or oversized");
    }
    if sidecar.hints.windows(2).any(|pair| pair[0] >= pair[1]) {
        return invalid("locator-hints", "locator hints are duplicated or unordered");
    }
    for hint in &sidecar.hints {
        if hint.locator.is_empty()
            || hint.locator.len() > MAX_LOCATOR_BYTES
            || hint.locator.chars().any(char::is_control)
        {
            return invalid(
                "locator-hint",
                "locator hint is empty, oversized, or contains controls",
            );
        }
    }
    Ok(())
}

// r[impl chaoscontrol.snapshot_descriptor.consumer_contract]
pub fn validate_consumer_reference(
    reference: &ConsumerSnapshotReference,
) -> Result<(), DescriptorError> {
    validate_blake3_id("descriptor-id", &reference.descriptor_id)?;
    validate_blake3_id("preflight-id", &reference.preflight_id)?;
    if reference.completeness_profile != EXACT_SNAPSHOT_PROFILE {
        return invalid(
            "consumer-profile",
            "consumer reference uses an unsupported profile",
        );
    }
    validate_content(&reference.logical_payload)?;
    if reference.closure_members.is_empty()
        || reference
            .closure_members
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return invalid(
            "consumer-closure",
            "consumer closure refs are empty, duplicated, or unordered",
        );
    }
    for member in &reference.closure_members {
        validate_content(member)?;
    }
    if !reference.disallowed_claims.is_empty() {
        return invalid(
            "consumer-authority-overreach",
            "consumer reference claims branch, merge, authority, promotion, or release meaning",
        );
    }
    Ok(())
}

fn validate_completed_restore(receipt: &RestoreReceipt) -> Result<(), DescriptorError> {
    if !receipt.materialized
        || !receipt.mutation_started
        || receipt.poisoned
        || receipt.phases.len() != REQUIRED_RESTORE_PHASES.len()
        || receipt
            .phases
            .iter()
            .any(|phase| phase.status != PhaseStatus::Succeeded)
    {
        return invalid(
            "restore-completion",
            "completed restore has incomplete or failed observations",
        );
    }
    let continuation = receipt.continuation.as_ref().ok_or_else(|| {
        DescriptorError::new(
            "restore-continuation",
            "completed restore lacks continuation evidence".into(),
        )
    })?;
    if continuation.checked_steps == 0
        || continuation.checked_steps > MAX_CONTINUATION_STEPS
        || !continuation.deterministic_trace_matches
    {
        return invalid(
            "restore-continuation",
            "continuation observation is invalid or out of bounds",
        );
    }
    Ok(())
}

fn validate_blake3_id(field: &'static str, identity: &TaggedDigest) -> Result<(), DescriptorError> {
    validate_digest(identity)?;
    if identity.algorithm != DigestAlgorithm::Blake3 {
        return invalid(field, "identity must use BLAKE3");
    }
    Ok(())
}

fn invalid<T>(code: &'static str, detail: impl Into<String>) -> Result<T, DescriptorError> {
    Err(DescriptorError::new(code, detail.into()))
}
