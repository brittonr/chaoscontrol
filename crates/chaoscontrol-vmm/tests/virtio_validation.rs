use chaoscontrol_vmm::devices::virtio_chain::{
    plan_descriptor_chain, VirtqDesc, VIRTQ_DESC_F_INDIRECT, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE,
};
use chaoscontrol_vmm::devices::virtio_request::{
    plan_block_request, plan_entropy_request, plan_net_request, BlockDeviceProfile, BlockOperation,
    BlockRequestHeader, NetDirection, BLOCK_HEADER_BYTES, BLOCK_SECTOR_BYTES, VIRTIO_BLK_T_FLUSH,
    VIRTIO_BLK_T_OUT,
};
use chaoscontrol_vmm::devices::virtio_types::{
    DescriptorViolation, QueueViolation, RequestViolation, TransportViolation, VirtioLimits,
};
use chaoscontrol_vmm::devices::virtio_validation::{
    validate_available_delta, validate_queue_config, validate_restored_status, MemoryRegion,
    RawQueueConfig, VIRTIO_F_VERSION_1, VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER,
    VIRTIO_STATUS_DRIVER_OK, VIRTIO_STATUS_FEATURES_OK,
};

const MEMORY_BYTES: u64 = 1024 * 1024;
const QUEUE_SIZE: u16 = 8;
const DESC_ADDRESS: u64 = 0x1000;
const AVAIL_ADDRESS: u64 = 0x2000;
const USED_ADDRESS: u64 = 0x3000;
const DATA_ADDRESS: u64 = 0x4000;
const STATUS_ADDRESS: u64 = 0x5000;
const HEADER_ADDRESS: u64 = 0x6000;
const BLOCK_BYTES: u64 = 64 * 1024;
const DATA_BYTES: u32 = BLOCK_SECTOR_BYTES as u32;

fn memory() -> [MemoryRegion; 1] {
    [MemoryRegion {
        start: 0,
        length: MEMORY_BYTES,
    }]
}

fn raw_queue() -> RawQueueConfig {
    RawQueueConfig {
        size: u32::from(QUEUE_SIZE),
        descriptor_address: DESC_ADDRESS,
        driver_address: AVAIL_ADDRESS,
        device_address: USED_ADDRESS,
    }
}

#[test]
fn compliant_queue_and_wrapped_progress_validate() {
    let config = validate_queue_config(raw_queue(), QUEUE_SIZE, &memory(), VirtioLimits::default())
        .expect("valid queue");
    assert_eq!(config.size(), QUEUE_SIZE);
    assert_eq!(validate_available_delta(u16::MAX, 0, QUEUE_SIZE), Ok(1));
}

#[test]
fn restored_status_accepts_complete_state_and_rejects_missing_prerequisites() {
    let active_status = VIRTIO_STATUS_ACKNOWLEDGE
        | VIRTIO_STATUS_DRIVER
        | VIRTIO_STATUS_FEATURES_OK
        | VIRTIO_STATUS_DRIVER_OK;
    assert_eq!(
        validate_restored_status(active_status, VIRTIO_F_VERSION_1, VIRTIO_F_VERSION_1),
        Ok(())
    );
    assert!(matches!(
        validate_restored_status(
            VIRTIO_STATUS_DRIVER_OK,
            VIRTIO_F_VERSION_1,
            VIRTIO_F_VERSION_1
        ),
        Err(TransportViolation::StatusTransition { .. })
    ));
}

#[test]
fn invalid_queue_sizes_are_typed() {
    let mut raw = raw_queue();
    raw.size = 0;
    assert_eq!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::ZeroSize)
    );
    raw.size = u32::from(u16::MAX) + 1;
    assert!(matches!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::SizeWidth { .. })
    ));
    raw.size = 3;
    assert!(matches!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::SizeNotPowerOfTwo { .. })
    ));
    raw.size = u32::from(QUEUE_SIZE) * 2;
    assert!(matches!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::SizeAboveMaximum { .. })
    ));
}

#[test]
fn invalid_queue_geometry_is_typed() {
    let mut raw = raw_queue();
    raw.descriptor_address += 1;
    assert!(matches!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::AddressMisaligned { .. })
    ));
    raw = raw_queue();
    raw.driver_address = raw.descriptor_address;
    assert_eq!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::RingOverlap)
    );
    raw = raw_queue();
    raw.device_address = MEMORY_BYTES;
    assert!(matches!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::RingOutsideMemory { .. })
    ));
    raw = raw_queue();
    raw.descriptor_address = u64::MAX - 7;
    assert!(matches!(
        validate_queue_config(raw, QUEUE_SIZE, &memory(), VirtioLimits::default()),
        Err(QueueViolation::AddressMisaligned { .. } | QueueViolation::AddressOverflow)
    ));
}

#[test]
fn excessive_available_delta_is_rejected() {
    let excessive = QUEUE_SIZE + 1;
    assert_eq!(
        validate_available_delta(0, excessive, QUEUE_SIZE),
        Err(QueueViolation::AvailableDelta {
            delta: excessive,
            capacity: QUEUE_SIZE,
        })
    );
}

fn valid_descriptors() -> [VirtqDesc; 3] {
    [
        VirtqDesc {
            addr: HEADER_ADDRESS,
            len: BLOCK_HEADER_BYTES,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        },
        VirtqDesc {
            addr: DATA_ADDRESS,
            len: DATA_BYTES,
            flags: VIRTQ_DESC_F_NEXT,
            next: 2,
        },
        VirtqDesc {
            addr: STATUS_ADDRESS,
            len: 1,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    ]
}

#[test]
fn complete_block_chain_and_request_validate() {
    let chain = plan_descriptor_chain(
        &valid_descriptors(),
        0,
        3,
        &memory(),
        VirtioLimits::default(),
    )
    .expect("valid chain");
    let plan = plan_block_request(
        &chain,
        BlockRequestHeader {
            operation: VIRTIO_BLK_T_OUT,
            reserved: 0,
            sector: 0,
        },
        BlockDeviceProfile {
            disk_size: BLOCK_BYTES,
            flush_supported: true,
        },
        VirtioLimits::default(),
    )
    .expect("valid block request");
    assert_eq!(plan.operation, BlockOperation::Write);
    assert_eq!(plan.transfer_bytes, BLOCK_SECTOR_BYTES);
}

#[test]
fn cycles_bad_indices_and_flags_are_typed() {
    let mut descriptors = valid_descriptors();
    descriptors[1].next = 0;
    assert!(matches!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::Cycle { .. })
    ));
    descriptors = valid_descriptors();
    descriptors[1].next = 3;
    assert!(matches!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::NextIndex { .. })
    ));
    descriptors = valid_descriptors();
    descriptors[1].flags |= VIRTQ_DESC_F_INDIRECT;
    assert!(matches!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::UnsupportedFlags { .. })
    ));
}

#[test]
fn descriptor_head_count_and_unknown_non_indirect_flags_are_bounded() {
    let mut descriptors = valid_descriptors();
    assert_eq!(
        plan_descriptor_chain(&descriptors, 3, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::HeadIndex {
            index: 3,
            capacity: 3
        }),
    );
    let limits = VirtioLimits {
        max_chain_descriptors: 2,
        ..VirtioLimits::default()
    };
    assert_eq!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), limits),
        Err(DescriptorViolation::CountLimit {
            count: 3,
            maximum: 2
        }),
    );
    const UNKNOWN_FLAG: u16 = 1 << 15;
    descriptors[1].flags |= UNKNOWN_FLAG;
    assert_eq!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::UnsupportedFlags {
            flags: descriptors[1].flags
        }),
    );
}

#[test]
fn descriptor_ranges_and_aggregate_budget_are_typed() {
    let mut descriptors = valid_descriptors();
    descriptors[1].addr = u64::MAX;
    assert!(matches!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::AddressOverflow { .. })
    ));
    descriptors = valid_descriptors();
    descriptors[1].addr = MEMORY_BYTES;
    assert!(matches!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default()),
        Err(DescriptorViolation::OutsideMemory { .. })
    ));
    descriptors = valid_descriptors();
    let limits = VirtioLimits {
        max_aggregate_bytes: u64::from(BLOCK_HEADER_BYTES + DATA_BYTES),
        ..VirtioLimits::default()
    };
    assert!(matches!(
        plan_descriptor_chain(&descriptors, 0, 3, &memory(), limits),
        Err(DescriptorViolation::AggregateLimit { .. })
    ));
}

#[test]
fn wrong_directions_shapes_and_lengths_are_typed() {
    let mut descriptors = valid_descriptors();
    descriptors[1].flags |= VIRTQ_DESC_F_WRITE;
    let chain = plan_descriptor_chain(&descriptors, 0, 3, &memory(), VirtioLimits::default())
        .expect("generic chain");
    assert_eq!(
        plan_block_request(
            &chain,
            BlockRequestHeader {
                operation: VIRTIO_BLK_T_OUT,
                reserved: 0,
                sector: 0,
            },
            BlockDeviceProfile {
                disk_size: BLOCK_BYTES,
                flush_supported: true,
            },
            VirtioLimits::default(),
        ),
        Err(RequestViolation::DataDirection)
    );
    assert_eq!(
        plan_entropy_request(&chain, VirtioLimits::default()),
        Err(RequestViolation::DataDirection)
    );
    assert!(matches!(
        plan_net_request(&chain, NetDirection::Transmit, 0, VirtioLimits::default()),
        Err(RequestViolation::DataDirection | RequestViolation::NetHeaderLength { .. })
    ));
}

fn flush_descriptors() -> [VirtqDesc; 2] {
    [
        VirtqDesc {
            addr: HEADER_ADDRESS,
            len: BLOCK_HEADER_BYTES,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        },
        VirtqDesc {
            addr: STATUS_ADDRESS,
            len: 1,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        },
    ]
}

#[test]
fn flush_requires_the_device_feature_and_carries_no_data_descriptor() {
    let flush = flush_descriptors();
    let chain = plan_descriptor_chain(&flush, 0, 2, &memory(), VirtioLimits::default())
        .expect("flush chain");
    let header = BlockRequestHeader {
        operation: VIRTIO_BLK_T_FLUSH,
        reserved: 0,
        sector: 0,
    };
    let plan = plan_block_request(
        &chain,
        header,
        BlockDeviceProfile {
            disk_size: BLOCK_BYTES,
            flush_supported: true,
        },
        VirtioLimits::default(),
    )
    .expect("admitted flush");
    assert_eq!(plan.operation, BlockOperation::Flush);
    assert_eq!(plan.transfer_bytes, 0);
    assert_eq!(plan.disk_offset, 0);
    assert_eq!(plan.status_address, STATUS_ADDRESS);

    assert_eq!(
        plan_block_request(
            &chain,
            header,
            BlockDeviceProfile {
                disk_size: BLOCK_BYTES,
                flush_supported: false,
            },
            VirtioLimits::default(),
        ),
        Err(RequestViolation::FlushUnsupported)
    );

    let with_data = valid_descriptors();
    let chain = plan_descriptor_chain(&with_data, 0, 3, &memory(), VirtioLimits::default())
        .expect("generic chain");
    assert_eq!(
        plan_block_request(
            &chain,
            header,
            BlockDeviceProfile {
                disk_size: BLOCK_BYTES,
                flush_supported: true,
            },
            VirtioLimits::default(),
        ),
        Err(RequestViolation::DescriptorShape)
    );
}

#[test]
fn flush_status_descriptor_must_be_a_writable_byte() {
    let header = BlockRequestHeader {
        operation: VIRTIO_BLK_T_FLUSH,
        reserved: 0,
        sector: 0,
    };
    let profile = BlockDeviceProfile {
        disk_size: BLOCK_BYTES,
        flush_supported: true,
    };
    let mut read_only_status = flush_descriptors();
    read_only_status[1].flags = 0;
    let chain = plan_descriptor_chain(&read_only_status, 0, 2, &memory(), VirtioLimits::default())
        .expect("generic chain");
    assert_eq!(
        plan_block_request(&chain, header, profile, VirtioLimits::default()),
        Err(RequestViolation::StatusDirection)
    );

    let mut short_status = flush_descriptors();
    short_status[1].len = 0;
    let chain = plan_descriptor_chain(&short_status, 0, 2, &memory(), VirtioLimits::default())
        .expect("generic chain");
    assert_eq!(
        plan_block_request(&chain, header, profile, VirtioLimits::default()),
        Err(RequestViolation::StatusLength { actual: 0 })
    );
}
