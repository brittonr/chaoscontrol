//! Pure admission, identity, and accounting for selected runtime capacity.

const CAPACITY_PLAN_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.runtime-capacity.plan.v1\0";
pub const RUNTIME_CAPACITY_PLAN_SCHEMA: &str = "chaoscontrol.runtime-capacity-plan.v1";
pub const MAX_RUNTIME_JOURNAL_RECORD_SLOTS: usize =
    crate::scheduler::core::DEFAULT_SCHEDULE_JOURNAL_LIMIT;
pub const MAX_RUNTIME_SCRATCH_CLASSES: usize = 8;
pub const MAX_RUNTIME_SCRATCH_SLOTS: usize = 64;
pub const MAX_RUNTIME_SCRATCH_SLOT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RUNTIME_PACKET_SLOTS: usize = 4_096;
pub const MAX_RUNTIME_PACKET_SLOT_BYTES: usize = 64 * 1024;
pub const MAX_RUNTIME_QUEUE_METADATA_SLOTS: usize = MAX_RUNTIME_PACKET_SLOTS;
pub const MAX_RUNTIME_PREALLOCATED_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScratchClassLimit {
    pub slot_bytes: usize,
    pub slots: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapacityLimits {
    pub journal_record_slots: usize,
    pub scratch_classes: Vec<ScratchClassLimit>,
    pub packet_slots: usize,
    pub packet_slot_bytes: usize,
    pub retained_packet_bytes: usize,
    pub queue_metadata_slots: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapacityPlan {
    pub schema: String,
    pub journal_record_slots: usize,
    pub scratch_classes: Vec<ScratchClassLimit>,
    pub scratch_slots: usize,
    pub scratch_bytes: usize,
    pub packet_slots: usize,
    pub packet_slot_bytes: usize,
    pub packet_bytes: usize,
    pub retained_packet_bytes: usize,
    pub queue_metadata_slots: usize,
    pub total_preallocated_bytes: usize,
    pub plan_identity: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityField {
    JournalRecordSlots,
    ScratchClasses,
    ScratchSlotBytes,
    ScratchSlots,
    PacketSlots,
    PacketSlotBytes,
    RetainedPacketBytes,
    QueueMetadataSlots,
    TotalPreallocatedBytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCapacityError {
    Zero {
        field: CapacityField,
    },
    AboveMaximum {
        field: CapacityField,
        requested: usize,
        maximum: usize,
    },
    ScratchClassesNotAscending,
    QueueMetadataBelowPacketSlots {
        queue_metadata_slots: usize,
        packet_slots: usize,
    },
    RetainedBytesAbovePacketStorage {
        retained_packet_bytes: usize,
        packet_bytes: usize,
    },
    Arithmetic {
        field: CapacityField,
    },
    SlotExhausted,
    InvalidSlot {
        slot: usize,
    },
    SlotAlreadyInUse {
        slot: usize,
    },
    SlotAlreadyFree {
        slot: usize,
    },
    StaleLease {
        expected_generation: u64,
        lease_generation: u64,
    },
    OversizedLease {
        requested: usize,
        capacity: usize,
    },
    CounterOverflow {
        counter: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacitySlotState {
    Free,
    InUse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityLease {
    generation: u64,
    slot: usize,
    capacity: usize,
}

impl CapacityLease {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityPoolState {
    generation: u64,
    slot_capacities: Vec<usize>,
    slots: Vec<CapacitySlotState>,
    total_capacity: usize,
    in_use: usize,
    high_water: usize,
    exhaustion_count: u64,
    release_count: u64,
}

impl CapacityPoolState {
    pub fn new(generation: u64, slot_capacities: Vec<usize>) -> Result<Self, RuntimeCapacityError> {
        if slot_capacities.is_empty() {
            return Err(RuntimeCapacityError::Zero {
                field: CapacityField::ScratchSlots,
            });
        }
        if slot_capacities.contains(&0) {
            return Err(RuntimeCapacityError::Zero {
                field: CapacityField::ScratchSlotBytes,
            });
        }
        let total_capacity = slot_capacities.iter().try_fold(0usize, |total, capacity| {
            total
                .checked_add(*capacity)
                .ok_or(RuntimeCapacityError::Arithmetic {
                    field: CapacityField::ScratchSlotBytes,
                })
        })?;
        let slots = vec![CapacitySlotState::Free; slot_capacities.len()];
        Ok(Self {
            generation,
            slot_capacities,
            slots,
            total_capacity,
            in_use: 0,
            high_water: 0,
            exhaustion_count: 0,
            release_count: 0,
        })
    }

    pub fn acquire(&mut self, requested: usize) -> Result<CapacityLease, RuntimeCapacityError> {
        if requested == 0 {
            return Err(RuntimeCapacityError::Zero {
                field: CapacityField::ScratchSlotBytes,
            });
        }
        let selected = self
            .slots
            .iter()
            .zip(&self.slot_capacities)
            .enumerate()
            .find(|(_, (state, capacity))| {
                **state == CapacitySlotState::Free && **capacity >= requested
            })
            .map(|(slot, (_, capacity))| (slot, *capacity));
        let Some((slot, capacity)) = selected else {
            self.exhaustion_count = self.exhaustion_count.checked_add(1).ok_or(
                RuntimeCapacityError::CounterOverflow {
                    counter: "capacity exhaustion count",
                },
            )?;
            return Err(RuntimeCapacityError::SlotExhausted);
        };
        self.slots[slot] = CapacitySlotState::InUse;
        self.in_use = self
            .in_use
            .checked_add(1)
            .ok_or(RuntimeCapacityError::CounterOverflow {
                counter: "capacity slots in use",
            })?;
        self.high_water = self.high_water.max(self.in_use);
        Ok(CapacityLease {
            generation: self.generation,
            slot,
            capacity,
        })
    }

    pub fn release(&mut self, lease: CapacityLease) -> Result<(), RuntimeCapacityError> {
        if lease.generation != self.generation {
            return Err(RuntimeCapacityError::StaleLease {
                expected_generation: self.generation,
                lease_generation: lease.generation,
            });
        }
        let Some(state) = self.slots.get_mut(lease.slot) else {
            return Err(RuntimeCapacityError::InvalidSlot { slot: lease.slot });
        };
        if *state == CapacitySlotState::Free {
            return Err(RuntimeCapacityError::SlotAlreadyFree { slot: lease.slot });
        }
        let expected_capacity = self.slot_capacities[lease.slot];
        if lease.capacity != expected_capacity {
            return Err(RuntimeCapacityError::OversizedLease {
                requested: lease.capacity,
                capacity: expected_capacity,
            });
        }
        *state = CapacitySlotState::Free;
        self.in_use = self
            .in_use
            .checked_sub(1)
            .ok_or(RuntimeCapacityError::CounterOverflow {
                counter: "capacity slots in use",
            })?;
        self.release_count =
            self.release_count
                .checked_add(1)
                .ok_or(RuntimeCapacityError::CounterOverflow {
                    counter: "capacity release count",
                })?;
        Ok(())
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn slot_count(&self) -> usize {
        self.slot_capacities.len()
    }

    pub fn total_capacity(&self) -> usize {
        self.total_capacity
    }

    pub fn in_use(&self) -> usize {
        self.in_use
    }

    pub fn high_water(&self) -> usize {
        self.high_water
    }

    pub fn exhaustion_count(&self) -> u64 {
        self.exhaustion_count
    }

    pub fn release_count(&self) -> u64 {
        self.release_count
    }

    pub fn leaked_slots(&self) -> usize {
        self.in_use
    }
}

pub const RUNTIME_CAPACITY_OBSERVATIONS_SCHEMA: &str =
    "chaoscontrol.runtime-capacity-observations.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapacityStartupResult {
    Admitted,
    AllocationFailed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityUsageObservation {
    pub allocated_slots: usize,
    pub allocated_bytes: usize,
    pub in_use: usize,
    pub high_water: usize,
    pub exhaustion_count: u64,
    pub release_count: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapacityClaims {
    pub deterministic_latency: bool,
    pub global_zero_allocation: bool,
    pub zero_copy_io: bool,
    pub host_memory_guaranteed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapacityObservations {
    pub schema: String,
    pub plan: RuntimeCapacityPlan,
    pub startup_result: RuntimeCapacityStartupResult,
    pub scratch_pools: Vec<CapacityUsageObservation>,
    pub packet_pool: CapacityUsageObservation,
    pub queue_metadata_slots: usize,
    pub queue_metadata_high_water: usize,
    pub retained_packet_bytes: usize,
    pub leaked_scratch_slots: usize,
    pub leaked_packet_slots: usize,
    pub claims: RuntimeCapacityClaims,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapacityObservationError {
    Schema,
    PlanIdentity,
    ScratchPlan,
    PacketPlan,
    Usage,
    LeakCount,
    Overclaim { field: &'static str },
    Arithmetic,
}

pub fn validate_runtime_capacity_observations(
    observations: &RuntimeCapacityObservations,
) -> Result<(), RuntimeCapacityObservationError> {
    if observations.schema != RUNTIME_CAPACITY_OBSERVATIONS_SCHEMA {
        return Err(RuntimeCapacityObservationError::Schema);
    }
    if observations.plan.plan_identity != runtime_capacity_plan_identity(&observations.plan) {
        return Err(RuntimeCapacityObservationError::PlanIdentity);
    }
    let (scratch_slots, scratch_bytes, scratch_in_use) = observations
        .scratch_pools
        .iter()
        .try_fold((0usize, 0usize, 0usize), |(slots, bytes, in_use), pool| {
            if pool.in_use > pool.allocated_slots || pool.high_water > pool.allocated_slots {
                return Err(RuntimeCapacityObservationError::Usage);
            }
            Ok((
                slots
                    .checked_add(pool.allocated_slots)
                    .ok_or(RuntimeCapacityObservationError::Arithmetic)?,
                bytes
                    .checked_add(pool.allocated_bytes)
                    .ok_or(RuntimeCapacityObservationError::Arithmetic)?,
                in_use
                    .checked_add(pool.in_use)
                    .ok_or(RuntimeCapacityObservationError::Arithmetic)?,
            ))
        })?;
    if scratch_slots != observations.plan.scratch_slots
        || scratch_bytes != observations.plan.scratch_bytes
    {
        return Err(RuntimeCapacityObservationError::ScratchPlan);
    }
    if observations.packet_pool.allocated_slots != observations.plan.packet_slots
        || observations.packet_pool.allocated_bytes != observations.plan.packet_bytes
    {
        return Err(RuntimeCapacityObservationError::PacketPlan);
    }
    if observations.packet_pool.in_use > observations.packet_pool.allocated_slots
        || observations.packet_pool.high_water > observations.packet_pool.allocated_slots
        || observations.queue_metadata_slots != observations.plan.queue_metadata_slots
        || observations.queue_metadata_high_water > observations.queue_metadata_slots
        || observations.retained_packet_bytes > observations.plan.retained_packet_bytes
    {
        return Err(RuntimeCapacityObservationError::Usage);
    }
    if observations.leaked_scratch_slots != scratch_in_use
        || observations.leaked_packet_slots != observations.packet_pool.in_use
    {
        return Err(RuntimeCapacityObservationError::LeakCount);
    }
    for (field, claimed) in [
        (
            "deterministic_latency",
            observations.claims.deterministic_latency,
        ),
        (
            "global_zero_allocation",
            observations.claims.global_zero_allocation,
        ),
        ("zero_copy_io", observations.claims.zero_copy_io),
        (
            "host_memory_guaranteed",
            observations.claims.host_memory_guaranteed,
        ),
    ] {
        if claimed {
            return Err(RuntimeCapacityObservationError::Overclaim { field });
        }
    }
    Ok(())
}

pub fn plan_runtime_capacity(
    limits: &RuntimeCapacityLimits,
) -> Result<RuntimeCapacityPlan, RuntimeCapacityError> {
    require_bounded(
        CapacityField::JournalRecordSlots,
        limits.journal_record_slots,
        MAX_RUNTIME_JOURNAL_RECORD_SLOTS,
    )?;
    require_bounded(
        CapacityField::ScratchClasses,
        limits.scratch_classes.len(),
        MAX_RUNTIME_SCRATCH_CLASSES,
    )?;
    require_bounded(
        CapacityField::PacketSlots,
        limits.packet_slots,
        MAX_RUNTIME_PACKET_SLOTS,
    )?;
    require_bounded(
        CapacityField::PacketSlotBytes,
        limits.packet_slot_bytes,
        MAX_RUNTIME_PACKET_SLOT_BYTES,
    )?;
    require_bounded(
        CapacityField::RetainedPacketBytes,
        limits.retained_packet_bytes,
        MAX_RUNTIME_PREALLOCATED_BYTES,
    )?;
    require_bounded(
        CapacityField::QueueMetadataSlots,
        limits.queue_metadata_slots,
        MAX_RUNTIME_QUEUE_METADATA_SLOTS,
    )?;
    if limits.queue_metadata_slots < limits.packet_slots {
        return Err(RuntimeCapacityError::QueueMetadataBelowPacketSlots {
            queue_metadata_slots: limits.queue_metadata_slots,
            packet_slots: limits.packet_slots,
        });
    }

    let mut previous_slot_bytes = 0usize;
    let mut scratch_slots = 0usize;
    let mut scratch_bytes = 0usize;
    for class in &limits.scratch_classes {
        require_bounded(
            CapacityField::ScratchSlotBytes,
            class.slot_bytes,
            MAX_RUNTIME_SCRATCH_SLOT_BYTES,
        )?;
        require_bounded(
            CapacityField::ScratchSlots,
            class.slots,
            MAX_RUNTIME_SCRATCH_SLOTS,
        )?;
        if class.slot_bytes <= previous_slot_bytes {
            return Err(RuntimeCapacityError::ScratchClassesNotAscending);
        }
        previous_slot_bytes = class.slot_bytes;
        scratch_slots =
            scratch_slots
                .checked_add(class.slots)
                .ok_or(RuntimeCapacityError::Arithmetic {
                    field: CapacityField::ScratchSlots,
                })?;
        scratch_bytes = scratch_bytes
            .checked_add(class.slot_bytes.checked_mul(class.slots).ok_or(
                RuntimeCapacityError::Arithmetic {
                    field: CapacityField::ScratchSlotBytes,
                },
            )?)
            .ok_or(RuntimeCapacityError::Arithmetic {
                field: CapacityField::ScratchSlotBytes,
            })?;
    }
    if scratch_slots > MAX_RUNTIME_SCRATCH_SLOTS {
        return Err(RuntimeCapacityError::AboveMaximum {
            field: CapacityField::ScratchSlots,
            requested: scratch_slots,
            maximum: MAX_RUNTIME_SCRATCH_SLOTS,
        });
    }

    let packet_bytes = checked_capacity_bytes(
        CapacityField::PacketSlotBytes,
        limits.packet_slots,
        limits.packet_slot_bytes,
    )?;
    if limits.retained_packet_bytes > packet_bytes {
        return Err(RuntimeCapacityError::RetainedBytesAbovePacketStorage {
            retained_packet_bytes: limits.retained_packet_bytes,
            packet_bytes,
        });
    }
    let total_preallocated_bytes =
        scratch_bytes
            .checked_add(packet_bytes)
            .ok_or(RuntimeCapacityError::Arithmetic {
                field: CapacityField::TotalPreallocatedBytes,
            })?;
    if total_preallocated_bytes > MAX_RUNTIME_PREALLOCATED_BYTES {
        return Err(RuntimeCapacityError::AboveMaximum {
            field: CapacityField::TotalPreallocatedBytes,
            requested: total_preallocated_bytes,
            maximum: MAX_RUNTIME_PREALLOCATED_BYTES,
        });
    }

    let mut plan = RuntimeCapacityPlan {
        schema: RUNTIME_CAPACITY_PLAN_SCHEMA.to_string(),
        journal_record_slots: limits.journal_record_slots,
        scratch_classes: limits.scratch_classes.clone(),
        scratch_slots,
        scratch_bytes,
        packet_slots: limits.packet_slots,
        packet_slot_bytes: limits.packet_slot_bytes,
        packet_bytes,
        retained_packet_bytes: limits.retained_packet_bytes,
        queue_metadata_slots: limits.queue_metadata_slots,
        total_preallocated_bytes,
        plan_identity: [0; 32],
    };
    plan.plan_identity = runtime_capacity_plan_identity(&plan);
    Ok(plan)
}

pub fn runtime_capacity_plan_identity(plan: &RuntimeCapacityPlan) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CAPACITY_PLAN_IDENTITY_DOMAIN);
    hash_usize(&mut hasher, plan.journal_record_slots);
    hash_usize(&mut hasher, plan.scratch_classes.len());
    for class in &plan.scratch_classes {
        hash_usize(&mut hasher, class.slot_bytes);
        hash_usize(&mut hasher, class.slots);
    }
    hash_usize(&mut hasher, plan.packet_slots);
    hash_usize(&mut hasher, plan.packet_slot_bytes);
    hash_usize(&mut hasher, plan.retained_packet_bytes);
    hash_usize(&mut hasher, plan.queue_metadata_slots);
    *hasher.finalize().as_bytes()
}

fn checked_capacity_bytes(
    field: CapacityField,
    slots: usize,
    slot_bytes: usize,
) -> Result<usize, RuntimeCapacityError> {
    slots
        .checked_mul(slot_bytes)
        .ok_or(RuntimeCapacityError::Arithmetic { field })
}

fn require_bounded(
    field: CapacityField,
    requested: usize,
    maximum: usize,
) -> Result<(), RuntimeCapacityError> {
    if requested == 0 {
        return Err(RuntimeCapacityError::Zero { field });
    }
    if requested > maximum {
        return Err(RuntimeCapacityError::AboveMaximum {
            field,
            requested,
            maximum,
        });
    }
    Ok(())
}

fn hash_usize(hasher: &mut blake3::Hasher, value: usize) {
    hasher.update(&(value as u64).to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_JOURNAL_SLOTS: usize = 16;
    const TEST_SMALL_SCRATCH_BYTES: usize = 64;
    const TEST_LARGE_SCRATCH_BYTES: usize = 256;
    const TEST_SMALL_SCRATCH_SLOTS: usize = 2;
    const TEST_LARGE_SCRATCH_SLOTS: usize = 1;
    const TEST_PACKET_SLOTS: usize = 4;
    const TEST_PACKET_SLOT_BYTES: usize = 512;
    const TEST_RETAINED_PACKET_BYTES: usize = 1_024;
    const TEST_POOL_GENERATION: u64 = 7;
    const TEST_ALTERNATE_GENERATION: u64 = 3;

    fn limits() -> RuntimeCapacityLimits {
        RuntimeCapacityLimits {
            journal_record_slots: TEST_JOURNAL_SLOTS,
            scratch_classes: vec![
                ScratchClassLimit {
                    slot_bytes: TEST_SMALL_SCRATCH_BYTES,
                    slots: TEST_SMALL_SCRATCH_SLOTS,
                },
                ScratchClassLimit {
                    slot_bytes: TEST_LARGE_SCRATCH_BYTES,
                    slots: TEST_LARGE_SCRATCH_SLOTS,
                },
            ],
            packet_slots: TEST_PACKET_SLOTS,
            packet_slot_bytes: TEST_PACKET_SLOT_BYTES,
            retained_packet_bytes: TEST_RETAINED_PACKET_BYTES,
            queue_metadata_slots: TEST_PACKET_SLOTS,
        }
    }

    fn observations() -> RuntimeCapacityObservations {
        let plan = plan_runtime_capacity(&limits()).expect("valid capacity plan");
        RuntimeCapacityObservations {
            schema: RUNTIME_CAPACITY_OBSERVATIONS_SCHEMA.to_string(),
            plan,
            startup_result: RuntimeCapacityStartupResult::Admitted,
            scratch_pools: vec![
                CapacityUsageObservation {
                    allocated_slots: TEST_SMALL_SCRATCH_SLOTS,
                    allocated_bytes: TEST_SMALL_SCRATCH_BYTES * TEST_SMALL_SCRATCH_SLOTS,
                    in_use: 0,
                    high_water: 1,
                    exhaustion_count: 0,
                    release_count: 1,
                },
                CapacityUsageObservation {
                    allocated_slots: TEST_LARGE_SCRATCH_SLOTS,
                    allocated_bytes: TEST_LARGE_SCRATCH_BYTES * TEST_LARGE_SCRATCH_SLOTS,
                    in_use: 0,
                    high_water: 1,
                    exhaustion_count: 0,
                    release_count: 1,
                },
            ],
            packet_pool: CapacityUsageObservation {
                allocated_slots: TEST_PACKET_SLOTS,
                allocated_bytes: TEST_PACKET_SLOTS * TEST_PACKET_SLOT_BYTES,
                in_use: 0,
                high_water: 1,
                exhaustion_count: 0,
                release_count: 1,
            },
            queue_metadata_slots: TEST_PACKET_SLOTS,
            queue_metadata_high_water: 1,
            retained_packet_bytes: 0,
            leaked_scratch_slots: 0,
            leaked_packet_slots: 0,
            claims: RuntimeCapacityClaims::default(),
        }
    }

    #[test]
    fn capacity_observations_bind_the_plan_usage_leaks_and_non_claims() {
        let valid = observations();
        validate_runtime_capacity_observations(&valid).expect("valid observations");

        let mut forged_identity = valid.clone();
        forged_identity.plan.plan_identity[0] ^= 1;
        assert_eq!(
            validate_runtime_capacity_observations(&forged_identity),
            Err(RuntimeCapacityObservationError::PlanIdentity)
        );

        let mut forged_leak = valid.clone();
        forged_leak.leaked_packet_slots = 1;
        assert_eq!(
            validate_runtime_capacity_observations(&forged_leak),
            Err(RuntimeCapacityObservationError::LeakCount)
        );

        let mut overclaim = valid;
        overclaim.claims.deterministic_latency = true;
        assert_eq!(
            validate_runtime_capacity_observations(&overclaim),
            Err(RuntimeCapacityObservationError::Overclaim {
                field: "deterministic_latency",
            })
        );
    }

    #[test]
    fn valid_plan_has_stable_checked_totals_and_identity() {
        let first = plan_runtime_capacity(&limits()).expect("valid capacity plan");
        let second = plan_runtime_capacity(&limits()).expect("repeat capacity plan");
        assert_eq!(first, second);
        assert_eq!(
            first.scratch_slots,
            TEST_SMALL_SCRATCH_SLOTS + TEST_LARGE_SCRATCH_SLOTS
        );
        assert_eq!(
            first.packet_bytes,
            TEST_PACKET_SLOTS * TEST_PACKET_SLOT_BYTES
        );
        assert_eq!(first.plan_identity, runtime_capacity_plan_identity(&first));
    }

    #[test]
    fn invalid_zero_cap_contradiction_and_overflow_fail_closed() {
        let mut zero = limits();
        zero.packet_slots = 0;
        assert_eq!(
            plan_runtime_capacity(&zero),
            Err(RuntimeCapacityError::Zero {
                field: CapacityField::PacketSlots,
            })
        );

        let mut one_past_cap = limits();
        one_past_cap.packet_slots = MAX_RUNTIME_PACKET_SLOTS + 1;
        assert_eq!(
            plan_runtime_capacity(&one_past_cap),
            Err(RuntimeCapacityError::AboveMaximum {
                field: CapacityField::PacketSlots,
                requested: MAX_RUNTIME_PACKET_SLOTS + 1,
                maximum: MAX_RUNTIME_PACKET_SLOTS,
            })
        );

        let mut contradiction = limits();
        contradiction.queue_metadata_slots = TEST_PACKET_SLOTS - 1;
        assert_eq!(
            plan_runtime_capacity(&contradiction),
            Err(RuntimeCapacityError::QueueMetadataBelowPacketSlots {
                queue_metadata_slots: TEST_PACKET_SLOTS - 1,
                packet_slots: TEST_PACKET_SLOTS,
            })
        );

        let mut overflow = limits();
        overflow.packet_slots = MAX_RUNTIME_PACKET_SLOTS;
        overflow.packet_slot_bytes = MAX_RUNTIME_PACKET_SLOT_BYTES;
        overflow.queue_metadata_slots = MAX_RUNTIME_PACKET_SLOTS;
        overflow.scratch_classes = vec![ScratchClassLimit {
            slot_bytes: MAX_RUNTIME_SCRATCH_SLOT_BYTES,
            slots: MAX_RUNTIME_SCRATCH_SLOTS,
        }];
        assert!(matches!(
            plan_runtime_capacity(&overflow),
            Err(RuntimeCapacityError::AboveMaximum {
                field: CapacityField::TotalPreallocatedBytes,
                ..
            })
        ));
        assert_eq!(
            checked_capacity_bytes(CapacityField::PacketSlotBytes, usize::MAX, 2),
            Err(RuntimeCapacityError::Arithmetic {
                field: CapacityField::PacketSlotBytes,
            })
        );
    }

    #[test]
    fn pool_accounts_for_acquire_release_exhaustion_and_leaks() {
        let mut pool = CapacityPoolState::new(
            TEST_POOL_GENERATION,
            vec![TEST_SMALL_SCRATCH_BYTES, TEST_LARGE_SCRATCH_BYTES],
        )
        .expect("valid pool");
        let small = pool.acquire(TEST_SMALL_SCRATCH_BYTES).expect("small lease");
        let large = pool.acquire(TEST_LARGE_SCRATCH_BYTES).expect("large lease");
        assert_eq!(pool.acquire(1), Err(RuntimeCapacityError::SlotExhausted));
        assert_eq!(pool.high_water(), TEST_SMALL_SCRATCH_SLOTS);
        assert_eq!(pool.exhaustion_count(), 1);
        pool.release(small).expect("release small lease");
        assert_eq!(pool.leaked_slots(), 1);
        pool.release(large).expect("release large lease");
        assert_eq!(pool.leaked_slots(), 0);
        assert_eq!(pool.release_count(), TEST_SMALL_SCRATCH_SLOTS as u64);
    }

    #[test]
    fn pool_rejects_stale_duplicate_and_oversized_leases() {
        let mut pool =
            CapacityPoolState::new(TEST_ALTERNATE_GENERATION, vec![TEST_SMALL_SCRATCH_BYTES])
                .expect("valid pool");
        let lease = pool.acquire(1).expect("lease");
        let duplicate = CapacityLease {
            generation: lease.generation,
            slot: lease.slot,
            capacity: lease.capacity,
        };
        pool.release(lease).expect("first release");
        assert_eq!(
            pool.release(duplicate),
            Err(RuntimeCapacityError::SlotAlreadyFree { slot: 0 })
        );

        let current = pool.acquire(1).expect("current lease");
        let stale = CapacityLease {
            generation: current.generation - 1,
            slot: current.slot,
            capacity: current.capacity,
        };
        assert_eq!(
            pool.release(stale),
            Err(RuntimeCapacityError::StaleLease {
                expected_generation: current.generation,
                lease_generation: current.generation - 1,
            })
        );
        let forged = CapacityLease {
            generation: current.generation,
            slot: current.slot,
            capacity: current.capacity + 1,
        };
        assert_eq!(
            pool.release(forged),
            Err(RuntimeCapacityError::OversizedLease {
                requested: current.capacity + 1,
                capacity: current.capacity,
            })
        );
        pool.release(current).expect("release current lease");
    }
}
