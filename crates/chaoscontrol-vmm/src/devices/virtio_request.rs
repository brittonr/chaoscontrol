//! Pure device-specific request-shape and transfer-budget planning.

use super::virtio_chain::{DescriptorBuffer, DescriptorChainPlan};
use super::virtio_types::{RequestViolation, VirtioLimits};

pub const BLOCK_SECTOR_BYTES: u64 = 512;
pub const BLOCK_HEADER_BYTES: u32 = 16;
pub const BLOCK_STATUS_BYTES: u32 = 1;
pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;
pub const NET_HEADER_BYTES: u64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRequestHeader {
    pub operation: u32,
    pub reserved: u32,
    pub sector: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockOperation {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRequestPlan {
    pub operation: BlockOperation,
    pub disk_offset: u64,
    pub transfer_bytes: u64,
    pub status_address: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetDirection {
    Receive,
    Transmit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetRequestPlan {
    pub direction: NetDirection,
    pub packet_bytes: u64,
    pub used_bytes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntropyRequestPlan {
    pub transfer_bytes: u64,
}

pub fn validated_block_status(chain: &DescriptorChainPlan) -> Option<u64> {
    let status = chain.buffers().last()?;
    if status.write && status.len >= BLOCK_STATUS_BYTES {
        Some(status.addr)
    } else {
        None
    }
}

pub fn plan_block_request(
    chain: &DescriptorChainPlan,
    header: BlockRequestHeader,
    disk_size: u64,
    limits: VirtioLimits,
) -> Result<BlockRequestPlan, RequestViolation> {
    let buffers = chain.buffers();
    if buffers.len() < 3 {
        return Err(RequestViolation::DescriptorShape);
    }
    let request_header = buffers.first().ok_or(RequestViolation::DescriptorShape)?;
    if request_header.write {
        return Err(RequestViolation::HeaderDirection);
    }
    if request_header.len < BLOCK_HEADER_BYTES {
        return Err(RequestViolation::HeaderLength {
            actual: request_header.len,
        });
    }
    let status = buffers.last().ok_or(RequestViolation::DescriptorShape)?;
    if !status.write {
        return Err(RequestViolation::StatusDirection);
    }
    if status.len < BLOCK_STATUS_BYTES {
        return Err(RequestViolation::StatusLength { actual: status.len });
    }
    if header.reserved != 0 {
        return Err(RequestViolation::HeaderReserved {
            value: header.reserved,
        });
    }
    let operation = match header.operation {
        VIRTIO_BLK_T_IN => BlockOperation::Read,
        VIRTIO_BLK_T_OUT => BlockOperation::Write,
        operation => return Err(RequestViolation::UnsupportedOperation { operation }),
    };
    let data = &buffers[1..buffers.len() - 1];
    let transfer_bytes = validate_block_data(data, operation, limits)?;
    let disk_offset = header
        .sector
        .checked_mul(BLOCK_SECTOR_BYTES)
        .ok_or(RequestViolation::StorageOverflow)?;
    let disk_end = disk_offset
        .checked_add(transfer_bytes)
        .ok_or(RequestViolation::StorageOverflow)?;
    if disk_end > disk_size {
        return Err(RequestViolation::StorageOutsideDevice {
            end: disk_end,
            device_size: disk_size,
        });
    }
    Ok(BlockRequestPlan {
        operation,
        disk_offset,
        transfer_bytes,
        status_address: status.addr,
    })
}

pub fn plan_net_request(
    chain: &DescriptorChainPlan,
    direction: NetDirection,
    packet_bytes: u64,
    limits: VirtioLimits,
) -> Result<NetRequestPlan, RequestViolation> {
    let buffers = chain.buffers();
    let first = buffers.first().ok_or(RequestViolation::DescriptorShape)?;
    let expect_write = direction == NetDirection::Receive;
    if buffers.iter().any(|buffer| buffer.write != expect_write) {
        return Err(RequestViolation::DataDirection);
    }
    if u64::from(first.len) < NET_HEADER_BYTES {
        return Err(RequestViolation::NetHeaderLength { actual: first.len });
    }
    let total = chain.aggregate_length();
    let required = NET_HEADER_BYTES
        .checked_add(packet_bytes)
        .ok_or(RequestViolation::StorageOverflow)?;
    match direction {
        NetDirection::Transmit => {
            let payload = total
                .checked_sub(NET_HEADER_BYTES)
                .ok_or(RequestViolation::DescriptorShape)?;
            if payload == 0 {
                return Err(RequestViolation::EmptyTransfer);
            }
            require_transfer_limit(payload, limits.max_net_frame_bytes)?;
            let used_bytes = u32::try_from(0u64).map_err(|_| RequestViolation::StorageOverflow)?;
            Ok(NetRequestPlan {
                direction,
                packet_bytes: payload,
                used_bytes,
            })
        }
        NetDirection::Receive => {
            if packet_bytes == 0 {
                return Err(RequestViolation::EmptyTransfer);
            }
            require_transfer_limit(packet_bytes, limits.max_net_frame_bytes)?;
            let maximum_capacity = limits
                .max_net_frame_bytes
                .checked_add(NET_HEADER_BYTES)
                .ok_or(RequestViolation::StorageOverflow)?;
            if total > maximum_capacity || total < required {
                return Err(RequestViolation::NetCapacity {
                    available: total,
                    required,
                });
            }
            let used_bytes =
                u32::try_from(required).map_err(|_| RequestViolation::StorageOverflow)?;
            Ok(NetRequestPlan {
                direction,
                packet_bytes,
                used_bytes,
            })
        }
    }
}

pub fn plan_entropy_request(
    chain: &DescriptorChainPlan,
    limits: VirtioLimits,
) -> Result<EntropyRequestPlan, RequestViolation> {
    let buffers = chain.buffers();
    if buffers.is_empty() {
        return Err(RequestViolation::DescriptorShape);
    }
    if buffers.iter().any(|buffer| !buffer.write) {
        return Err(RequestViolation::DataDirection);
    }
    let transfer_bytes = chain.aggregate_length();
    if transfer_bytes == 0 {
        return Err(RequestViolation::EmptyTransfer);
    }
    require_transfer_limit(transfer_bytes, limits.max_entropy_transfer_bytes)?;
    Ok(EntropyRequestPlan { transfer_bytes })
}

fn validate_block_data(
    buffers: &[DescriptorBuffer],
    operation: BlockOperation,
    limits: VirtioLimits,
) -> Result<u64, RequestViolation> {
    if buffers.is_empty() {
        return Err(RequestViolation::EmptyTransfer);
    }
    let expect_write = operation == BlockOperation::Read;
    if buffers.iter().any(|buffer| buffer.write != expect_write) {
        return Err(RequestViolation::DataDirection);
    }
    let transfer_bytes = buffers.iter().try_fold(0u64, |total, buffer| {
        total
            .checked_add(u64::from(buffer.len))
            .ok_or(RequestViolation::StorageOverflow)
    })?;
    if transfer_bytes == 0 {
        return Err(RequestViolation::EmptyTransfer);
    }
    if transfer_bytes % BLOCK_SECTOR_BYTES != 0 {
        return Err(RequestViolation::TransferAlignment {
            length: transfer_bytes,
            alignment: BLOCK_SECTOR_BYTES,
        });
    }
    require_transfer_limit(transfer_bytes, limits.max_block_transfer_bytes)?;
    Ok(transfer_bytes)
}

fn require_transfer_limit(length: u64, maximum: u64) -> Result<(), RequestViolation> {
    if length > maximum {
        return Err(RequestViolation::TransferLimit { length, maximum });
    }
    Ok(())
}
