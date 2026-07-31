mod virtio_support;

use chaoscontrol_vmm::devices::block::{BlockFault, DeterministicBlock};
use chaoscontrol_vmm::devices::virtio_block::VirtioBlock;
use chaoscontrol_vmm::devices::virtio_buffer::RejectingBufferAllocator;
use chaoscontrol_vmm::devices::virtio_chain::{VirtqDesc, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE};
use chaoscontrol_vmm::devices::virtio_mmio::{VirtioMmioDevice, VIRTIO_MMIO_CONFIG};
use chaoscontrol_vmm::devices::virtio_request::{
    BLOCK_HEADER_BYTES, BLOCK_SECTOR_BYTES, VIRTIO_BLK_T_OUT,
};
use chaoscontrol_vmm::devices::virtio_types::{
    LastRequestLiveOutcome, ResourceViolation, UsedWriteFailurePoint, VirtioFailure,
};
use virtio_support::*;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DISK_BYTES: usize = 64 * 1024;
const TRANSFER_BYTES: usize = BLOCK_SECTOR_BYTES as usize;
const TRANSFER_BYTES_U32: u32 = BLOCK_SECTOR_BYTES as u32;
const HEADER_RESERVED_OFFSET: u64 = 4;
const HEADER_SECTOR_OFFSET: u64 = 8;
const STATUS_OK: u8 = 0;
const STATUS_IOERR: u8 = 1;
const FIRST_AVAILABLE_INDEX: u16 = 1;
const SECOND_AVAILABLE_INDEX: u16 = 2;
const CAPACITY_FIELD_BYTES: usize = 8;
const WIDE_CONFIG_BYTES: usize = 12;
const CONFIG_CROSS_BOUNDARY_OFFSET: u64 = 7;
const INITIAL_CONFIG_BYTE: u8 = 0xA5;

fn block_device(block: VirtioBlock) -> VirtioMmioDevice {
    VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(block))
}

fn configure(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap) {
    negotiate_features(device, mem);
    configure_queue(device, mem, 0);
    finish_driver(device, mem);
}

fn write_request(mem: &GuestMemoryMmap, data_flags: u16, sector: u64) {
    mem.write_obj(VIRTIO_BLK_T_OUT, GuestAddress(HEADER_ADDRESS))
        .expect("operation");
    mem.write_obj(0u32, GuestAddress(HEADER_ADDRESS + HEADER_RESERVED_OFFSET))
        .expect("reserved");
    mem.write_obj(sector, GuestAddress(HEADER_ADDRESS + HEADER_SECTOR_OFFSET))
        .expect("sector");
    mem.write_slice(&[0xA5; TRANSFER_BYTES], GuestAddress(DATA_ADDRESS))
        .expect("data");
    write_descriptor(
        mem,
        0,
        VirtqDesc {
            addr: HEADER_ADDRESS,
            len: BLOCK_HEADER_BYTES,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        },
    );
    write_descriptor(
        mem,
        1,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: TRANSFER_BYTES_U32,
            flags: data_flags | VIRTQ_DESC_F_NEXT,
            next: 2,
        },
    );
    write_descriptor(
        mem,
        2,
        VirtqDesc {
            addr: STATUS_ADDRESS,
            len: 1,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    );
    publish_head(mem, 0, FIRST_AVAILABLE_INDEX);
}

fn block_backend(device: &VirtioMmioDevice) -> &VirtioBlock {
    device
        .backend()
        .as_any()
        .downcast_ref::<VirtioBlock>()
        .expect("block backend")
}

#[test]
fn block_config_reads_zero_bytes_past_capacity() {
    let device = block_device(VirtioBlock::new(DeterministicBlock::new(DISK_BYTES)));
    let expected_sectors = u64::try_from(DISK_BYTES).expect("disk bytes") / BLOCK_SECTOR_BYTES;
    let mut wide = [INITIAL_CONFIG_BYTE; WIDE_CONFIG_BYTES];
    device.read(VIRTIO_MMIO_CONFIG, &mut wide);
    assert_eq!(
        &wide[..CAPACITY_FIELD_BYTES],
        &expected_sectors.to_le_bytes()
    );
    assert_eq!(
        &wide[CAPACITY_FIELD_BYTES..],
        &[0; WIDE_CONFIG_BYTES - CAPACITY_FIELD_BYTES]
    );

    let mut crossing = [INITIAL_CONFIG_BYTE; CAPACITY_FIELD_BYTES];
    device.read(
        VIRTIO_MMIO_CONFIG + CONFIG_CROSS_BOUNDARY_OFFSET,
        &mut crossing,
    );
    assert_eq!(crossing, [0; CAPACITY_FIELD_BYTES]);
}

#[test]
fn valid_write_commits_backend_used_cursor_and_interrupt() {
    let mem = memory();
    let mut device = block_device(VirtioBlock::new(DeterministicBlock::new(DISK_BYTES)));
    configure(&mut device, &mem);
    write_request(&mem, 0, 0);

    assert!(notify(&mut device, &mem, 0));
    let mut disk_data = [0u8; TRANSFER_BYTES];
    block_backend(&device)
        .disk()
        .clone()
        .read(0, &mut disk_data)
        .expect("disk read");
    assert_eq!(disk_data, [0xA5; TRANSFER_BYTES]);
    assert_eq!(
        mem.read_obj::<u8>(GuestAddress(STATUS_ADDRESS))
            .expect("status"),
        STATUS_OK
    );
    assert_eq!(used_index(&mem), FIRST_AVAILABLE_INDEX);
    assert_eq!(
        device.live_state().queues[0].last_avail_idx,
        FIRST_AVAILABLE_INDEX
    );
    assert!(device.interrupt_pending());
}

#[test]
fn wrong_direction_retains_typed_outcome_then_allows_valid_request() {
    let mem = memory();
    let mut device = block_device(VirtioBlock::new(DeterministicBlock::new(DISK_BYTES)));
    configure(&mut device, &mem);
    write_request(&mem, VIRTQ_DESC_F_WRITE, 0);

    assert!(notify(&mut device, &mem, 0));
    assert_eq!(block_backend(&device).disk().stats().writes, 0);
    assert_eq!(
        mem.read_obj::<u8>(GuestAddress(STATUS_ADDRESS))
            .expect("status"),
        STATUS_IOERR
    );
    assert_eq!(used_index(&mem), FIRST_AVAILABLE_INDEX);
    assert!(device.live_state().failure.is_none());
    assert!(matches!(
        device.live_state().queues[0].last_request_outcome,
        Some(LastRequestLiveOutcome::Rejected { .. })
    ));

    write_request(&mem, 0, 0);
    publish_head_at(&mem, FIRST_AVAILABLE_INDEX, 0, SECOND_AVAILABLE_INDEX);
    assert!(notify(&mut device, &mem, 0));
    assert_eq!(block_backend(&device).disk().stats().writes, 1);
    assert_eq!(used_index(&mem), SECOND_AVAILABLE_INDEX);
    assert!(matches!(
        device.live_state().queues[0].last_request_outcome,
        Some(LastRequestLiveOutcome::Completed { .. })
    ));
}

#[test]
fn ioerr_outcome_survives_used_ring_publication_failure() {
    let mem = memory();
    let mut device = block_device(VirtioBlock::new(DeterministicBlock::new(DISK_BYTES)));
    configure(&mut device, &mem);
    write_request(&mem, VIRTQ_DESC_F_WRITE, 0);
    device
        .inject_used_write_failure(0, UsedWriteFailurePoint::BeforeIndex)
        .expect("used-ring failure");

    assert!(!notify(&mut device, &mem, 0));
    assert_eq!(used_index(&mem), 0);
    let queue = &device.live_state().queues[0];
    assert_eq!(queue.failure, Some(VirtioFailure::UsedRingWrite));
    assert!(matches!(
        queue.last_request_outcome,
        Some(LastRequestLiveOutcome::Rejected { .. })
    ));
    assert!(queue
        .pending_completion
        .is_some_and(|pending| { !pending.backend_started && pending.effects_started }));
    assert_eq!(
        mem.read_obj::<u8>(GuestAddress(STATUS_ADDRESS))
            .expect("status"),
        STATUS_IOERR
    );
}

#[test]
fn descriptor_cycle_stops_without_cursor_backend_or_interrupt() {
    let mem = memory();
    let mut device = block_device(VirtioBlock::new(DeterministicBlock::new(DISK_BYTES)));
    configure(&mut device, &mem);
    write_request(&mem, 0, 0);
    write_descriptor(
        &mem,
        1,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: TRANSFER_BYTES_U32,
            flags: VIRTQ_DESC_F_NEXT,
            next: 0,
        },
    );

    assert!(!notify(&mut device, &mem, 0));
    assert_eq!(block_backend(&device).disk().stats().writes, 0);
    assert_eq!(used_index(&mem), 0);
    assert_eq!(device.live_state().queues[0].last_avail_idx, 0);
    assert!(!device.interrupt_pending());
    assert!(device.live_state().queues[0].failure.is_some());
    assert!(device.live_state().queues[0].pending_completion.is_none());
}

#[test]
fn allocation_failure_has_no_cursor_backend_completion_or_interrupt() {
    let mem = memory();
    let block = VirtioBlock::with_allocator(
        DeterministicBlock::new(DISK_BYTES),
        Box::new(RejectingBufferAllocator),
    );
    let mut device = block_device(block);
    configure(&mut device, &mem);
    write_request(&mem, 0, 0);

    assert!(!notify(&mut device, &mem, 0));
    assert_eq!(block_backend(&device).disk().stats().writes, 0);
    assert_eq!(used_index(&mem), 0);
    assert_eq!(device.live_state().queues[0].last_avail_idx, 0);
    assert!(!device.interrupt_pending());
    assert!(matches!(
        device.live_state().failure,
        Some(VirtioFailure::Resource(
            ResourceViolation::Allocation { .. }
        ))
    ));
}

#[test]
fn backend_failure_has_no_cursor_or_successful_completion() {
    let mem = memory();
    let mut disk = DeterministicBlock::new(DISK_BYTES);
    disk.inject_fault(BlockFault::WriteError { offset: 0 });
    let mut device = block_device(VirtioBlock::new(disk));
    configure(&mut device, &mem);
    write_request(&mem, 0, 0);

    assert!(!notify(&mut device, &mem, 0));
    assert_eq!(used_index(&mem), 0);
    assert_eq!(device.live_state().queues[0].last_avail_idx, 0);
    assert!(!device.interrupt_pending());
    assert_eq!(
        device.live_state().failure,
        Some(VirtioFailure::BackendWrite)
    );
    let pending = device.live_state().queues[0]
        .pending_completion
        .expect("backend-started pending completion");
    assert!(pending.backend_started);
}

#[test]
fn out_of_range_storage_gets_typed_error_completion() {
    let mem = memory();
    let mut device = block_device(VirtioBlock::new(DeterministicBlock::new(DISK_BYTES)));
    configure(&mut device, &mem);
    let outside_sector = u64::try_from(DISK_BYTES).expect("disk bytes") / BLOCK_SECTOR_BYTES;
    write_request(&mem, 0, outside_sector);

    assert!(notify(&mut device, &mem, 0));
    assert_eq!(block_backend(&device).disk().stats().writes, 0);
    assert_eq!(
        mem.read_obj::<u8>(GuestAddress(STATUS_ADDRESS))
            .expect("status"),
        STATUS_IOERR
    );
    assert_eq!(used_index(&mem), FIRST_AVAILABLE_INDEX);
}
