//! Startup-allocated scratch buffers for virtio imperative shells.

use super::virtio_types::{ResourceViolation, VirtioFailure, DEFAULT_SCRATCH_BYTES};

const SCRATCH_POOL_GENERATION: u64 = 1;
const DEFAULT_SCRATCH_SLOT_COUNT: usize = 1;

pub struct ScratchBufferLease {
    capacity_lease: ::chaoscontrol_sim_core::CapacityLease,
    requested: usize,
    buffer: Vec<u8>,
}

impl ScratchBufferLease {
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.requested]
    }

    pub fn slot(&self) -> usize {
        self.capacity_lease.slot()
    }

    pub fn generation(&self) -> u64 {
        self.capacity_lease.generation()
    }
}

pub trait BoundedBufferAllocator: Send {
    fn acquire(
        &mut self,
        requested: usize,
        maximum: usize,
    ) -> Result<ScratchBufferLease, ResourceViolation>;

    fn release(&mut self, lease: ScratchBufferLease) -> Result<(), ResourceViolation>;

    fn observations(&self) -> ScratchPoolObservations;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScratchPoolObservations {
    pub allocated_slots: usize,
    pub allocated_bytes: usize,
    pub in_use: usize,
    pub high_water: usize,
    pub exhaustion_count: u64,
    pub release_count: u64,
}

pub struct HostBufferAllocator {
    state: ::chaoscontrol_sim_core::CapacityPoolState,
    slots: Vec<Option<Vec<u8>>>,
}

impl HostBufferAllocator {
    pub fn try_default() -> Result<Self, ResourceViolation> {
        Self::try_from_classes(&[::chaoscontrol_sim_core::ScratchClassLimit {
            slot_bytes: DEFAULT_SCRATCH_BYTES,
            slots: DEFAULT_SCRATCH_SLOT_COUNT,
        }])
    }

    pub fn try_from_classes(
        classes: &[::chaoscontrol_sim_core::ScratchClassLimit],
    ) -> Result<Self, ResourceViolation> {
        Self::try_from_classes_with(classes, allocate_zeroed_buffer)
    }

    fn try_from_classes_with(
        classes: &[::chaoscontrol_sim_core::ScratchClassLimit],
        mut allocate: impl FnMut(usize) -> Result<Vec<u8>, ResourceViolation>,
    ) -> Result<Self, ResourceViolation> {
        let slot_count = classes.iter().try_fold(0usize, |total, class| {
            total
                .checked_add(class.slots)
                .ok_or(ResourceViolation::Allocation {
                    requested: usize::MAX,
                })
        })?;
        let mut slot_capacities = Vec::new();
        slot_capacities.try_reserve_exact(slot_count).map_err(|_| {
            ResourceViolation::Allocation {
                requested: slot_count,
            }
        })?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(slot_count)
            .map_err(|_| ResourceViolation::Allocation {
                requested: slot_count,
            })?;
        for class in classes {
            for _ in 0..class.slots {
                let buffer = allocate(class.slot_bytes)?;
                if buffer.len() != class.slot_bytes {
                    return Err(ResourceViolation::Allocation {
                        requested: class.slot_bytes,
                    });
                }
                slot_capacities.push(class.slot_bytes);
                slots.push(Some(buffer));
            }
        }
        let state = ::chaoscontrol_sim_core::CapacityPoolState::new(
            SCRATCH_POOL_GENERATION,
            slot_capacities,
        )
        .map_err(map_capacity_error)?;
        Ok(Self { state, slots })
    }

    pub fn leaked_slots(&self) -> usize {
        self.state.leaked_slots()
    }
}

impl Default for HostBufferAllocator {
    fn default() -> Self {
        Self::try_default()
            .expect("default virtio scratch capacity must allocate before activation")
    }
}

impl BoundedBufferAllocator for HostBufferAllocator {
    fn acquire(
        &mut self,
        requested: usize,
        maximum: usize,
    ) -> Result<ScratchBufferLease, ResourceViolation> {
        if requested > maximum {
            return Err(ResourceViolation::ScratchLimit { requested, maximum });
        }
        let capacity_lease = self.state.acquire(requested).map_err(map_capacity_error)?;
        let slot = capacity_lease.slot();
        let Some(entry) = self.slots.get_mut(slot) else {
            self.state
                .release(capacity_lease)
                .map_err(map_capacity_error)?;
            return Err(ResourceViolation::ScratchLease { slot });
        };
        let Some(mut buffer) = entry.take() else {
            self.state
                .release(capacity_lease)
                .map_err(map_capacity_error)?;
            return Err(ResourceViolation::ScratchLease { slot });
        };
        if requested > buffer.len() {
            let maximum = buffer.len();
            *entry = Some(buffer);
            self.state
                .release(capacity_lease)
                .map_err(map_capacity_error)?;
            return Err(ResourceViolation::ScratchLimit { requested, maximum });
        }
        buffer[..requested].fill(0);
        Ok(ScratchBufferLease {
            capacity_lease,
            requested,
            buffer,
        })
    }

    fn release(&mut self, mut lease: ScratchBufferLease) -> Result<(), ResourceViolation> {
        let slot = lease.capacity_lease.slot();
        let Some(entry) = self.slots.get_mut(slot) else {
            return Err(ResourceViolation::ScratchLease { slot });
        };
        if entry.is_some() || lease.buffer.len() != lease.capacity_lease.capacity() {
            return Err(ResourceViolation::ScratchLease { slot });
        }
        lease.buffer.fill(0);
        self.state
            .release(lease.capacity_lease)
            .map_err(map_capacity_error)?;
        *entry = Some(lease.buffer);
        Ok(())
    }

    fn observations(&self) -> ScratchPoolObservations {
        ScratchPoolObservations {
            allocated_slots: self.state.slot_count(),
            allocated_bytes: self.state.total_capacity(),
            in_use: self.state.in_use(),
            high_water: self.state.high_water(),
            exhaustion_count: self.state.exhaustion_count(),
            release_count: self.state.release_count(),
        }
    }
}

fn allocate_zeroed_buffer(requested: usize) -> Result<Vec<u8>, ResourceViolation> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(requested)
        .map_err(|_| ResourceViolation::Allocation { requested })?;
    buffer.resize(requested, 0);
    Ok(buffer)
}

pub struct RejectingBufferAllocator;

impl BoundedBufferAllocator for RejectingBufferAllocator {
    fn acquire(
        &mut self,
        requested: usize,
        maximum: usize,
    ) -> Result<ScratchBufferLease, ResourceViolation> {
        if requested > maximum {
            return Err(ResourceViolation::ScratchLimit { requested, maximum });
        }
        Err(ResourceViolation::Allocation { requested })
    }

    fn release(&mut self, lease: ScratchBufferLease) -> Result<(), ResourceViolation> {
        Err(ResourceViolation::ScratchLease { slot: lease.slot() })
    }

    fn observations(&self) -> ScratchPoolObservations {
        ScratchPoolObservations::default()
    }
}

pub fn with_zeroed_scratch<T>(
    allocator: &mut dyn BoundedBufferAllocator,
    requested: usize,
    maximum: usize,
    operation: impl FnOnce(&mut [u8]) -> Result<T, VirtioFailure>,
) -> Result<T, VirtioFailure> {
    let mut lease = allocator
        .acquire(requested, maximum)
        .map_err(VirtioFailure::Resource)?;
    let outcome = operation(lease.bytes_mut());
    allocator.release(lease).map_err(VirtioFailure::Resource)?;
    outcome
}

fn map_capacity_error(error: ::chaoscontrol_sim_core::RuntimeCapacityError) -> ResourceViolation {
    match error {
        ::chaoscontrol_sim_core::RuntimeCapacityError::SlotExhausted => {
            ResourceViolation::ScratchExhausted
        }
        ::chaoscontrol_sim_core::RuntimeCapacityError::Zero { .. } => {
            ResourceViolation::ScratchLimit {
                requested: 0,
                maximum: DEFAULT_SCRATCH_BYTES,
            }
        }
        ::chaoscontrol_sim_core::RuntimeCapacityError::InvalidSlot { slot }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::SlotAlreadyInUse { slot }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::SlotAlreadyFree { slot } => {
            ResourceViolation::ScratchLease { slot }
        }
        ::chaoscontrol_sim_core::RuntimeCapacityError::StaleLease { .. }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::OversizedLease { .. }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::CounterOverflow { .. }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::AboveMaximum { .. }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::ScratchClassesNotAscending
        | ::chaoscontrol_sim_core::RuntimeCapacityError::QueueMetadataBelowPacketSlots { .. }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::RetainedBytesAbovePacketStorage {
            ..
        }
        | ::chaoscontrol_sim_core::RuntimeCapacityError::Arithmetic { .. } => {
            ResourceViolation::ScratchLease { slot: usize::MAX }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SLOT_BYTES: usize = 32;
    const TEST_REQUEST_BYTES: usize = 16;
    const TEST_FILL_BYTE: u8 = 0xA5;

    fn pool() -> HostBufferAllocator {
        HostBufferAllocator::try_from_classes(&[::chaoscontrol_sim_core::ScratchClassLimit {
            slot_bytes: TEST_SLOT_BYTES,
            slots: DEFAULT_SCRATCH_SLOT_COUNT,
        }])
        .expect("scratch pool")
    }

    #[test]
    fn startup_allocation_failure_is_typed_before_a_pool_exists() {
        let result = HostBufferAllocator::try_from_classes_with(
            &[::chaoscontrol_sim_core::ScratchClassLimit {
                slot_bytes: TEST_SLOT_BYTES,
                slots: DEFAULT_SCRATCH_SLOT_COUNT,
            }],
            |requested| Err(ResourceViolation::Allocation { requested }),
        );
        assert!(matches!(
            result,
            Err(ResourceViolation::Allocation {
                requested: TEST_SLOT_BYTES,
            })
        ));
    }

    #[test]
    fn scratch_lease_is_zeroed_reused_and_accounted() {
        let mut pool = pool();
        let first_slot =
            with_zeroed_scratch(&mut pool, TEST_REQUEST_BYTES, TEST_SLOT_BYTES, |bytes| {
                assert!(bytes.iter().all(|byte| *byte == 0));
                bytes.fill(TEST_FILL_BYTE);
                Ok(0)
            })
            .expect("first lease");
        assert_eq!(first_slot, 0);
        with_zeroed_scratch(&mut pool, TEST_REQUEST_BYTES, TEST_SLOT_BYTES, |bytes| {
            assert!(bytes.iter().all(|byte| *byte == 0));
            Ok(())
        })
        .expect("reused lease");
        let observations = pool.observations();
        assert_eq!(observations.in_use, 0);
        assert_eq!(observations.high_water, DEFAULT_SCRATCH_SLOT_COUNT);
        assert_eq!(observations.release_count, 2);
        assert_eq!(pool.leaked_slots(), 0);
    }

    #[test]
    fn scratch_pool_rejects_oversized_and_exhausted_requests() {
        let mut pool = pool();
        assert!(matches!(
            pool.acquire(TEST_SLOT_BYTES + 1, TEST_SLOT_BYTES),
            Err(ResourceViolation::ScratchLimit {
                requested,
                maximum: TEST_SLOT_BYTES,
            }) if requested == TEST_SLOT_BYTES + 1
        ));
        let lease = pool
            .acquire(TEST_REQUEST_BYTES, TEST_SLOT_BYTES)
            .expect("first lease");
        assert!(matches!(
            pool.acquire(TEST_REQUEST_BYTES, TEST_SLOT_BYTES),
            Err(ResourceViolation::ScratchExhausted)
        ));
        pool.release(lease).expect("release lease");
    }

    #[test]
    fn operation_error_still_returns_and_zeroes_the_lease() {
        let mut pool = pool();
        let result: Result<(), VirtioFailure> =
            with_zeroed_scratch(&mut pool, TEST_REQUEST_BYTES, TEST_SLOT_BYTES, |bytes| {
                bytes.fill(TEST_FILL_BYTE);
                Err(VirtioFailure::BackendWrite)
            });
        assert_eq!(result, Err(VirtioFailure::BackendWrite));
        assert_eq!(pool.leaked_slots(), 0);
        let lease = pool
            .acquire(TEST_REQUEST_BYTES, TEST_SLOT_BYTES)
            .expect("lease after error");
        assert!(lease.buffer[..TEST_REQUEST_BYTES]
            .iter()
            .all(|byte| *byte == 0));
        pool.release(lease).expect("release lease");
    }
}
