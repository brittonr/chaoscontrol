mod virtio_support;

use chaoscontrol_vmm::devices::entropy::DeterministicEntropy;
use chaoscontrol_vmm::devices::net::DeterministicNet;
use chaoscontrol_vmm::devices::virtio_buffer::RejectingBufferAllocator;
use chaoscontrol_vmm::devices::virtio_chain::{VirtqDesc, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE};
use chaoscontrol_vmm::devices::virtio_entropy::VirtioEntropy;
use chaoscontrol_vmm::devices::virtio_mmio::{VirtioMmioDevice, VIRTIO_MMIO_CONFIG};
use chaoscontrol_vmm::devices::virtio_net::VirtioNet;
use chaoscontrol_vmm::devices::virtio_request::NET_HEADER_BYTES;
use chaoscontrol_vmm::devices::virtio_types::{ResourceViolation, VirtioFailure, VirtioLimits};
use virtio_support::*;
use vm_memory::{Bytes, GuestAddress};

const TEST_SEED: u64 = 42;
const ENTROPY_BYTES: u32 = 64;
const FIRST_AVAILABLE_INDEX: u16 = 1;
const NET_QUEUE_RX: u32 = 0;
const NET_QUEUE_TX: u32 = 1;
const NET_PAYLOAD_BYTES: usize = 64;
const NET_PAYLOAD_BYTES_U32: u32 = 64;
const TEST_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
const MAC_LAST_BYTE_OFFSET: u64 = 5;
const MAC_LAST_BYTE_INDEX: usize = 5;
const WIDE_CONFIG_BYTES: usize = 4;
const INITIAL_CONFIG_BYTE: u8 = 0xA5;

fn configure(device: &mut VirtioMmioDevice, mem: &vm_memory::GuestMemoryMmap, queue: u32) {
    negotiate_features(device, mem);
    configure_queue(device, mem, queue);
    finish_driver(device, mem);
}

fn entropy_backend(device: &VirtioMmioDevice) -> &VirtioEntropy {
    device
        .backend()
        .as_any()
        .downcast_ref::<VirtioEntropy>()
        .expect("entropy backend")
}

fn net_backend_mut(device: &mut VirtioMmioDevice) -> &mut VirtioNet {
    device
        .backend_mut()
        .as_any_mut()
        .downcast_mut::<VirtioNet>()
        .expect("net backend")
}

fn entropy_descriptor(mem: &vm_memory::GuestMemoryMmap, flags: u16) {
    write_descriptor(
        mem,
        0,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: ENTROPY_BYTES,
            flags,
            next: 0,
        },
    );
    publish_head(mem, 0, FIRST_AVAILABLE_INDEX);
}

#[test]
fn net_config_cross_boundary_read_zeroes_tail() {
    let net = VirtioNet::new(DeterministicNet::new(TEST_MAC));
    let device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(net));
    let mut output = [INITIAL_CONFIG_BYTE; WIDE_CONFIG_BYTES];
    device.read(VIRTIO_MMIO_CONFIG + MAC_LAST_BYTE_OFFSET, &mut output);
    assert_eq!(output, [TEST_MAC[MAC_LAST_BYTE_INDEX], 0, 0, 0]);
}

#[test]
fn valid_entropy_request_advances_only_after_plan_acceptance() {
    let mem = memory();
    let entropy = VirtioEntropy::new(DeterministicEntropy::new(TEST_SEED));
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(entropy));
    configure(&mut device, &mem, 0);
    entropy_descriptor(&mem, VIRTQ_DESC_F_WRITE);

    assert!(notify(&mut device, &mem, 0));
    assert_eq!(
        entropy_backend(&device).entropy().bytes_generated(),
        u64::from(ENTROPY_BYTES)
    );
    assert_eq!(used_index(&mem), FIRST_AVAILABLE_INDEX);
    assert!(device.interrupt_pending());
}

#[test]
fn invalid_entropy_direction_does_not_advance_or_commit() {
    let mem = memory();
    let entropy = VirtioEntropy::new(DeterministicEntropy::new(TEST_SEED));
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(entropy));
    configure(&mut device, &mem, 0);
    entropy_descriptor(&mem, 0);

    assert!(!notify(&mut device, &mem, 0));
    assert_eq!(entropy_backend(&device).entropy().bytes_generated(), 0);
    assert_eq!(used_index(&mem), 0);
    assert_eq!(device.live_state().queues[0].last_avail_idx, 0);
    assert!(!device.interrupt_pending());
}

#[test]
fn entropy_allocation_failure_preserves_prng_and_queue() {
    let mem = memory();
    let entropy = VirtioEntropy::with_allocator(
        DeterministicEntropy::new(TEST_SEED),
        Box::new(RejectingBufferAllocator),
    );
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(entropy));
    configure(&mut device, &mem, 0);
    entropy_descriptor(&mem, VIRTQ_DESC_F_WRITE);

    assert!(!notify(&mut device, &mem, 0));
    assert_eq!(entropy_backend(&device).entropy().bytes_generated(), 0);
    assert_eq!(used_index(&mem), 0);
    assert!(matches!(
        device.live_state().failure,
        Some(VirtioFailure::Resource(
            ResourceViolation::Allocation { .. }
        ))
    ));
}

#[test]
fn valid_net_tx_excludes_virtio_header_and_commits_once() {
    let mem = memory();
    let net = VirtioNet::new(DeterministicNet::new(TEST_MAC));
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(net));
    configure(&mut device, &mem, NET_QUEUE_TX);
    let header_bytes = u32::try_from(NET_HEADER_BYTES).expect("header bytes");
    mem.write_slice(
        &[0u8; NET_HEADER_BYTES as usize],
        GuestAddress(HEADER_ADDRESS),
    )
    .expect("net header");
    mem.write_slice(&[0x5A; NET_PAYLOAD_BYTES], GuestAddress(DATA_ADDRESS))
        .expect("net payload");
    write_descriptor(
        &mem,
        0,
        VirtqDesc {
            addr: HEADER_ADDRESS,
            len: header_bytes,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        },
    );
    write_descriptor(
        &mem,
        1,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: NET_PAYLOAD_BYTES_U32,
            flags: 0,
            next: 0,
        },
    );
    publish_head(&mem, 0, FIRST_AVAILABLE_INDEX);

    assert!(notify(&mut device, &mem, NET_QUEUE_TX));
    let packets = net_backend_mut(&mut device).net_mut().drain_tx();
    assert_eq!(packets, vec![vec![0x5A; NET_PAYLOAD_BYTES]]);
    assert_eq!(used_index(&mem), FIRST_AVAILABLE_INDEX);
    assert!(device.interrupt_pending());
}

#[test]
fn valid_net_rx_writes_header_and_packet_before_backend_pop() {
    let mem = memory();
    let mut net = DeterministicNet::new(TEST_MAC);
    net.inject_packet(vec![0xC3; NET_PAYLOAD_BYTES]);
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(VirtioNet::new(net)));
    configure(&mut device, &mem, NET_QUEUE_RX);
    let capacity = NET_HEADER_BYTES + u64::try_from(NET_PAYLOAD_BYTES).expect("payload bytes");
    write_descriptor(
        &mem,
        0,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: u32::try_from(capacity).expect("capacity"),
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    );
    publish_head(&mem, 0, FIRST_AVAILABLE_INDEX);

    assert!(notify(&mut device, &mem, NET_QUEUE_RX));
    let mut output = vec![0u8; usize::try_from(capacity).expect("capacity")];
    mem.read_slice(&mut output, GuestAddress(DATA_ADDRESS))
        .expect("rx output");
    assert_eq!(
        &output[..NET_HEADER_BYTES as usize],
        &[0u8; NET_HEADER_BYTES as usize]
    );
    assert_eq!(
        &output[NET_HEADER_BYTES as usize..],
        &[0xC3; NET_PAYLOAD_BYTES]
    );
    assert!(!net_backend_mut(&mut device).net().has_rx_data());
    assert_eq!(used_index(&mem), FIRST_AVAILABLE_INDEX);
}

#[test]
fn oversized_net_tx_fails_before_backend_cursor_or_interrupt() {
    let mem = memory();
    let net = VirtioNet::new(DeterministicNet::new(TEST_MAC));
    let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(net));
    configure(&mut device, &mem, NET_QUEUE_TX);
    let excessive = VirtioLimits::default().max_net_frame_bytes + 1;
    write_descriptor(
        &mem,
        0,
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: u32::try_from(NET_HEADER_BYTES + excessive).expect("excessive length"),
            flags: 0,
            next: 0,
        },
    );
    publish_head(&mem, 0, FIRST_AVAILABLE_INDEX);

    assert!(!notify(&mut device, &mem, NET_QUEUE_TX));
    assert!(net_backend_mut(&mut device).net_mut().drain_tx().is_empty());
    assert_eq!(used_index(&mem), 0);
    assert_eq!(device.live_state().queues[1].last_avail_idx, 0);
    assert!(!device.interrupt_pending());
}
