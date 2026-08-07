#![allow(dead_code)]

use chaoscontrol_vmm::devices::virtio_chain::VirtqDesc;
use chaoscontrol_vmm::devices::virtio_mmio::{
    MmioWriteEffect, VirtioMmioDevice, VIRTIO_MMIO_DRIVER_FEATURES,
    VIRTIO_MMIO_DRIVER_FEATURES_SEL, VIRTIO_MMIO_QUEUE_DESC_HIGH, VIRTIO_MMIO_QUEUE_DESC_LOW,
    VIRTIO_MMIO_QUEUE_DEVICE_HIGH, VIRTIO_MMIO_QUEUE_DEVICE_LOW, VIRTIO_MMIO_QUEUE_DRIVER_HIGH,
    VIRTIO_MMIO_QUEUE_DRIVER_LOW, VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_MMIO_QUEUE_NUM,
    VIRTIO_MMIO_QUEUE_READY, VIRTIO_MMIO_QUEUE_SEL, VIRTIO_MMIO_STATUS,
};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

pub const MEMORY_BYTES: usize = 1024 * 1024;
pub const QUEUE_SIZE: u16 = 8;
pub const DESCRIPTOR_ADDRESS: u64 = 0x1000;
pub const AVAILABLE_ADDRESS: u64 = 0x2000;
pub const USED_ADDRESS: u64 = 0x3000;
pub const HEADER_ADDRESS: u64 = 0x4000;
pub const DATA_ADDRESS: u64 = 0x5000;
pub const STATUS_ADDRESS: u64 = 0x6000;
pub const SECOND_DATA_ADDRESS: u64 = 0x7000;
pub const THIRD_DATA_ADDRESS: u64 = 0x8000;
pub const DEVICE_BASE: u64 = 0xD000_0000;
pub const DEVICE_IRQ: u32 = 5;
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_ACKNOWLEDGE_DRIVER: u32 = 3;
const STATUS_FEATURES_OK: u32 = 11;
const STATUS_DRIVER_OK: u32 = 15;
const VERSION_ONE_HIGH_WORD: u32 = 1;
const DESCRIPTOR_BYTES: u64 = 16;
const DESCRIPTOR_LENGTH_OFFSET: u64 = 8;
const DESCRIPTOR_FLAGS_OFFSET: u64 = 12;
const DESCRIPTOR_NEXT_OFFSET: u64 = 14;
const AVAILABLE_INDEX_ADDRESS: u64 = AVAILABLE_ADDRESS + 2;
const AVAILABLE_RING_ADDRESS: u64 = AVAILABLE_ADDRESS + 4;
const AVAILABLE_RING_ELEMENT_BYTES: u64 = 2;
const USED_INDEX_ADDRESS: u64 = USED_ADDRESS + 2;

pub fn memory() -> GuestMemoryMmap {
    GuestMemoryMmap::from_ranges(&[(GuestAddress(0), MEMORY_BYTES)]).expect("guest memory")
}

pub fn register(
    device: &mut VirtioMmioDevice,
    mem: &GuestMemoryMmap,
    offset: u64,
    value: u32,
) -> Result<MmioWriteEffect, chaoscontrol_vmm::devices::virtio_types::VirtioFailure> {
    device.write(offset, &value.to_le_bytes(), mem)
}

pub fn negotiate_features(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap) {
    register(device, mem, VIRTIO_MMIO_STATUS, STATUS_ACKNOWLEDGE).expect("acknowledge");
    register(device, mem, VIRTIO_MMIO_STATUS, STATUS_ACKNOWLEDGE_DRIVER).expect("driver");
    register(
        device,
        mem,
        VIRTIO_MMIO_DRIVER_FEATURES_SEL,
        VERSION_ONE_HIGH_WORD,
    )
    .expect("feature selector");
    register(
        device,
        mem,
        VIRTIO_MMIO_DRIVER_FEATURES,
        VERSION_ONE_HIGH_WORD,
    )
    .expect("version one");
    register(device, mem, VIRTIO_MMIO_STATUS, STATUS_FEATURES_OK).expect("features ok");
}

pub fn configure_queue(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap, queue: u32) {
    register(device, mem, VIRTIO_MMIO_QUEUE_SEL, queue).expect("queue select");
    register(device, mem, VIRTIO_MMIO_QUEUE_NUM, u32::from(QUEUE_SIZE)).expect("queue size");
    write_address(
        device,
        mem,
        VIRTIO_MMIO_QUEUE_DESC_LOW,
        VIRTIO_MMIO_QUEUE_DESC_HIGH,
        DESCRIPTOR_ADDRESS,
    );
    write_address(
        device,
        mem,
        VIRTIO_MMIO_QUEUE_DRIVER_LOW,
        VIRTIO_MMIO_QUEUE_DRIVER_HIGH,
        AVAILABLE_ADDRESS,
    );
    write_address(
        device,
        mem,
        VIRTIO_MMIO_QUEUE_DEVICE_LOW,
        VIRTIO_MMIO_QUEUE_DEVICE_HIGH,
        USED_ADDRESS,
    );
    register(device, mem, VIRTIO_MMIO_QUEUE_READY, 1).expect("queue ready");
}

pub fn finish_driver(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap) {
    register(device, mem, VIRTIO_MMIO_STATUS, STATUS_DRIVER_OK).expect("driver ok");
}

pub fn publish_head(mem: &GuestMemoryMmap, head: u16, available_index: u16) {
    publish_head_at(mem, 0, head, available_index);
}

pub fn publish_head_at(mem: &GuestMemoryMmap, slot: u16, head: u16, available_index: u16) {
    let slot_offset = u64::from(slot) * AVAILABLE_RING_ELEMENT_BYTES;
    mem.write_obj(head, GuestAddress(AVAILABLE_RING_ADDRESS + slot_offset))
        .expect("available head");
    mem.write_obj(available_index, GuestAddress(AVAILABLE_INDEX_ADDRESS))
        .expect("available index");
}

pub fn write_descriptor(mem: &GuestMemoryMmap, index: u16, descriptor: VirtqDesc) {
    let address = DESCRIPTOR_ADDRESS + u64::from(index) * DESCRIPTOR_BYTES;
    mem.write_obj(descriptor.addr, GuestAddress(address))
        .expect("descriptor address");
    mem.write_obj(
        descriptor.len,
        GuestAddress(address + DESCRIPTOR_LENGTH_OFFSET),
    )
    .expect("descriptor length");
    mem.write_obj(
        descriptor.flags,
        GuestAddress(address + DESCRIPTOR_FLAGS_OFFSET),
    )
    .expect("descriptor flags");
    mem.write_obj(
        descriptor.next,
        GuestAddress(address + DESCRIPTOR_NEXT_OFFSET),
    )
    .expect("descriptor next");
}

pub fn notify(device: &mut VirtioMmioDevice, mem: &GuestMemoryMmap, queue: u32) -> bool {
    let effect = register(device, mem, VIRTIO_MMIO_QUEUE_NOTIFY, queue).expect("queue notify");
    let MmioWriteEffect::NotifyQueue(queue_index) = effect else {
        panic!("expected notify effect");
    };
    device.process_queue(queue_index, mem)
}

pub fn used_index(mem: &GuestMemoryMmap) -> u16 {
    mem.read_obj(GuestAddress(USED_INDEX_ADDRESS))
        .expect("used index")
}

fn write_address(
    device: &mut VirtioMmioDevice,
    mem: &GuestMemoryMmap,
    low_register: u64,
    high_register: u64,
    address: u64,
) {
    let bytes = address.to_le_bytes();
    let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let high = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    register(device, mem, low_register, low).expect("low address");
    register(device, mem, high_register, high).expect("high address");
}
