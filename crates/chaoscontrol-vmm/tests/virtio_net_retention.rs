mod virtio_support;

use chaoscontrol_vmm::devices::net::DeterministicNet;
use chaoscontrol_vmm::devices::virtio_chain::VirtqDesc;
use chaoscontrol_vmm::devices::virtio_mmio::{VirtioMmioDevice, VIRTIO_MMIO_INTERRUPT_ACK};
use chaoscontrol_vmm::devices::virtio_net::VirtioNet;
use chaoscontrol_vmm::devices::virtio_request::NET_HEADER_BYTES;
use chaoscontrol_vmm::devices::virtio_types::{ResourceViolation, VirtioFailure, VirtioLimits};
use virtio_support::*;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const TEST_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
const TX_QUEUE: u32 = 1;
const RETAINED_PACKETS: usize = 2;
const RETAINED_BYTES: u64 = 128;
const FULL_PACKET_BYTES: usize = 64;
const PLUS_ONE_PACKET_BYTES: usize = 1;
const FIRST_USED_INDEX: u16 = 1;
const SECOND_USED_INDEX: u16 = 2;
const THIRD_AVAILABLE_INDEX: u16 = 3;
const VRING_INTERRUPT: u32 = 1;
const PACKET_FILL: u8 = 0x5A;
const PACKET_ADDRESSES: [u64; 3] = [DATA_ADDRESS, SECOND_DATA_ADDRESS, THIRD_DATA_ADDRESS];

fn retained_device(limits: VirtioLimits) -> (GuestMemoryMmap, VirtioMmioDevice) {
    let mem = memory();
    let net = VirtioNet::new(DeterministicNet::new(TEST_MAC));
    let mut device = VirtioMmioDevice::with_limits(DEVICE_BASE, DEVICE_IRQ, Box::new(net), limits);
    negotiate_features(&mut device, &mem);
    configure_queue(&mut device, &mem, TX_QUEUE);
    finish_driver(&mut device, &mem);
    (mem, device)
}

fn write_packet(mem: &GuestMemoryMmap, descriptor: u16, packet_bytes: usize) {
    let address = PACKET_ADDRESSES[usize::from(descriptor)];
    let header_bytes = usize::try_from(NET_HEADER_BYTES).expect("header bytes");
    mem.write_slice(&[0u8; NET_HEADER_BYTES as usize], GuestAddress(address))
        .expect("net header");
    mem.write_slice(
        &vec![PACKET_FILL; packet_bytes],
        GuestAddress(address + NET_HEADER_BYTES),
    )
    .expect("net packet");
    let descriptor_bytes = header_bytes
        .checked_add(packet_bytes)
        .expect("descriptor bytes");
    write_descriptor(
        mem,
        descriptor,
        VirtqDesc {
            addr: address,
            len: u32::try_from(descriptor_bytes).expect("descriptor length"),
            flags: 0,
            next: 0,
        },
    );
}

fn submit_packet(
    mem: &GuestMemoryMmap,
    device: &mut VirtioMmioDevice,
    slot: u16,
    packet_bytes: usize,
) -> bool {
    write_packet(mem, slot, packet_bytes);
    let available_index = slot.checked_add(1).expect("available index");
    publish_head_at(mem, slot, slot, available_index);
    notify(device, mem, TX_QUEUE)
}

fn acknowledge(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap) {
    register(device, mem, VIRTIO_MMIO_INTERRUPT_ACK, VRING_INTERRUPT)
        .expect("interrupt acknowledgement");
}

fn net(device: &VirtioMmioDevice) -> &DeterministicNet {
    device
        .backend()
        .as_any()
        .downcast_ref::<VirtioNet>()
        .expect("net backend")
        .net()
}

fn fill_exact_limit(mem: &GuestMemoryMmap, device: &mut VirtioMmioDevice) {
    assert!(submit_packet(mem, device, 0, FULL_PACKET_BYTES));
    acknowledge(device, mem);
    assert!(submit_packet(
        mem,
        device,
        FIRST_USED_INDEX,
        FULL_PACKET_BYTES
    ));
    acknowledge(device, mem);
    assert_eq!(used_index(mem), SECOND_USED_INDEX);
    assert_eq!(net(device).tx_queued_packets(), RETAINED_PACKETS);
    assert_eq!(net(device).tx_queued_bytes(), RETAINED_BYTES);
    assert_eq!(net(device).stats().tx_packets, RETAINED_PACKETS as u64);
    assert_eq!(net(device).stats().tx_bytes, RETAINED_BYTES);
}

fn assert_limit_rejection(
    mem: &GuestMemoryMmap,
    device: &VirtioMmioDevice,
    expected: ResourceViolation,
) {
    assert_eq!(used_index(mem), SECOND_USED_INDEX);
    assert!(!device.interrupt_pending());
    assert_eq!(
        device.live_state().failure,
        Some(VirtioFailure::Resource(expected))
    );
    let queue = &device.live_state().queues[TX_QUEUE as usize];
    assert_eq!(queue.last_avail_idx, SECOND_USED_INDEX);
    assert_eq!(queue.next_used_idx, SECOND_USED_INDEX);
    assert!(queue.pending_completion.is_none());
    assert_eq!(net(device).tx_queued_packets(), RETAINED_PACKETS);
    assert_eq!(net(device).tx_queued_bytes(), RETAINED_BYTES);
    assert_eq!(net(device).stats().tx_packets, RETAINED_PACKETS as u64);
    assert_eq!(net(device).stats().tx_bytes, RETAINED_BYTES);
}

#[test]
fn retained_packet_limit_accepts_exact_limit_and_rejects_plus_one() {
    let limits = VirtioLimits {
        max_net_tx_packets: RETAINED_PACKETS,
        max_net_tx_bytes: RETAINED_BYTES + FULL_PACKET_BYTES as u64,
        ..VirtioLimits::default()
    };
    let (mem, mut device) = retained_device(limits);
    fill_exact_limit(&mem, &mut device);
    assert!(!submit_packet(
        &mem,
        &mut device,
        SECOND_USED_INDEX,
        PLUS_ONE_PACKET_BYTES
    ));
    assert_limit_rejection(
        &mem,
        &device,
        ResourceViolation::RetainedPacketLimit {
            requested: THIRD_AVAILABLE_INDEX as usize,
            maximum: RETAINED_PACKETS,
        },
    );
}

#[test]
fn retained_byte_limit_accepts_exact_limit_and_rejects_plus_one() {
    let limits = VirtioLimits {
        max_net_tx_packets: THIRD_AVAILABLE_INDEX as usize,
        max_net_tx_bytes: RETAINED_BYTES,
        ..VirtioLimits::default()
    };
    let (mem, mut device) = retained_device(limits);
    fill_exact_limit(&mem, &mut device);
    assert!(!submit_packet(
        &mem,
        &mut device,
        SECOND_USED_INDEX,
        PLUS_ONE_PACKET_BYTES
    ));
    assert_limit_rejection(
        &mem,
        &device,
        ResourceViolation::RetainedByteLimit {
            requested: RETAINED_BYTES + PLUS_ONE_PACKET_BYTES as u64,
            maximum: RETAINED_BYTES,
        },
    );
}
