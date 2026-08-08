//! Pure virtio feature and device-status transition validation.

use super::virtio_types::TransportViolation;

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
    let status_without_failure = status & !VIRTIO_STATUS_FAILED;
    let legal_status = matches!(
        status_without_failure,
        0 | VIRTIO_STATUS_ACKNOWLEDGE
            | STATUS_ACKNOWLEDGE_DRIVER
            | STATUS_FEATURES_ACCEPTED
            | STATUS_DRIVER_ACTIVE
    );
    if status & VIRTIO_STATUS_DEVICE_NEEDS_RESET != 0
        || status & !DRIVER_STATUS_BITS != 0
        || !legal_status
    {
        return Err(TransportViolation::StatusTransition {
            current: 0,
            next: status,
        });
    }
    validate_accepted_features(status, offered_features, driver_features)
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
