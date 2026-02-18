//! Deterministic device backends for the ChaosControl hypervisor.
//!
//! Each module provides a simulated device that replaces real hardware with a
//! controllable, snapshot-capable implementation suitable for deterministic
//! replay and fault injection.

pub mod block;
pub mod entropy;
pub mod net;
pub mod pit;
pub mod serial;
pub mod virtio_block;
pub mod virtio_entropy;
pub mod virtio_mmio;
pub mod virtio_net;
