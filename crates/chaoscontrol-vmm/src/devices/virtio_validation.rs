//! Pure queue geometry, memory containment, progress, and status validation.

pub use super::virtio_status::{
    validate_restored_status, validate_status_transition, VIRTIO_F_VERSION_1,
    VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DEVICE_NEEDS_RESET, VIRTIO_STATUS_DRIVER,
    VIRTIO_STATUS_DRIVER_OK, VIRTIO_STATUS_FAILED, VIRTIO_STATUS_FEATURES_OK,
};
use super::virtio_types::{QueueViolation, VirtioLimits};

pub const DESCRIPTOR_BYTES: u64 = 16;
pub const DESCRIPTOR_ALIGNMENT: u64 = 16;
pub const AVAILABLE_HEADER_BYTES: u64 = 4;
pub const AVAILABLE_ELEMENT_BYTES: u64 = 2;
pub const AVAILABLE_ALIGNMENT: u64 = 2;
pub const USED_HEADER_BYTES: u64 = 4;
pub const USED_ELEMENT_BYTES: u64 = 8;
pub const USED_ALIGNMENT: u64 = 4;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryRegion {
    pub start: u64,
    pub length: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckedRange {
    start: u64,
    end: u64,
}

impl CheckedRange {
    pub fn start(self) -> u64 {
        self.start
    }

    pub fn end(self) -> u64 {
        self.end
    }

    pub fn length(self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawQueueConfig {
    pub size: u32,
    pub descriptor_address: u64,
    pub driver_address: u64,
    pub device_address: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedQueueConfig {
    size: u16,
    descriptors: CheckedRange,
    available: CheckedRange,
    used: CheckedRange,
}

impl ValidatedQueueConfig {
    pub fn size(self) -> u16 {
        self.size
    }

    pub fn descriptor_range(self) -> CheckedRange {
        self.descriptors
    }

    pub fn available_range(self) -> CheckedRange {
        self.available
    }

    pub fn used_range(self) -> CheckedRange {
        self.used
    }
}

pub fn checked_range(start: u64, length: u64) -> Result<CheckedRange, QueueViolation> {
    let end = start
        .checked_add(length)
        .ok_or(QueueViolation::AddressOverflow)?;
    Ok(CheckedRange { start, end })
}

pub fn range_is_contained(regions: &[MemoryRegion], range: CheckedRange) -> bool {
    if range.start == range.end {
        return true;
    }
    let mut covered = range.start;
    for region in regions {
        let Ok(region_range) = checked_range(region.start, region.length) else {
            return false;
        };
        if region_range.end <= covered {
            continue;
        }
        if region_range.start > covered {
            return false;
        }
        covered = covered.max(region_range.end);
        if covered >= range.end {
            return true;
        }
    }
    false
}

pub fn validate_queue_size(
    raw: u32,
    offered_maximum: u16,
    limits: VirtioLimits,
) -> Result<u16, QueueViolation> {
    let size = u16::try_from(raw).map_err(|_| QueueViolation::SizeWidth { value: raw })?;
    if size == 0 {
        return Err(QueueViolation::ZeroSize);
    }
    if !size.is_power_of_two() {
        return Err(QueueViolation::SizeNotPowerOfTwo { value: size });
    }
    let maximum = offered_maximum.min(limits.max_queue_size);
    if size > maximum {
        return Err(QueueViolation::SizeAboveMaximum {
            value: size,
            maximum,
        });
    }
    Ok(size)
}

pub fn validate_queue_config(
    raw: RawQueueConfig,
    offered_maximum: u16,
    regions: &[MemoryRegion],
    limits: VirtioLimits,
) -> Result<ValidatedQueueConfig, QueueViolation> {
    let size = validate_queue_size(raw.size, offered_maximum, limits)?;
    require_alignment(raw.descriptor_address, DESCRIPTOR_ALIGNMENT)?;
    require_alignment(raw.driver_address, AVAILABLE_ALIGNMENT)?;
    require_alignment(raw.device_address, USED_ALIGNMENT)?;

    let descriptor_length = u64::from(size)
        .checked_mul(DESCRIPTOR_BYTES)
        .ok_or(QueueViolation::AddressOverflow)?;
    let available_length = u64::from(size)
        .checked_mul(AVAILABLE_ELEMENT_BYTES)
        .and_then(|value| value.checked_add(AVAILABLE_HEADER_BYTES))
        .ok_or(QueueViolation::AddressOverflow)?;
    let used_length = u64::from(size)
        .checked_mul(USED_ELEMENT_BYTES)
        .and_then(|value| value.checked_add(USED_HEADER_BYTES))
        .ok_or(QueueViolation::AddressOverflow)?;

    let descriptors = checked_range(raw.descriptor_address, descriptor_length)?;
    let available = checked_range(raw.driver_address, available_length)?;
    let used = checked_range(raw.device_address, used_length)?;
    require_contained(regions, descriptors)?;
    require_contained(regions, available)?;
    require_contained(regions, used)?;
    if overlaps(descriptors, available) || overlaps(descriptors, used) || overlaps(available, used)
    {
        return Err(QueueViolation::RingOverlap);
    }
    debug_assert!(size > 0);
    Ok(ValidatedQueueConfig {
        size,
        descriptors,
        available,
        used,
    })
}

pub fn validate_available_delta(
    last_available: u16,
    available: u16,
    capacity: u16,
) -> Result<u16, QueueViolation> {
    if capacity == 0 {
        return Err(QueueViolation::ZeroSize);
    }
    let delta = available.wrapping_sub(last_available);
    if delta > capacity {
        return Err(QueueViolation::AvailableDelta { delta, capacity });
    }
    Ok(delta)
}

pub fn descriptor_address(config: ValidatedQueueConfig, index: u16) -> Option<u64> {
    element_address(
        config.descriptors.start,
        index,
        config.size,
        DESCRIPTOR_BYTES,
        0,
    )
}

pub fn available_element_address(config: ValidatedQueueConfig, index: u16) -> Option<u64> {
    if config.size == 0 {
        return None;
    }
    element_address(
        config.available.start,
        index % config.size,
        config.size,
        AVAILABLE_ELEMENT_BYTES,
        AVAILABLE_HEADER_BYTES,
    )
}

pub fn used_element_address(config: ValidatedQueueConfig, index: u16) -> Option<u64> {
    if config.size == 0 {
        return None;
    }
    element_address(
        config.used.start,
        index % config.size,
        config.size,
        USED_ELEMENT_BYTES,
        USED_HEADER_BYTES,
    )
}

fn require_alignment(address: u64, alignment: u64) -> Result<(), QueueViolation> {
    if !address.is_multiple_of(alignment) {
        return Err(QueueViolation::AddressMisaligned { address, alignment });
    }
    Ok(())
}

fn require_contained(regions: &[MemoryRegion], range: CheckedRange) -> Result<(), QueueViolation> {
    if !range_is_contained(regions, range) {
        return Err(QueueViolation::RingOutsideMemory {
            address: range.start,
            length: range.length(),
        });
    }
    Ok(())
}

fn overlaps(left: CheckedRange, right: CheckedRange) -> bool {
    left.start < right.end && right.start < left.end
}

fn element_address(base: u64, index: u16, size: u16, width: u64, header: u64) -> Option<u64> {
    if size == 0 || index >= size {
        return None;
    }
    u64::from(index)
        .checked_mul(width)
        .and_then(|offset| header.checked_add(offset))
        .and_then(|offset| base.checked_add(offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impossible_private_ranges_and_configs_remain_bounded() {
        let reversed = CheckedRange { start: 2, end: 1 };
        assert_eq!(reversed.length(), 0);
        let empty = checked_range(0, 0).expect("empty range");
        let forged = ValidatedQueueConfig {
            size: 0,
            descriptors: empty,
            available: empty,
            used: empty,
        };
        assert_eq!(available_element_address(forged, 0), None);
        assert_eq!(used_element_address(forged, 0), None);
    }
}
