//! Bounded fallible host-buffer allocation for virtio imperative shells.

use super::virtio_types::ResourceViolation;

pub trait BoundedBufferAllocator: Send {
    fn zeroed(&mut self, requested: usize, maximum: usize) -> Result<Vec<u8>, ResourceViolation>;
}

#[derive(Default)]
pub struct HostBufferAllocator;

impl BoundedBufferAllocator for HostBufferAllocator {
    fn zeroed(&mut self, requested: usize, maximum: usize) -> Result<Vec<u8>, ResourceViolation> {
        if requested > maximum {
            return Err(ResourceViolation::ScratchLimit { requested, maximum });
        }
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(requested)
            .map_err(|_| ResourceViolation::Allocation { requested })?;
        buffer.resize(requested, 0);
        Ok(buffer)
    }
}

pub struct RejectingBufferAllocator;

impl BoundedBufferAllocator for RejectingBufferAllocator {
    fn zeroed(&mut self, requested: usize, maximum: usize) -> Result<Vec<u8>, ResourceViolation> {
        if requested > maximum {
            return Err(ResourceViolation::ScratchLimit { requested, maximum });
        }
        Err(ResourceViolation::Allocation { requested })
    }
}
