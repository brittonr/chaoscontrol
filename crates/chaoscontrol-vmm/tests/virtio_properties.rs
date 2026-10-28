mod virtio_support;

use chaoscontrol_vmm::devices::virtio_chain::{plan_descriptor_chain, VirtqDesc};
use chaoscontrol_vmm::devices::virtio_request::{
    plan_block_request, plan_entropy_request, plan_net_request, BlockRequestHeader, NetDirection,
};
use chaoscontrol_vmm::devices::virtio_types::{VirtioLimits, MAX_QUEUE_SIZE};
use chaoscontrol_vmm::devices::virtio_validation::{
    available_element_address, used_element_address, validate_available_delta,
    validate_queue_config, MemoryRegion, RawQueueConfig,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

const CORPUS_CASES: u64 = 4096;
const DESCRIPTOR_CASES: usize = 8;
const MEMORY_BYTES: u64 = 128 * 1024;
const DISK_BYTES: u64 = 1024 * 1024;
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const LCG_INCREMENT: u64 = 1442695040888963407;

#[derive(Clone, Copy)]
struct CorpusValue(u64);

impl CorpusValue {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(LCG_MULTIPLIER)
            .wrapping_add(LCG_INCREMENT);
        self.0
    }

    fn next_u32(&mut self) -> u32 {
        let bytes = self.next().to_le_bytes();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn next_u16(&mut self) -> u16 {
        let bytes = self.next().to_le_bytes();
        u16::from_le_bytes([bytes[0], bytes[1]])
    }
}

#[test]
fn generated_guest_inputs_never_panic_or_escape_limits() {
    let limits = VirtioLimits::default();
    for seed in 0..CORPUS_CASES {
        let (outcome, allocations) = reached_shell_corpus::probe(|| {
            catch_unwind(AssertUnwindSafe(|| {
                let mut corpus = CorpusValue(seed);
                let memory = [MemoryRegion {
                    start: corpus.next() % MEMORY_BYTES,
                    length: corpus.next(),
                }];
                let raw = RawQueueConfig {
                    size: corpus.next_u32(),
                    descriptor_address: corpus.next(),
                    driver_address: corpus.next(),
                    device_address: corpus.next(),
                };
                if let Ok(config) = validate_queue_config(raw, MAX_QUEUE_SIZE, &memory, limits) {
                    assert!(config.size() > 0);
                    assert!(config.size() <= limits.max_queue_size);
                    assert!(available_element_address(config, corpus.next_u16()).is_some());
                    assert!(used_element_address(config, corpus.next_u16()).is_some());
                }
                let available = corpus.next_u16();
                let last = corpus.next_u16();
                if let Ok(delta) = validate_available_delta(last, available, MAX_QUEUE_SIZE) {
                    assert!(delta <= MAX_QUEUE_SIZE);
                }

                let mut descriptors = [VirtqDesc::default(); DESCRIPTOR_CASES];
                for descriptor in &mut descriptors {
                    *descriptor = VirtqDesc {
                        addr: corpus.next(),
                        len: corpus.next_u32(),
                        flags: corpus.next_u16(),
                        next: corpus.next_u16(),
                    };
                }
                let queue_size = corpus.next_u16() % (MAX_QUEUE_SIZE + 1);
                let head = corpus.next_u16();
                if let Ok(chain) =
                    plan_descriptor_chain(&descriptors, head, queue_size, &memory, limits)
                {
                    assert!(chain.count() <= limits.max_chain_descriptors);
                    assert!(chain.aggregate_length() <= limits.max_aggregate_bytes);
                    let header = BlockRequestHeader {
                        operation: corpus.next_u32(),
                        reserved: corpus.next_u32(),
                        sector: corpus.next(),
                    };
                    let _ = plan_block_request(
                        &chain,
                        header,
                        chaoscontrol_vmm::devices::virtio_request::BlockDeviceProfile {
                            disk_size: DISK_BYTES,
                            flush_supported: true,
                        },
                        limits,
                    );
                    let _ = plan_net_request(&chain, NetDirection::Transmit, corpus.next(), limits);
                    let _ = plan_net_request(&chain, NetDirection::Receive, corpus.next(), limits);
                    let _ = plan_entropy_request(&chain, limits);
                }
            }))
        });
        assert_eq!(allocations, 0, "pure generated case {seed} allocated");
        assert!(outcome.is_ok(), "generated case {seed} panicked");
    }
}

mod reached_shell_corpus {
    use super::virtio_support::*;
    use chaoscontrol_sim_core::ScratchClassLimit;
    use chaoscontrol_vmm::devices::block::DeterministicBlock;
    use chaoscontrol_vmm::devices::entropy::DeterministicEntropy;
    use chaoscontrol_vmm::devices::net::DeterministicNet;
    use chaoscontrol_vmm::devices::virtio_block::VirtioBlock;
    use chaoscontrol_vmm::devices::virtio_buffer::{
        BoundedBufferAllocator, HostBufferAllocator, RejectingBufferAllocator,
    };
    use chaoscontrol_vmm::devices::virtio_chain::{
        plan_descriptor_chain, VirtqDesc, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE,
    };
    use chaoscontrol_vmm::devices::virtio_entropy::VirtioEntropy;
    use chaoscontrol_vmm::devices::virtio_mmio::VirtioMmioDevice;
    use chaoscontrol_vmm::devices::virtio_net::VirtioNet;
    use chaoscontrol_vmm::devices::virtio_request::{
        plan_block_failure, plan_chunked_scratch, BLOCK_HEADER_BYTES, NET_HEADER_BYTES,
        VIRTIO_BLK_T_IN,
    };
    use chaoscontrol_vmm::devices::virtio_status::{
        plan_completion_transition, queue_processing_allowed, validate_queue_activation,
        validate_queue_mutation, validate_status_transition, CompletionTransition, QueueMutation,
        VIRTIO_F_VERSION_1,
    };
    use chaoscontrol_vmm::devices::virtio_types::{
        LastRequestLiveOutcome, VirtioFailure, VirtioLimits,
    };
    use chaoscontrol_vmm::devices::virtio_validation::MemoryRegion;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    const SEEDS: usize = 32;
    const SCRATCH_BYTES: usize = 256;
    const DISK_BYTES: usize = 4096;
    const PAYLOAD_BYTES: usize = 64;
    const MAX_NET_FRAME_BYTES: usize = 128;
    const BLOCK_BYTES: usize = 512;
    const PAGE_BYTES: usize = 4096;
    const BASE_MEMORY_BYTES: usize = 128 * 1024;

    // Same thread-local allocator-attempt technique as capacity_allocation_probe.
    // Setup, observation/export and test assertions remain outside the measured path.
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
    // SAFETY: every allocation/deallocation delegates unchanged to System.
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
    pub(super) fn probe<T>(operation: impl FnOnce() -> T) -> (T, usize) {
        ATTEMPTS.with(|count| count.set(0));
        TRACKING.with(|enabled| assert!(!enabled.replace(true)));
        let guard = TrackingGuard;
        let result = operation();
        drop(guard);
        (result, ATTEMPTS.with(Cell::get))
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Family {
        Block,
        Entropy,
        NetTx,
        NetRx,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Case {
        Valid,
        Direction,
        UnknownFlags,
        OutsideMemory,
        Excessive,
        AllocationFailure,
    }

    fn allocator(reject: bool) -> Box<dyn BoundedBufferAllocator> {
        if reject {
            Box::new(RejectingBufferAllocator)
        } else {
            Box::new(
                HostBufferAllocator::try_from_classes(&[ScratchClassLimit {
                    slot_bytes: SCRATCH_BYTES,
                    slots: 1,
                }])
                .unwrap(),
            )
        }
    }

    #[test]
    fn generated_memory_and_request_families_have_bounded_real_outcomes() {
        let (calibration, attempts) = probe(|| Vec::<u8>::with_capacity(SCRATCH_BYTES));
        assert!(
            attempts > 0,
            "allocator probe must observe real allocations"
        );
        drop(calibration);
        let mut reached = [[0usize; 6]; 4];
        for seed in 0..SEEDS {
            for (family_index, family) in
                [Family::Block, Family::Entropy, Family::NetTx, Family::NetRx]
                    .into_iter()
                    .enumerate()
            {
                for (case_index, case) in [
                    Case::Valid,
                    Case::Direction,
                    Case::UnknownFlags,
                    Case::OutsideMemory,
                    Case::Excessive,
                    Case::AllocationFailure,
                ]
                .into_iter()
                .enumerate()
                {
                    // RX consumes retained packet storage directly; it has no scratch reservation.
                    if family == Family::NetRx && case == Case::AllocationFailure {
                        continue;
                    }
                    let memory_bytes = BASE_MEMORY_BYTES + seed * PAGE_BYTES;
                    let mem =
                        GuestMemoryMmap::from_ranges(&[(GuestAddress(0), memory_bytes)]).unwrap();
                    let limits = VirtioLimits {
                        scratch_bytes: SCRATCH_BYTES,
                        max_block_transfer_bytes: 1024,
                        max_entropy_transfer_bytes: 512,
                        max_net_frame_bytes: MAX_NET_FRAME_BYTES as u64,
                        max_aggregate_bytes: 4096,
                        ..VirtioLimits::default()
                    };
                    let byte = seed as u8 + 1;
                    let payload = [byte; PAYLOAD_BYTES];
                    let reject = case == Case::AllocationFailure;
                    let backend: Box<dyn chaoscontrol_vmm::devices::virtio_mmio::VirtioBackend> =
                        match family {
                            Family::Block => {
                                let mut disk = DeterministicBlock::new(DISK_BYTES);
                                disk.write(0, &[byte; BLOCK_BYTES]).unwrap();
                                Box::new(VirtioBlock::with_allocator(disk, allocator(reject)))
                            }
                            Family::Entropy => Box::new(VirtioEntropy::with_allocator(
                                DeterministicEntropy::new(seed as u64),
                                allocator(reject),
                            )),
                            Family::NetTx | Family::NetRx => {
                                let mut net = DeterministicNet::new([2, 0, 0, 0, 0, byte]);
                                if family == Family::NetRx {
                                    let packet = if case == Case::Excessive {
                                        vec![byte; MAX_NET_FRAME_BYTES + 1]
                                    } else {
                                        payload.to_vec()
                                    };
                                    net.inject_packet(packet);
                                }
                                Box::new(VirtioNet::with_allocator(net, allocator(reject)))
                            }
                        };
                    let mut device =
                        VirtioMmioDevice::with_limits(DEVICE_BASE, DEVICE_IRQ, backend, limits);
                    let queue = u32::from(family == Family::NetTx);
                    negotiate_features(&mut device, &mem);
                    configure_queue(&mut device, &mem, queue);
                    finish_driver(&mut device, &mem);
                    let data_address = DATA_ADDRESS + (seed as u64 % 8) * 16;
                    let mut data = VirtqDesc {
                        addr: data_address,
                        len: match family {
                            Family::Block => BLOCK_BYTES as u32,
                            Family::Entropy => PAYLOAD_BYTES as u32,
                            Family::NetTx | Family::NetRx => {
                                NET_HEADER_BYTES as u32 + PAYLOAD_BYTES as u32
                            }
                        },
                        flags: if family == Family::NetTx {
                            0
                        } else {
                            VIRTQ_DESC_F_WRITE
                        },
                        next: 0,
                    };
                    match case {
                        Case::Direction => data.flags ^= VIRTQ_DESC_F_WRITE,
                        Case::UnknownFlags => data.flags |= 8,
                        Case::OutsideMemory => data.addr = memory_bytes as u64 - 1,
                        Case::Excessive => {
                            data.len = match family {
                                Family::Block => 1536,
                                Family::Entropy => 513,
                                Family::NetTx | Family::NetRx => {
                                    NET_HEADER_BYTES as u32 + MAX_NET_FRAME_BYTES as u32 + 1
                                }
                            }
                        }
                        _ => {}
                    }
                    mem.write_slice(&[0xa5; BLOCK_BYTES * 3], GuestAddress(data_address))
                        .unwrap();
                    if family == Family::NetTx {
                        mem.write_slice(
                            &[0; NET_HEADER_BYTES as usize],
                            GuestAddress(data_address),
                        )
                        .unwrap();
                        mem.write_slice(&payload, GuestAddress(data_address + NET_HEADER_BYTES))
                            .unwrap();
                    }
                    if family == Family::Block {
                        mem.write_obj(VIRTIO_BLK_T_IN, GuestAddress(HEADER_ADDRESS))
                            .unwrap();
                        mem.write_obj(0u32, GuestAddress(HEADER_ADDRESS + 4))
                            .unwrap();
                        mem.write_obj(0u64, GuestAddress(HEADER_ADDRESS + 8))
                            .unwrap();
                        write_descriptor(
                            &mem,
                            0,
                            VirtqDesc {
                                addr: HEADER_ADDRESS,
                                len: BLOCK_HEADER_BYTES,
                                flags: VIRTQ_DESC_F_NEXT,
                                next: 1,
                            },
                        );
                        data.flags |= VIRTQ_DESC_F_NEXT;
                        data.next = 2;
                        write_descriptor(&mem, 1, data);
                        write_descriptor(
                            &mem,
                            2,
                            VirtqDesc {
                                addr: STATUS_ADDRESS,
                                len: 1,
                                flags: VIRTQ_DESC_F_WRITE,
                                next: 0,
                            },
                        );
                        mem.write_obj(0xa5u8, GuestAddress(STATUS_ADDRESS)).unwrap();
                    } else {
                        write_descriptor(&mem, 0, data);
                    }
                    publish_head(&mem, 0, 1);
                    let mut before = vec![0; memory_bytes];
                    mem.read_slice(&mut before, GuestAddress(0)).unwrap();
                    let (completed, allocations) = probe(|| notify(&mut device, &mem, queue));
                    assert_eq!(
                        allocations, 0,
                        "{family:?}/{case:?}/{seed}: initialized shell may not allocate"
                    );
                    reached[family_index][case_index] += 1;
                    let request_error = family == Family::Block
                        && matches!(case, Case::Direction | Case::Excessive);
                    let accepted = case == Case::Valid || request_error;
                    assert_eq!(completed, accepted, "{family:?}/{case:?}/{seed}");
                    let state = device.live_state();
                    assert_eq!(
                        state.queues[queue as usize].last_avail_idx,
                        u16::from(accepted)
                    );
                    assert_eq!(used_index(&mem), u16::from(accepted));
                    assert_eq!(device.interrupt_pending(), accepted);
                    if !accepted {
                        assert!(matches!(
                            state.failure,
                            Some(
                                VirtioFailure::Descriptor(_)
                                    | VirtioFailure::Request(_)
                                    | VirtioFailure::Resource(_)
                            )
                        ));
                        let mut after = vec![0; memory_bytes];
                        mem.read_slice(&mut after, GuestAddress(0)).unwrap();
                        assert_eq!(after, before, "rejected request cannot change guest memory");
                    } else {
                        assert_eq!(state.failure, None);
                    }
                    let observations = match family {
                        Family::Block => {
                            let block = device
                                .backend()
                                .as_any()
                                .downcast_ref::<VirtioBlock>()
                                .unwrap();
                            assert_eq!(block.disk().stats().writes, 1, "only setup writes disk");
                            let mut disk_bytes = [0; BLOCK_BYTES];
                            block.disk().clone().read(0, &mut disk_bytes).unwrap();
                            assert_eq!(disk_bytes, [byte; BLOCK_BYTES]);
                            let mut output = [0; BLOCK_BYTES];
                            mem.read_slice(&mut output, GuestAddress(data_address))
                                .unwrap();
                            assert_eq!(
                                output,
                                if case == Case::Valid {
                                    [byte; BLOCK_BYTES]
                                } else {
                                    [0xa5; BLOCK_BYTES]
                                }
                            );
                            if accepted {
                                assert_eq!(
                                    mem.read_obj::<u8>(GuestAddress(STATUS_ADDRESS)).unwrap(),
                                    u8::from(request_error)
                                );
                            }
                            if request_error {
                                assert!(matches!(
                                    state.queues[0].last_request_outcome,
                                    Some(LastRequestLiveOutcome::Rejected { .. })
                                ));
                            }
                            block.scratch_capacity_observations()
                        }
                        Family::Entropy => {
                            let entropy = device
                                .backend()
                                .as_any()
                                .downcast_ref::<VirtioEntropy>()
                                .unwrap();
                            assert_eq!(
                                entropy.entropy().bytes_generated(),
                                if accepted { PAYLOAD_BYTES as u64 } else { 0 }
                            );
                            if accepted {
                                let mut expected = [0; PAYLOAD_BYTES];
                                DeterministicEntropy::new(seed as u64).fill_bytes(&mut expected);
                                let mut output = [0; PAYLOAD_BYTES];
                                mem.read_slice(&mut output, GuestAddress(data_address))
                                    .unwrap();
                                assert_eq!(output, expected);
                            }
                            entropy.scratch_capacity_observations()
                        }
                        Family::NetTx | Family::NetRx => {
                            let net = device
                                .backend_mut()
                                .as_any_mut()
                                .downcast_mut::<VirtioNet>()
                                .unwrap();
                            if family == Family::NetTx {
                                let packets = net.net_mut().drain_tx();
                                assert_eq!(
                                    packets,
                                    if accepted {
                                        vec![payload.to_vec()]
                                    } else {
                                        vec![]
                                    }
                                );
                            } else {
                                assert_eq!(net.net().has_rx_data(), !accepted);
                                if accepted {
                                    let mut output = [0; PAYLOAD_BYTES];
                                    mem.read_slice(
                                        &mut output,
                                        GuestAddress(data_address + NET_HEADER_BYTES),
                                    )
                                    .unwrap();
                                    assert_eq!(output, payload);
                                }
                            }
                            net.scratch_capacity_observations()
                        }
                    };
                    assert!(observations.allocated_bytes <= SCRATCH_BYTES);
                    assert!(observations.high_water <= 1);
                    assert_eq!(observations.in_use, 0);
                }
            }
        }
        assert_eq!(
            reached,
            [
                [SEEDS; 6],
                [SEEDS; 6],
                [SEEDS; 6],
                [SEEDS, SEEDS, SEEDS, SEEDS, SEEDS, 0]
            ],
            "each applicable request and shell family must be reached"
        );
    }

    #[test]
    fn lifecycle_and_failure_plans_replay_without_allocation() {
        for ready in [false, true] {
            for mutation in [
                QueueMutation::Size,
                QueueMutation::Address,
                QueueMutation::Activate,
            ] {
                let ((first, second), attempts) = probe(|| {
                    (
                        validate_queue_mutation(ready, mutation),
                        validate_queue_mutation(ready, mutation),
                    )
                });
                assert_eq!(attempts, 0);
                assert_eq!(first, second);
                assert_eq!(first.is_ok(), !ready);
            }
        }
        for current in [0, 1, 3, 11, 15, 143, 79] {
            for next in [0, 1, 3, 11, 15, 143, 79, u32::MAX] {
                let facts = || {
                    (
                        validate_status_transition(
                            current,
                            next,
                            VIRTIO_F_VERSION_1,
                            VIRTIO_F_VERSION_1,
                        ),
                        validate_queue_activation(next, 1),
                        queue_processing_allowed(next, true),
                        plan_chunked_scratch(u64::MAX, SCRATCH_BYTES),
                    )
                };
                let ((first, second), attempts) = probe(|| (facts(), facts()));
                assert_eq!(attempts, 0);
                assert_eq!(first, second);
                assert_eq!(first.2, next == 15);
                assert_eq!(first.3, Ok((SCRATCH_BYTES, SCRATCH_BYTES)));
            }
        }
        assert!(plan_chunked_scratch(1, 0).is_err());
        assert_eq!(
            plan_chunked_scratch(7, SCRATCH_BYTES),
            Ok((7, SCRATCH_BYTES))
        );
        for writable in [false, true] {
            let descriptors = [VirtqDesc {
                addr: DATA_ADDRESS,
                len: 1,
                flags: if writable { VIRTQ_DESC_F_WRITE } else { 0 },
                next: 0,
            }];
            let memory = [MemoryRegion {
                start: 0,
                length: MEMORY_BYTES as u64,
            }];
            let ((first, second), attempts) = probe(|| {
                let facts = || {
                    let chain = plan_descriptor_chain(
                        &descriptors,
                        0,
                        QUEUE_SIZE,
                        &memory,
                        VirtioLimits::default(),
                    )
                    .unwrap();
                    plan_block_failure(&chain)
                };
                (facts(), facts())
            });
            assert_eq!(attempts, 0);
            assert_eq!(first, second);
            assert_eq!(
                first,
                if writable {
                    chaoscontrol_vmm::devices::virtio_request::BlockFailurePlan::ErrorCompletion {
                        status_address: DATA_ADDRESS,
                    }
                } else {
                    chaoscontrol_vmm::devices::virtio_request::BlockFailurePlan::NeedsReset
                }
            );
        }
    }

    #[test]
    fn completion_plans_reject_duplicate_staging_and_mismatched_publication() {
        use CompletionTransition::*;
        let stage = Stage {
            head_index: 3,
            written_length: 64,
        };
        let publish = Publish {
            head_index: 3,
            written_length: 64,
        };
        let ((staged, replayed), attempts) = probe(|| {
            (
                plan_completion_transition(None, stage),
                plan_completion_transition(None, stage),
            )
        });
        assert_eq!(attempts, 0);
        assert_eq!(staged, replayed);
        let staged = staged.unwrap();
        assert!(!staged.unwrap().backend_started);
        assert!(!staged.unwrap().effects_started);
        assert_eq!(
            plan_completion_transition(staged, stage),
            Err(VirtioFailure::CompletionState)
        );
        for transition in [BackendStarted, EffectsStarted, publish] {
            assert_eq!(
                plan_completion_transition(None, transition),
                Err(VirtioFailure::CompletionState)
            );
        }
        for transition in [
            Publish {
                head_index: 4,
                written_length: 64,
            },
            Publish {
                head_index: 3,
                written_length: 63,
            },
        ] {
            assert_eq!(
                plan_completion_transition(staged, transition),
                Err(VirtioFailure::CompletionState)
            );
        }
        for backend in [false, true] {
            let transition = if backend {
                BackendStarted
            } else {
                EffectsStarted
            };
            let ((first, second), attempts) = probe(|| {
                (
                    plan_completion_transition(staged, transition),
                    plan_completion_transition(staged, transition),
                )
            });
            assert_eq!(attempts, 0);
            assert_eq!(first, second);
            let effects = first.unwrap();
            assert!(effects.unwrap().effects_started);
            assert_eq!(effects.unwrap().backend_started, backend);
            assert_eq!(plan_completion_transition(effects, publish), Ok(None));
        }
    }
}
