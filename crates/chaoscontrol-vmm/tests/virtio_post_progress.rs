mod virtio_support;

use chaoscontrol_vmm::devices::block::{BlockFault, DeterministicBlock};
use chaoscontrol_vmm::devices::entropy::DeterministicEntropy;
use chaoscontrol_vmm::devices::net::DeterministicNet;
use chaoscontrol_vmm::devices::virtio_block::VirtioBlock;
use chaoscontrol_vmm::devices::virtio_chain::{VirtqDesc, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE};
use chaoscontrol_vmm::devices::virtio_entropy::VirtioEntropy;
use chaoscontrol_vmm::devices::virtio_mmio::VirtioMmioDevice;
use chaoscontrol_vmm::devices::virtio_net::VirtioNet;
use chaoscontrol_vmm::devices::virtio_request::{
    BLOCK_HEADER_BYTES, BLOCK_SECTOR_BYTES, NET_HEADER_BYTES, VIRTIO_BLK_T_OUT,
};
use chaoscontrol_vmm::devices::virtio_types::{UsedWriteFailurePoint, VirtioFailure};
use virtio_support::*;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DISK_BYTES: usize = 64 * 1024;
const BLOCK_TRANSFER_BYTES: usize = BLOCK_SECTOR_BYTES as usize;
const HEADER_RESERVED_OFFSET: u64 = 4;
const HEADER_SECTOR_OFFSET: u64 = 8;
const PARTIAL_WRITE_BYTES: usize = 1;
const ENTROPY_BYTES: u32 = 32;
const NET_PACKET_BYTES: usize = 32;
const NET_HEADER_BYTES_USIZE: usize = NET_HEADER_BYTES as usize;
const NET_HEADER_BYTES_U32: u32 = NET_HEADER_BYTES as u32;
const NET_RX_BUFFER_BYTES: u32 = NET_HEADER_BYTES_U32 + NET_PACKET_BYTES as u32;
const PACKET_FILL: u8 = 0x5A;
const BLOCK_FILL: u8 = 0xC3;
const TEST_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
const BLOCK_QUEUE: u32 = 0;
const ENTROPY_QUEUE: u32 = 0;
const NET_RX_QUEUE: u32 = 0;
const NET_TX_QUEUE: u32 = 1;

fn configure(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap, queue: u32) {
    negotiate_features(device, mem);
    configure_queue(device, mem, queue);
    finish_driver(device, mem);
}

fn assert_poisoned(device: &VirtioMmioDevice, queue_index: usize, failure: VirtioFailure) {
    let live = device.live_state();
    assert_eq!(live.failure, Some(failure.clone()));
    let queue = &live.queues[queue_index];
    assert_eq!(queue.failure, Some(failure));
    assert_eq!(queue.last_avail_idx, 0);
    assert_eq!(queue.next_used_idx, 0);
    assert!(queue
        .pending_completion
        .is_some_and(|pending| { pending.backend_started && pending.effects_started }));
    assert!(!device.interrupt_pending());
}

fn write_block_request(mem: &GuestMemoryMmap) {
    mem.write_obj(VIRTIO_BLK_T_OUT, GuestAddress(HEADER_ADDRESS))
        .expect("operation");
    mem.write_obj(0u32, GuestAddress(HEADER_ADDRESS + HEADER_RESERVED_OFFSET))
        .expect("reserved");
    mem.write_obj(0u64, GuestAddress(HEADER_ADDRESS + HEADER_SECTOR_OFFSET))
        .expect("sector");
    mem.write_slice(
        &[BLOCK_FILL; BLOCK_TRANSFER_BYTES],
        GuestAddress(DATA_ADDRESS),
    )
    .expect("block payload");
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
            len: BLOCK_SECTOR_BYTES as u32,
            flags: VIRTQ_DESC_F_NEXT,
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
    publish_head(mem, 0, 1);
}

#[test]
fn block_torn_write_poison_is_observable_without_completion() {
    let mem = memory();
    let mut disk = DeterministicBlock::new(DISK_BYTES);
    disk.inject_fault(BlockFault::TornWrite {
        offset: 0,
        bytes_written: PARTIAL_WRITE_BYTES,
    });
    let mut device =
        VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(VirtioBlock::new(disk)));
    configure(&mut device, &mem, BLOCK_QUEUE);
    write_block_request(&mem);

    assert!(!notify(&mut device, &mem, BLOCK_QUEUE));
    assert_eq!(used_index(&mem), 0);
    let backend = device
        .backend()
        .as_any()
        .downcast_ref::<VirtioBlock>()
        .expect("block backend");
    assert_eq!(backend.disk().stats().writes, 1);
    assert_eq!(
        backend.disk().stats().bytes_written,
        PARTIAL_WRITE_BYTES as u64
    );
    assert_poisoned(&device, BLOCK_QUEUE as usize, VirtioFailure::BackendWrite);
}

fn write_net_packet(mem: &GuestMemoryMmap) {
    mem.write_slice(&[0; NET_HEADER_BYTES_USIZE], GuestAddress(DATA_ADDRESS))
        .expect("net header");
    mem.write_slice(
        &[PACKET_FILL; NET_PACKET_BYTES],
        GuestAddress(DATA_ADDRESS + NET_HEADER_BYTES),
    )
    .expect("net payload");
    let length = NET_HEADER_BYTES_USIZE + NET_PACKET_BYTES;
    write_descriptor(
        mem,
        0,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: u32::try_from(length).expect("packet length"),
            flags: 0,
            next: 0,
        },
    );
    publish_head(mem, 0, 1);
}

#[test]
fn net_post_commit_failure_poison_is_observable_without_completion() {
    let mem = memory();
    let mut net = DeterministicNet::new(TEST_MAC);
    net.inject_failure_after_next_tx_enqueue();
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(VirtioNet::new(net)));
    configure(&mut device, &mem, NET_TX_QUEUE);
    write_net_packet(&mem);

    assert!(!notify(&mut device, &mem, NET_TX_QUEUE));
    assert_eq!(used_index(&mem), 0);
    let backend = device
        .backend()
        .as_any()
        .downcast_ref::<VirtioNet>()
        .expect("net backend");
    assert_eq!(backend.net().tx_queued_packets(), 1);
    assert_eq!(backend.net().tx_queued_bytes(), NET_PACKET_BYTES as u64);
    assert_poisoned(&device, NET_TX_QUEUE as usize, VirtioFailure::BackendWrite);
}

#[test]
fn net_rx_used_ring_failure_retains_post_progress_state() {
    let mem = memory();
    let mut net = DeterministicNet::new(TEST_MAC);
    net.inject_packet(vec![PACKET_FILL; NET_PACKET_BYTES]);
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(VirtioNet::new(net)));
    configure(&mut device, &mem, NET_RX_QUEUE);
    write_descriptor(
        &mem,
        0,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: NET_RX_BUFFER_BYTES,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    );
    publish_head(&mem, 0, 1);
    device
        .inject_used_write_failure(NET_RX_QUEUE as usize, UsedWriteFailurePoint::BeforeIndex)
        .expect("used-ring failure");

    assert!(!notify(&mut device, &mem, NET_RX_QUEUE));
    let backend = device
        .backend()
        .as_any()
        .downcast_ref::<VirtioNet>()
        .expect("net backend");
    assert!(!backend.net().has_rx_data());
    let mut payload = [0; NET_PACKET_BYTES];
    mem.read_slice(&mut payload, GuestAddress(DATA_ADDRESS + NET_HEADER_BYTES))
        .expect("guest RX payload");
    assert_eq!(payload, [PACKET_FILL; NET_PACKET_BYTES]);
    assert_eq!(used_index(&mem), 0);
    assert_poisoned(&device, NET_RX_QUEUE as usize, VirtioFailure::UsedRingWrite);
}

fn entropy_device() -> (GuestMemoryMmap, VirtioMmioDevice) {
    let mem = memory();
    let entropy = VirtioEntropy::new(DeterministicEntropy::new(0));
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(entropy));
    configure(&mut device, &mem, ENTROPY_QUEUE);
    write_descriptor(
        &mem,
        0,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: ENTROPY_BYTES,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    );
    publish_head(&mem, 0, 1);
    (mem, device)
}

fn entropy_bytes_generated(device: &VirtioMmioDevice) -> u64 {
    device
        .backend()
        .as_any()
        .downcast_ref::<VirtioEntropy>()
        .expect("entropy backend")
        .entropy()
        .bytes_generated()
}

#[test]
fn entropy_post_commit_failure_poison_is_observable_without_completion() {
    let (mem, mut device) = entropy_device();
    device
        .backend_mut()
        .as_any_mut()
        .downcast_mut::<VirtioEntropy>()
        .expect("entropy backend")
        .inject_failure_after_entropy_commit();
    assert!(!notify(&mut device, &mem, ENTROPY_QUEUE));
    assert_eq!(entropy_bytes_generated(&device), u64::from(ENTROPY_BYTES));
    assert_eq!(used_index(&mem), 0);
    assert_poisoned(&device, ENTROPY_QUEUE as usize, VirtioFailure::BackendWrite);
}

#[test]
fn used_ring_failure_keeps_used_index_as_completion_authority() {
    let (mem, mut device) = entropy_device();
    device
        .inject_used_write_failure(ENTROPY_QUEUE as usize, UsedWriteFailurePoint::BeforeIndex)
        .expect("failure injection");
    assert!(!notify(&mut device, &mem, ENTROPY_QUEUE));
    assert_eq!(entropy_bytes_generated(&device), u64::from(ENTROPY_BYTES));
    assert_eq!(used_index(&mem), 0);
    assert_poisoned(
        &device,
        ENTROPY_QUEUE as usize,
        VirtioFailure::UsedRingWrite,
    );
}
