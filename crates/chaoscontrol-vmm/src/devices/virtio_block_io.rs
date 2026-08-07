//! Bounded guest-memory and disk transfer helpers for virtio-blk.

use super::block::DeterministicBlock;
use super::virtio_chain::{DescriptorBuffer, DescriptorChainPlan};
use super::virtio_request::{BlockRequestHeader, BLOCK_HEADER_BYTES};
use super::virtio_types::{RequestViolation, VirtioFailure};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const HEADER_RESERVED_OFFSET: u64 = 4;
const HEADER_SECTOR_OFFSET: u64 = 8;

pub(super) fn read_header(
    mem: &GuestMemoryMmap,
    chain: &DescriptorChainPlan,
) -> Result<BlockRequestHeader, VirtioFailure> {
    let header = chain
        .buffers()
        .first()
        .ok_or(VirtioFailure::Request(RequestViolation::DescriptorShape))?;
    if header.write {
        return Err(VirtioFailure::Request(RequestViolation::HeaderDirection));
    }
    if header.len < BLOCK_HEADER_BYTES {
        return Err(VirtioFailure::Request(RequestViolation::HeaderLength {
            actual: header.len,
        }));
    }
    Ok(BlockRequestHeader {
        operation: read_field(mem, header.addr)?,
        reserved: read_field_at(mem, header.addr, HEADER_RESERVED_OFFSET)?,
        sector: read_field_at(mem, header.addr, HEADER_SECTOR_OFFSET)?,
    })
}

pub(super) fn block_data_buffers(
    chain: &DescriptorChainPlan,
) -> Result<&[DescriptorBuffer], VirtioFailure> {
    let buffers = chain.buffers();
    buffers
        .get(1..buffers.len().saturating_sub(1))
        .ok_or(VirtioFailure::Request(RequestViolation::DescriptorShape))
}

pub(super) fn preflight_guest_reads(
    mem: &GuestMemoryMmap,
    buffers: &[DescriptorBuffer],
    scratch: &mut [u8],
) -> Result<(), VirtioFailure> {
    for buffer in buffers {
        visit_chunks(buffer, scratch, |address, chunk| {
            mem.read_slice(chunk, GuestAddress(address))
                .map_err(|_| VirtioFailure::GuestMemoryRead)
        })?;
    }
    Ok(())
}

pub(super) fn transfer_guest_to_disk(
    mem: &GuestMemoryMmap,
    disk: &mut DeterministicBlock,
    buffers: &[DescriptorBuffer],
    mut disk_offset: u64,
    scratch: &mut [u8],
) -> Result<(), VirtioFailure> {
    for buffer in buffers {
        visit_chunks(buffer, scratch, |address, chunk| {
            mem.read_slice(chunk, GuestAddress(address))
                .map_err(|_| VirtioFailure::GuestMemoryRead)?;
            disk.write(disk_offset, chunk)
                .map_err(|_| VirtioFailure::BackendWrite)?;
            disk_offset = checked_advance(disk_offset, chunk.len(), VirtioFailure::BackendWrite)?;
            Ok(())
        })?;
    }
    Ok(())
}

pub(super) fn transfer_disk_to_guest(
    mem: &GuestMemoryMmap,
    disk: &mut DeterministicBlock,
    buffers: &[DescriptorBuffer],
    mut disk_offset: u64,
    scratch: &mut [u8],
) -> Result<(), VirtioFailure> {
    for buffer in buffers {
        visit_chunks(buffer, scratch, |address, chunk| {
            disk.read(disk_offset, chunk)
                .map_err(|_| VirtioFailure::BackendRead)?;
            mem.write_slice(chunk, GuestAddress(address))
                .map_err(|_| VirtioFailure::GuestMemoryWrite)?;
            disk_offset = checked_advance(disk_offset, chunk.len(), VirtioFailure::BackendRead)?;
            Ok(())
        })?;
    }
    Ok(())
}

fn visit_chunks(
    buffer: &DescriptorBuffer,
    scratch: &mut [u8],
    mut visit: impl FnMut(u64, &mut [u8]) -> Result<(), VirtioFailure>,
) -> Result<(), VirtioFailure> {
    let mut remaining = usize::try_from(buffer.len).map_err(|_| VirtioFailure::GuestMemoryRead)?;
    let mut address = buffer.addr;
    while remaining > 0 {
        let chunk_length = remaining.min(scratch.len());
        visit(address, &mut scratch[..chunk_length])?;
        address = checked_advance(address, chunk_length, VirtioFailure::GuestMemoryRead)?;
        remaining -= chunk_length;
    }
    Ok(())
}

fn checked_advance(
    address: u64,
    length: usize,
    failure: VirtioFailure,
) -> Result<u64, VirtioFailure> {
    let length = u64::try_from(length).map_err(|_| failure.clone())?;
    address.checked_add(length).ok_or(failure)
}

fn read_field<T: vm_memory::ByteValued>(
    mem: &GuestMemoryMmap,
    address: u64,
) -> Result<T, VirtioFailure> {
    mem.read_obj(GuestAddress(address))
        .map_err(|_| VirtioFailure::GuestMemoryRead)
}

fn read_field_at<T: vm_memory::ByteValued>(
    mem: &GuestMemoryMmap,
    address: u64,
    offset: u64,
) -> Result<T, VirtioFailure> {
    let field_address = address
        .checked_add(offset)
        .ok_or(VirtioFailure::GuestMemoryRead)?;
    read_field(mem, field_address)
}
