//! Wire protocol for ChaosControl SDK ↔ VMM hypercall communication.
//!
//! This crate defines the shared memory layout, command IDs, and payload
//! encoding used between guest-side SDK and host-side VMM.  It is
//! `no_std`-compatible with zero dependencies.
//!
//! # Transport
//!
//! Communication uses a **shared memory page** at a fixed guest-physical
//! address plus an I/O port trigger:
//!
//! 1. Guest writes a [`HypercallPage`] to [`HYPERCALL_PAGE_ADDR`]
//! 2. Guest does `outb(SDK_PORT, 0)` to trigger processing
//! 3. Host reads the page from guest memory, dispatches the command
//! 4. Host writes result fields back to the page
//! 5. Guest reads result and continues
//!
//! The hypercall page sits in the E820 reserved gap (`0x9FC00..0x100000`)
//! so the Linux kernel will never allocate it, but it is backed by the
//! KVM memory region and identity-mapped by the guest page tables.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod admission;
#[cfg(feature = "std")]
pub mod branch_marker;
#[cfg(feature = "std")]
mod canonical;
#[cfg(feature = "std")]
pub mod fallback;
#[cfg(feature = "std")]
pub mod guest_process;
#[cfg(feature = "std")]
pub mod identity;
mod memory;
mod message;
#[cfg(feature = "std")]
pub mod oci_intake;
#[cfg(feature = "std")]
pub mod process;
#[cfg(feature = "std")]
pub mod transport;

pub use memory::HypercallPage;
pub use message::encode_payload;
#[cfg(feature = "std")]
pub use message::{decode_payload, DecodedPayload};

// ═══════════════════════════════════════════════════════════════════════
//  Addresses and ports
// ═══════════════════════════════════════════════════════════════════════

/// Guest-physical address of the SDK hypercall page (4 KB).
///
/// Located in the E820 reserved gap between low memory end (`0x9FC00`)
/// and HIMEM_START (`0x100000`).  The kernel sees this as reserved BIOS
/// memory and will not use it.
pub const HYPERCALL_PAGE_ADDR: u64 = 0x000F_E000;

/// I/O port used to trigger a hypercall (legacy transport).
///
/// Guest writes `outb(SDK_PORT, 0)` after filling the hypercall page.
/// Superseded by VMCALL transport when available, but kept for fallback.
pub const SDK_PORT: u16 = 0x0510;

/// Transport mode: use `vmcall` instruction (fast path).
pub const TRANSPORT_VMCALL: u8 = 1;

/// Transport mode: use `outb(SDK_PORT, 0)` port I/O (fallback).
pub const TRANSPORT_PORT_IO: u8 = 2;

/// Byte offset within the hypercall page where the VMM writes
/// the transport mode indicator (`TRANSPORT_VMCALL` or
/// `TRANSPORT_PORT_IO`).  This lives in the `_reserved2` area
/// (offset 0x19) so it doesn't change the struct layout.
///
/// The VMM writes this byte to guest memory before the first vCPU
/// run.  The SDK reads it after mmapping `/dev/mem` to decide
/// whether to use `vmcall` or `outb`.
pub const TRANSPORT_MODE_OFFSET: u64 = 0x19;

/// KVM hypercall number for the ChaosControl SDK (VMCALL transport).
///
/// When the guest executes `vmcall` with `RAX = VMCALL_NR`, KVM exits
/// to the VMM via `KVM_EXIT_HYPERCALL`.  This is faster than port I/O
/// (no I/O emulation path) and semantically cleaner — VMCALL is the
/// canonical x86 mechanism for guest-to-hypervisor communication.
///
/// Number 48 is chosen to avoid collision with KVM's built-in hypercalls
/// (numbers 1–12) while fitting in the 64-bit bitmask used by
/// `KVM_CAP_EXIT_HYPERCALL`.
pub const VMCALL_NR: u64 = 48;

/// Size of the hypercall page in bytes.
pub const HYPERCALL_PAGE_SIZE: usize = 4096;

/// Offset of the payload area within the hypercall page.
pub const PAYLOAD_OFFSET: usize = 32;

/// Maximum payload size in bytes.
pub const PAYLOAD_MAX: usize = HYPERCALL_PAGE_SIZE - PAYLOAD_OFFSET;

// ═══════════════════════════════════════════════════════════════════════
//  Coverage bitmap
// ═══════════════════════════════════════════════════════════════════════

/// Guest-physical address of the coverage bitmap (64 KB).
///
/// Located in the BIOS reserved area (0xE0000–0xEFFFF) within the E820
/// gap between low memory end and HIMEM_START.  The kernel sees this as
/// reserved BIOS memory and will not allocate it, but it is backed by
/// the KVM memory region and identity-mapped by the guest page tables.
///
/// The bitmap follows the AFL convention: 64 KB of 8-bit saturating
/// counters indexed by `(prev_location XOR cur_location) % MAP_SIZE`.
pub const COVERAGE_BITMAP_ADDR: u64 = 0x000E_0000;

/// Size of the coverage bitmap in bytes (64 KB, same as AFL).
pub const COVERAGE_BITMAP_SIZE: usize = 65536;

/// I/O port used to signal coverage initialization.
///
/// Guest writes `outb(COVERAGE_PORT, 0)` after mapping the bitmap.
/// This tells the VMM that coverage collection is active.
pub const COVERAGE_PORT: u16 = 0x0511;

// ═══════════════════════════════════════════════════════════════════════
//  Assertion kind discriminants
// ═══════════════════════════════════════════════════════════════════════

pub const ASSERTION_KIND_ALWAYS_DISCRIMINANT: u8 = 0;
pub const ASSERTION_KIND_SOMETIMES_DISCRIMINANT: u8 = 1;
pub const ASSERTION_KIND_REACHABLE_DISCRIMINANT: u8 = 2;
pub const ASSERTION_KIND_UNREACHABLE_DISCRIMINANT: u8 = 3;

// ═══════════════════════════════════════════════════════════════════════
//  Command IDs
// ═══════════════════════════════════════════════════════════════════════

/// Assertion: condition must be true every time this point is reached.
pub const CMD_ASSERT_ALWAYS: u8 = 0x01;

/// Assertion: condition must be true at least once across all runs.
pub const CMD_ASSERT_SOMETIMES: u8 = 0x02;

/// Assertion: this point must be reached at least once across all runs.
pub const CMD_ASSERT_REACHABLE: u8 = 0x03;

/// Assertion: this point must never be reached in any run.
pub const CMD_ASSERT_UNREACHABLE: u8 = 0x04;

/// Begin a versioned bounded assertion catalog.
pub const CMD_ASSERT_CATALOG_BEGIN: u8 = 0x06;

/// Add one canonical descriptor to the pending assertion catalog.
pub const CMD_ASSERT_CATALOG_DESCRIPTOR: u8 = 0x08;

/// Complete and activate the pending assertion catalog.
pub const CMD_ASSERT_CATALOG_COMPLETE: u8 = 0x09;

/// Lifecycle: workload setup is complete, testing begins.
pub const CMD_LIFECYCLE_SETUP_COMPLETE: u8 = 0x10;

/// Lifecycle: emit a named structured event.
pub const CMD_LIFECYCLE_SEND_EVENT: u8 = 0x11;

/// Random: request a guided random u64 from the VMM.
pub const CMD_RANDOM_GET: u8 = 0x20;

/// Random: request a guided choice from `n` options (0..n-1).
pub const CMD_RANDOM_CHOICE: u8 = 0x21;

/// Coverage: signal that guest has initialized coverage instrumentation.
pub const CMD_COVERAGE_INIT: u8 = 0x30;

/// Resource observation: return the current guest-visible memory ceiling.
pub const CMD_RESOURCE_MEMORY_CEILING: u8 = 0x40;

/// Guest supervisor: poll one host-directed process fault command.
pub const CMD_PROCESS_FAULT_POLL: u8 = 0x50;

// ═══════════════════════════════════════════════════════════════════════
//  Status codes
// ═══════════════════════════════════════════════════════════════════════

/// Hypercall completed successfully.
pub const STATUS_OK: u8 = 0x00;

/// Hypercall failed (unknown command, bad payload, etc.).
pub const STATUS_ERROR: u8 = 0x01;

/// An `assert_always` fired with condition=false — test fails.
pub const STATUS_ASSERTION_FAILED: u8 = 0x02;

/// An `assert_unreachable` was reached — test fails.
pub const STATUS_UNREACHABLE_REACHED: u8 = 0x03;

/// A catalog or descriptor conflict made assertion evidence ineligible.
pub const STATUS_ASSERTION_IDENTITY_CONFLICT: u8 = 0x04;

/// A runtime assertion event did not bind to the active catalog.
pub const STATUS_ASSERTION_EVENT_REJECTED: u8 = 0x05;

/// An assertion boundary exceeded a configured field or cardinality limit.
pub const STATUS_ASSERTION_LIMIT_EXCEEDED: u8 = 0x06;

#[cfg(test)]
mod tests;
