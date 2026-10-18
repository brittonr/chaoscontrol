//! Pure virtio feature and device-status transition validation.

use super::virtio_types::{PendingCompletionLiveState, TransportViolation, VirtioFailure};

pub const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
pub const VIRTIO_STATUS_DRIVER: u32 = 2;
pub const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
pub const VIRTIO_STATUS_FEATURES_OK: u32 = 8;
pub const VIRTIO_STATUS_DEVICE_NEEDS_RESET: u32 = 64;
pub const VIRTIO_STATUS_FAILED: u32 = 128;
pub const VIRTIO_F_VERSION_1: u64 = 1 << 32;
const DRIVER_STATUS_BITS: u32 = VIRTIO_STATUS_ACKNOWLEDGE
    | VIRTIO_STATUS_DRIVER
    | VIRTIO_STATUS_DRIVER_OK
    | VIRTIO_STATUS_FEATURES_OK
    | VIRTIO_STATUS_FAILED;
const STATUS_ACKNOWLEDGE_DRIVER: u32 = VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER;
const STATUS_FEATURES_ACCEPTED: u32 = STATUS_ACKNOWLEDGE_DRIVER | VIRTIO_STATUS_FEATURES_OK;
const STATUS_DRIVER_ACTIVE: u32 = STATUS_FEATURES_ACCEPTED | VIRTIO_STATUS_DRIVER_OK;

pub fn validate_restored_status(
    status: u32,
    offered_features: u64,
    driver_features: u64,
) -> Result<(), TransportViolation> {
    let status_without_failure =
        status & !(VIRTIO_STATUS_FAILED | VIRTIO_STATUS_DEVICE_NEEDS_RESET);
    let legal_status = matches!(
        status_without_failure,
        0 | VIRTIO_STATUS_ACKNOWLEDGE
            | STATUS_ACKNOWLEDGE_DRIVER
            | STATUS_FEATURES_ACCEPTED
            | STATUS_DRIVER_ACTIVE
    );
    if status & !(DRIVER_STATUS_BITS | VIRTIO_STATUS_DEVICE_NEEDS_RESET) != 0 || !legal_status {
        return Err(TransportViolation::StatusTransition {
            current: 0,
            next: status,
        });
    }
    validate_accepted_features(status, offered_features, driver_features)
}

/// Pure lifecycle eligibility, shared by MMIO notification and polling paths.
pub fn queue_processing_allowed(status: u32, queue_ready: bool) -> bool {
    status & VIRTIO_STATUS_DRIVER_OK != 0
        && status & (VIRTIO_STATUS_DEVICE_NEEDS_RESET | VIRTIO_STATUS_FAILED) == 0
        && queue_ready
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueMutation {
    Size,
    Address,
    Activate,
}

/// Decide queue lifecycle eligibility before configuration or memory effects.
pub fn validate_queue_mutation(
    ready: bool,
    mutation: QueueMutation,
) -> Result<(), TransportViolation> {
    if !ready {
        return Ok(());
    }
    Err(match mutation {
        QueueMutation::Size => TransportViolation::QueueSizeWhileReady,
        QueueMutation::Address => TransportViolation::QueueAddressWhileReady,
        QueueMutation::Activate => TransportViolation::ReadyState,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionTransition {
    Stage {
        head_index: u16,
        written_length: u32,
    },
    BackendStarted,
    EffectsStarted,
    Publish {
        head_index: u16,
        written_length: u32,
    },
}

/// Compute completion-state eligibility without accessing the ring or applying effects.
pub fn plan_completion_transition(
    pending: Option<PendingCompletionLiveState>,
    transition: CompletionTransition,
) -> Result<Option<PendingCompletionLiveState>, VirtioFailure> {
    match transition {
        CompletionTransition::Stage {
            head_index,
            written_length,
        } => {
            if pending.is_some() {
                return Err(VirtioFailure::CompletionState);
            }
            Ok(Some(PendingCompletionLiveState {
                head_index,
                written_length,
                backend_started: false,
                effects_started: false,
            }))
        }
        CompletionTransition::BackendStarted | CompletionTransition::EffectsStarted => {
            let mut pending = pending.ok_or(VirtioFailure::CompletionState)?;
            pending.effects_started = true;
            if transition == CompletionTransition::BackendStarted {
                pending.backend_started = true;
            }
            Ok(Some(pending))
        }
        CompletionTransition::Publish {
            head_index,
            written_length,
        } => {
            let pending = pending.ok_or(VirtioFailure::CompletionState)?;
            if pending.head_index != head_index || pending.written_length != written_length {
                return Err(VirtioFailure::CompletionState);
            }
            Ok(None)
        }
    }
}

pub fn validate_queue_activation(status: u32, value: u32) -> Result<(), TransportViolation> {
    if value != 1 {
        return Err(TransportViolation::ReadyValue { value });
    }
    if status & VIRTIO_STATUS_FEATURES_OK == 0
        || status & (VIRTIO_STATUS_DEVICE_NEEDS_RESET | VIRTIO_STATUS_FAILED) != 0
    {
        return Err(TransportViolation::ReadyState);
    }
    Ok(())
}

pub fn validate_status_transition(
    current: u32,
    next: u32,
    offered_features: u64,
    driver_features: u64,
) -> Result<(), TransportViolation> {
    if next == 0 {
        return Ok(());
    }
    let current_without_failure = current & !VIRTIO_STATUS_FAILED;
    let next_without_failure = next & !VIRTIO_STATUS_FAILED;
    let legal_progress = next_without_failure == current_without_failure
        || matches!(
            (current_without_failure, next_without_failure),
            (0, VIRTIO_STATUS_ACKNOWLEDGE)
                | (VIRTIO_STATUS_ACKNOWLEDGE, STATUS_ACKNOWLEDGE_DRIVER)
                | (STATUS_ACKNOWLEDGE_DRIVER, STATUS_FEATURES_ACCEPTED)
                | (STATUS_FEATURES_ACCEPTED, STATUS_DRIVER_ACTIVE)
        );
    if current & VIRTIO_STATUS_DEVICE_NEEDS_RESET != 0
        || next & !DRIVER_STATUS_BITS != 0
        || next & current != current
        || !legal_progress
    {
        return Err(TransportViolation::StatusTransition { current, next });
    }
    validate_accepted_features(next, offered_features, driver_features)
}

fn validate_accepted_features(
    status: u32,
    offered_features: u64,
    driver_features: u64,
) -> Result<(), TransportViolation> {
    if status & VIRTIO_STATUS_FEATURES_OK == 0 {
        return Ok(());
    }
    if driver_features & !offered_features != 0 {
        return Err(TransportViolation::UnsupportedFeatures {
            requested: driver_features,
            offered: offered_features,
        });
    }
    if driver_features & VIRTIO_F_VERSION_1 == 0 {
        return Err(TransportViolation::ModernFeatureMissing);
    }
    Ok(())
}
