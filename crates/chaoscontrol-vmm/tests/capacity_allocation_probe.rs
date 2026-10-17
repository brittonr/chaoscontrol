//! Allocation-attempt evidence is scoped to the selected initialized-capacity paths,
//! not the complete VM, trace construction, snapshot capture, or host packet export.
use chaoscontrol_sim_core::{
    scheduler::{core::ProgressMode, SchedulerConfig, VcpuScheduler},
    ScratchClassLimit,
};
use chaoscontrol_vmm::devices::net::{DeterministicNet, NetQueueError};
use chaoscontrol_vmm::devices::virtio_buffer::{BoundedBufferAllocator, HostBufferAllocator};
use chaoscontrol_vmm::devices::virtio_types::ResourceViolation;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ATTEMPTS: Cell<usize> = const { Cell::new(0) };
}
struct AllocationProbe;
fn allocation_attempt() {
    if TRACKING.try_with(Cell::get).unwrap_or(false) {
        let _ = ATTEMPTS.try_with(|count| count.set(count.get() + 1));
    }
}
// SAFETY: allocation and deallocation are delegated unchanged to System.
unsafe impl GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocation_attempt();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocation_attempt();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocation_attempt();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;
struct TrackingGuard;
impl Drop for TrackingGuard {
    fn drop(&mut self) {
        TRACKING.with(|enabled| enabled.set(false));
    }
}
fn probe<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    ATTEMPTS.with(|count| count.set(0));
    TRACKING.with(|enabled| assert!(!enabled.replace(true)));
    let guard = TrackingGuard;
    let result = operation();
    drop(guard);
    (result, ATTEMPTS.with(Cell::get))
}

#[test]
fn selected_steady_state_capacity_paths_attempt_no_allocation() {
    const SLOT_BYTES: usize = 64;
    const SLOTS: usize = 2;
    const ITERATIONS: usize = 32;
    const PACKET: [u8; SLOT_BYTES] = [0x5a; SLOT_BYTES];
    let (calibration, observed) = probe(|| Vec::<u8>::with_capacity(SLOT_BYTES));
    assert!(
        observed > 0,
        "the probe must observe actual allocator attempts"
    );
    drop(calibration);
    let mut scratch = HostBufferAllocator::try_from_classes(&[ScratchClassLimit {
        slot_bytes: SLOT_BYTES,
        slots: SLOTS,
    }])
    .unwrap();
    let mut net = DeterministicNet::try_new([2, 0, 0, 0, 0, 1], SLOTS, SLOT_BYTES).unwrap();
    let mut scheduler = VcpuScheduler::try_new_with_journal_limit(
        &SchedulerConfig::default(),
        ProgressMode::ExactSingleStep,
        vec![true],
        SLOTS,
    )
    .unwrap();
    for _ in 0..ITERATIONS {
        let (_, attempts) = probe(|| {
            let mut first = scratch.acquire(SLOT_BYTES, SLOT_BYTES).unwrap();
            let second = scratch.acquire(SLOT_BYTES, SLOT_BYTES).unwrap();
            first.bytes_mut().copy_from_slice(&PACKET);
            assert!(matches!(
                scratch.acquire(1, SLOT_BYTES),
                Err(ResourceViolation::ScratchExhausted)
            ));
            scratch.release(first).unwrap();
            scratch.release(second).unwrap();
            let recycled = scratch.acquire(SLOT_BYTES, SLOT_BYTES).unwrap();
            scratch.release(recycled).unwrap();
            for _ in 0..SLOTS {
                net.try_enqueue_tx_bounded(PACKET, SLOTS, (SLOTS * SLOT_BYTES) as u64)
                    .unwrap();
            }
            assert!(matches!(
                net.try_enqueue_tx_bounded(PACKET, SLOTS, (SLOTS * SLOT_BYTES) as u64),
                Err(NetQueueError::PacketLimit { .. })
            ));
            let reservation = scheduler.reserve_transition().unwrap();
            scheduler.release_transition(reservation).unwrap();
        });
        assert_eq!(
            attempts, 0,
            "selected capacity acquisition, exhaustion and reuse"
        );
        // Export intentionally allocates caller-owned packets and is outside this claim.
        assert_eq!(net.drain_tx(), vec![PACKET.to_vec(); SLOTS]);
    }
    assert_eq!(scratch.leaked_slots(), 0);
    assert_eq!(scratch.observations().in_use, 0);
    assert_eq!(scratch.observations().high_water, SLOTS);
    assert_eq!(net.capacity_observations().in_use, 0);
    assert_eq!(
        net.capacity_observations().release_count,
        (ITERATIONS * SLOTS) as u64
    );
}
