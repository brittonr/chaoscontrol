//! Guest memory management for the ChaosControl deterministic hypervisor.
//!
//! This module centralises all guest physical memory layout constants,
//! page-table construction, GDT setup, and memory snapshot/restore into
//! a single [`GuestMemoryManager`] abstraction.  The memory layout
//! follows the standard Linux x86_64 boot protocol and is compatible
//! with Firecracker and crosvm conventions.
//!
//! # Guest Physical Memory Layout
//!
//! ```text
//! 0x0000_0000  ┌───────────────────────────┐
//!              │  Real-mode IVT             │
//! 0x0000_0500  ├───────────────────────────┤
//!              │  GDT (4 × 8-byte entries)  │
//! 0x0000_0520  ├───────────────────────────┤
//!              │  IDT (placeholder)         │
//! 0x0000_7000  ├───────────────────────────┤
//!              │  Zero page (boot_params)   │
//! 0x0000_8FF0  ├───────────────────────────┤
//!              │  Boot stack (grows down)   │
//! 0x0000_9000  ├───────────────────────────┤
//!              │  PML4 page table           │
//! 0x0000_A000  ├───────────────────────────┤
//!              │  PDPTE page table          │
//! 0x0000_B000  ├───────────────────────────┤
//!              │  PDE (512 × 2 MB pages)   │  → 1 GB identity map
//! 0x0002_0000  ├───────────────────────────┤
//!              │  Kernel command line       │
//! 0x0010_0000  ├───────────────────────────┤  ← HIMEM_START (1 MB)
//!              │  Kernel image + initrd     │
//!              │  ...                       │
//!              └───────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```no_run
//! use chaoscontrol_vmm::memory::{GuestMemoryManager, HIMEM_START};
//!
//! let mem = GuestMemoryManager::new(128 * 1024 * 1024).unwrap();
//! mem.setup_page_tables().unwrap();
//! mem.setup_gdt().unwrap();
//! mem.write_cmdline(b"console=ttyS0 nokaslr\0").unwrap();
//!
//! // Get the host address for KVM memory region setup.
//! let host_addr = mem.host_address();
//! ```

use kvm_bindings::kvm_segment;
use log::info;
use thiserror::Error;
use vm_memory::{Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

// ═══════════════════════════════════════════════════════════════════════
//  Memory layout constants
// ═══════════════════════════════════════════════════════════════════════

/// Start of high memory where the kernel is loaded (1 MB).
///
/// Everything below this address is reserved for real-mode data
/// structures, page tables, the GDT, and the boot stack.
pub const HIMEM_START: u64 = 0x0010_0000;

/// Initial stack pointer for the boot CPU.
///
/// Placed just below the page-table region in low memory.  The stack
/// grows downward from this address.
pub const BOOT_STACK_POINTER: u64 = 0x8ff0;

/// Address of the "zero page" (`boot_params` structure).
///
/// The Linux boot protocol requires the bootloader to place a
/// `boot_params` struct at a known address.  The kernel reads the
/// E820 memory map, command-line pointer, and initrd location from
/// this structure.
pub const ZERO_PAGE_START: u64 = 0x7000;

/// Address where the kernel command-line string is written.
///
/// The `cmd_line_ptr` field in `boot_params.hdr` must point here.
/// The string must be NUL-terminated.
pub const CMDLINE_START: u64 = 0x20000;

/// Maximum length of the kernel command line (bytes, including NUL).
pub const CMDLINE_MAX_SIZE: usize = 0x10000;

/// Offset of the boot GDT in guest physical memory.
///
/// Four 8-byte descriptors are written here: NULL, CODE64, DATA, TSS.
/// `sregs.gdt.base` must be set to this value.
pub const BOOT_GDT_OFFSET: u64 = 0x500;

/// Offset of the boot IDT in guest physical memory.
///
/// An empty IDT is written here during boot setup.
/// `sregs.idt.base` must be set to this value.
pub const BOOT_IDT_OFFSET: u64 = 0x520;

/// Physical address of the PML4 (Page Map Level 4) table.
///
/// This is the root of the 4-level x86_64 page-table hierarchy.
/// `CR3` must be loaded with this value to enable paging.
pub const PML4_START: u64 = 0x9000;

/// Physical address of the PDPTE (Page Directory Pointer Table) page.
///
/// The single PML4 entry at index 0 points here.
pub const PDPTE_START: u64 = 0xa000;

/// Physical address of the PDE (Page Directory Entry) table.
///
/// The single PDPTE entry at index 0 points here.  This table contains
/// 512 entries of 2 MB pages, identity-mapping the first 1 GB of
/// guest physical memory.
pub const PDE_START: u64 = 0xb000;

/// Number of 2 MB page-directory entries (covers 1 GB).
const PDE_ENTRY_COUNT: u64 = 512;

/// Page-table entry flags: present + writable.
const PTE_PRESENT_WRITABLE: u64 = 0x03;

/// Page-directory entry flags: present + writable + page-size (2 MB).
const PDE_PRESENT_WRITABLE_PS: u64 = 0x83;

// ═══════════════════════════════════════════════════════════════════════
//  GDT constants
// ═══════════════════════════════════════════════════════════════════════

/// Number of GDT entries written during boot setup.
///
/// Index 0 = NULL, 1 = CODE64, 2 = DATA, 3 = TSS.
pub const GDT_ENTRY_COUNT: usize = 4;

/// GDT table index for the NULL descriptor.
pub const GDT_INDEX_NULL: u8 = 0;

/// GDT table index for the 64-bit code segment.
pub const GDT_INDEX_CODE: u8 = 1;

/// GDT table index for the data segment.
pub const GDT_INDEX_DATA: u8 = 2;

/// GDT table index for the TSS.
pub const GDT_INDEX_TSS: u8 = 3;

/// Flags for the 64-bit code segment (ring 0, execute-read, long mode).
///
/// - Access byte `0x9B`: present, DPL=0, code/data (S=1), type=1011
///   (code, conforming=0, readable=1, accessed=1).
/// - Flags nibble `0xA`: granularity=1, D/B=0, L=1 (64-bit), AVL=0.
pub const GDT_FLAGS_CODE64: u16 = 0xa09b;

/// Flags for the data segment (ring 0, read-write).
///
/// - Access byte `0x93`: present, DPL=0, code/data (S=1), type=0011
///   (data, expand-up, writable, accessed).
/// - Flags nibble `0xC`: granularity=1, D/B=1 (32-bit operand size),
///   L=0, AVL=0.
pub const GDT_FLAGS_DATA: u16 = 0xc093;

/// Flags for the TSS segment (64-bit TSS, busy).
///
/// - Access byte `0x8B`: present, DPL=0, system (S=0), type=1011
///   (64-bit TSS, busy).
/// - Flags nibble `0x8`: granularity=1, D/B=0, L=0, AVL=0.
pub const GDT_FLAGS_TSS: u16 = 0x808b;

// ═══════════════════════════════════════════════════════════════════════
//  E820 memory map
// ═══════════════════════════════════════════════════════════════════════

/// E820 memory type: usable RAM available to the operating system.
pub const E820_RAM: u32 = 1;

/// E820 memory type: reserved by firmware (not available to the OS).
pub const E820_RESERVED: u32 = 2;

/// E820 memory type: ACPI reclaimable memory.
pub const E820_ACPI: u32 = 3;

/// E820 memory type: ACPI Non-Volatile Storage.
pub const E820_NVS: u32 = 4;

/// E820 memory type: defective / unusable region.
pub const E820_UNUSABLE: u32 = 5;

/// End of conventional low memory (below the EBDA).
///
/// Standard PCs reserve 0x9FC00–0x9FFFF for the Extended BIOS Data Area
/// and 0xA0000–0xFFFFF for legacy video memory and option ROMs.
/// Usable low RAM ends at this address.
pub const LOW_MEMORY_END: u64 = 0x9fc00;

pub use crate::verified::memory::E820Entry;

/// Build the standard E820 memory map for a guest with `memory_size` bytes.
///
/// Returns two entries:
///
/// 1. **Low memory** — from 0 to [`LOW_MEMORY_END`] (conventional RAM
///    below the EBDA).
/// 2. **High memory** — from [`HIMEM_START`] (1 MB) to the end of the
///    allocated guest RAM.
///
/// The gap between `LOW_MEMORY_END` (0x9FC00) and `HIMEM_START` (0x100000)
/// is implicitly reserved for the EBDA, legacy video memory, and option
/// ROMs, and is **not** reported as usable RAM.
///
/// # Panics
///
/// Panics if `memory_size` is less than or equal to [`HIMEM_START`],
/// because there would be no usable high memory.
pub fn build_e820_map(memory_size: u64) -> Vec<E820Entry> {
    crate::verified::memory::build_e820_map(memory_size)
}

// ═══════════════════════════════════════════════════════════════════════
//  Error type
// ═══════════════════════════════════════════════════════════════════════

/// Errors that can occur during guest memory operations.
#[derive(Error, Debug)]
pub enum MemoryError {
    /// The `vm-memory` crate failed to create the guest memory region.
    #[error("Failed to create guest memory region of {size} bytes")]
    Create {
        /// Requested allocation size.
        size: usize,
    },

    /// A write to guest physical memory failed.
    #[error("Failed to write to guest memory at {address:#x}")]
    Write {
        /// Guest physical address of the failed write.
        address: u64,
    },

    /// A read from guest physical memory failed.
    #[error("Failed to read from guest memory at {address:#x}")]
    Read {
        /// Guest physical address of the failed read.
        address: u64,
    },

    /// The kernel command line exceeds [`CMDLINE_MAX_SIZE`].
    #[error("Kernel command line too long: {len} bytes exceeds maximum of {CMDLINE_MAX_SIZE}")]
    CmdlineTooLong {
        /// Actual command-line length (including NUL terminator).
        len: usize,
    },

    /// Snapshot data length does not match the guest memory size.
    #[error("Snapshot size mismatch: expected {expected} bytes, got {actual}")]
    SnapshotSizeMismatch {
        /// Expected byte count (the guest memory size).
        expected: usize,
        /// Actual byte count of the provided data.
        actual: usize,
    },

    /// The host virtual address for the guest memory could not be resolved.
    #[error("Failed to resolve host virtual address for guest memory")]
    HostAddress,
}

// ═══════════════════════════════════════════════════════════════════════
//  GuestMemoryManager
// ═══════════════════════════════════════════════════════════════════════

/// Manages guest physical memory for a virtual machine.
///
/// `GuestMemoryManager` owns the [`GuestMemoryMmap`] allocation and
/// provides methods for:
///
/// - Writing the x86_64 boot-time data structures (page tables, GDT,
///   kernel command line).
/// - Taking full-memory snapshots and restoring from them.
/// - Obtaining the host virtual address for KVM
///   `kvm_userspace_memory_region` setup.
///
/// # Memory Allocation
///
/// Guest memory is allocated as a single contiguous `mmap` region
/// starting at guest physical address 0.  The region size is fixed at
/// construction time and cannot be changed.
///
/// # Interior Mutability
///
/// The underlying `mmap`-backed memory uses interior mutability — writes
/// go through a raw pointer into the mapped region.  This means methods
/// like [`setup_page_tables`](Self::setup_page_tables) and
/// [`restore`](Self::restore) take `&self` rather than `&mut self`,
/// matching the `vm-memory` crate's API.
///
/// # Example
///
/// ```no_run
/// use chaoscontrol_vmm::memory::GuestMemoryManager;
///
/// let mgr = GuestMemoryManager::new(256 * 1024 * 1024).unwrap();
///
/// // Set up boot-time data structures.
/// mgr.setup_page_tables().unwrap();
/// mgr.setup_gdt().unwrap();
/// mgr.write_cmdline(b"console=ttyS0 nokaslr\0").unwrap();
///
/// // Snapshot and restore.
/// let snapshot = mgr.dump();
/// mgr.restore(&snapshot).unwrap();
/// ```
pub struct GuestMemoryManager {
    /// The underlying mmap-backed guest memory.
    memory: GuestMemoryMmap,
    /// Total size of the guest memory in bytes.
    size: usize,
}

impl GuestMemoryManager {
    /// Create a new guest memory region of `size` bytes.
    ///
    /// The memory is allocated as a single contiguous region starting at
    /// guest physical address 0 and is zero-initialised by the kernel's
    /// `mmap` implementation.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Create`] if the underlying `mmap` allocation
    /// fails (e.g. insufficient host memory or address space).
    pub fn new(size: usize) -> Result<Self, MemoryError> {
        let regions = vec![(GuestAddress(0), size)];
        let memory =
            GuestMemoryMmap::from_ranges(&regions).map_err(|_| MemoryError::Create { size })?;

        info!(
            "Guest memory created: {} MB ({} bytes)",
            size / (1024 * 1024),
            size,
        );

        Ok(Self { memory, size })
    }

    /// Write identity-mapped page tables for the first 1 GB of guest
    /// physical memory.
    ///
    /// Sets up a three-level page-table hierarchy using 2 MB pages:
    ///
    /// ```text
    /// PML4[0]  → PDPTE       (at 0x9000)
    /// PDPTE[0] → PDE         (at 0xA000)
    /// PDE[0..511] → 2 MB pages (at 0xB000, covering 0x0000_0000–0x3FFF_FFFF)
    /// ```
    ///
    /// After this call, guest virtual addresses 0x0000_0000 through
    /// 0x3FFF_FFFF are identity-mapped (virtual address == physical
    /// address).  `CR3` must be set to [`PML4_START`] to activate these
    /// tables.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Write`] if any write to guest memory fails.
    pub fn setup_page_tables(&self) -> Result<(), MemoryError> {
        // PML4[0] → PDPTE (present + writable).
        self.memory
            .write_obj(PDPTE_START | PTE_PRESENT_WRITABLE, GuestAddress(PML4_START))
            .map_err(|_| MemoryError::Write {
                address: PML4_START,
            })?;

        // PDPTE[0] → PDE (present + writable).
        self.memory
            .write_obj(PDE_START | PTE_PRESENT_WRITABLE, GuestAddress(PDPTE_START))
            .map_err(|_| MemoryError::Write {
                address: PDPTE_START,
            })?;

        // PDE: 512 entries × 2 MB = 1 GB identity map.
        for i in 0..PDE_ENTRY_COUNT {
            let entry = (i << 21) | PDE_PRESENT_WRITABLE_PS;
            let addr = PDE_START + i * 8;
            self.memory
                .write_obj(entry, GuestAddress(addr))
                .map_err(|_| MemoryError::Write { address: addr })?;
        }

        info!(
            "Page tables written: PML4={:#x}, PDPTE={:#x}, PDE={:#x} \
             (512 × 2 MB = 1 GB identity map)",
            PML4_START, PDPTE_START, PDE_START,
        );

        Ok(())
    }

    /// Write the boot GDT and an empty IDT to guest memory.
    ///
    /// Four GDT entries are written at [`BOOT_GDT_OFFSET`]:
    ///
    /// | Index | Selector | Description                              |
    /// |-------|----------|------------------------------------------|
    /// | 0     | `0x00`   | NULL descriptor                          |
    /// | 1     | `0x08`   | 64-bit code segment (ring 0, exec-read)  |
    /// | 2     | `0x10`   | Data segment (ring 0, read-write)        |
    /// | 3     | `0x18`   | TSS segment                              |
    ///
    /// An empty IDT (8 zero bytes) is written at [`BOOT_IDT_OFFSET`].
    ///
    /// After calling this method, configure the vCPU special registers:
    /// - `sregs.gdt.base = BOOT_GDT_OFFSET`
    /// - `sregs.gdt.limit = 31` (4 entries × 8 bytes − 1)
    /// - `sregs.idt.base = BOOT_IDT_OFFSET`
    /// - `sregs.idt.limit = 7` (8 bytes − 1)
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Write`] if any write to guest memory fails.
    pub fn setup_gdt(&self) -> Result<(), MemoryError> {
        let gdt_table: [u64; GDT_ENTRY_COUNT] = [
            0,                                       // NULL descriptor
            gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff), // CODE64 segment
            gdt_entry(GDT_FLAGS_DATA, 0, 0xfffff),   // DATA segment
            gdt_entry(GDT_FLAGS_TSS, 0, 0xfffff),    // TSS segment
        ];

        for (i, entry) in gdt_table.iter().enumerate() {
            let addr = BOOT_GDT_OFFSET + (i as u64 * 8);
            self.memory
                .write_obj(*entry, GuestAddress(addr))
                .map_err(|_| MemoryError::Write { address: addr })?;
        }

        // Write an empty IDT placeholder.
        self.memory
            .write_obj(0u64, GuestAddress(BOOT_IDT_OFFSET))
            .map_err(|_| MemoryError::Write {
                address: BOOT_IDT_OFFSET,
            })?;

        info!(
            "GDT written at {:#x} ({} entries), IDT at {:#x}",
            BOOT_GDT_OFFSET, GDT_ENTRY_COUNT, BOOT_IDT_OFFSET,
        );

        Ok(())
    }

    /// Write the kernel command line to guest memory.
    ///
    /// The bytes are written starting at [`CMDLINE_START`].  If the last
    /// byte is not NUL (`0x00`), a NUL terminator is appended
    /// automatically.
    ///
    /// # Errors
    ///
    /// - [`MemoryError::CmdlineTooLong`] if the command line (including
    ///   any auto-appended NUL terminator) exceeds [`CMDLINE_MAX_SIZE`].
    /// - [`MemoryError::Write`] if the write to guest memory fails.
    pub fn write_cmdline(&self, cmdline: &[u8]) -> Result<(), MemoryError> {
        let needs_nul = cmdline.last() != Some(&0);
        let total_len = cmdline.len() + if needs_nul { 1 } else { 0 };

        if total_len > CMDLINE_MAX_SIZE {
            return Err(MemoryError::CmdlineTooLong { len: total_len });
        }

        self.memory
            .write_slice(cmdline, GuestAddress(CMDLINE_START))
            .map_err(|_| MemoryError::Write {
                address: CMDLINE_START,
            })?;

        // Append NUL terminator if the caller didn't include one.
        if needs_nul {
            let nul_addr = CMDLINE_START + cmdline.len() as u64;
            self.memory
                .write_obj(0u8, GuestAddress(nul_addr))
                .map_err(|_| MemoryError::Write { address: nul_addr })?;
        }

        info!(
            "Command line written at {:#x} ({} bytes, NUL-terminated)",
            CMDLINE_START, total_len,
        );

        Ok(())
    }

    /// Dump all guest memory as a byte vector.
    ///
    /// Returns a complete copy of the guest physical memory, suitable for
    /// snapshot serialisation.  The returned vector has exactly
    /// [`size()`](Self::size) bytes.
    ///
    /// # Panics
    ///
    /// Panics if the read from guest memory fails, which indicates a bug
    /// (the memory region was allocated by this manager and should always
    /// be readable in its entirety).
    pub fn dump(&self) -> Vec<u8> {
        let mut data = vec![0u8; self.size];
        self.memory
            .read_slice(&mut data, GuestAddress(0))
            .expect("failed to read guest memory during dump — this is a bug");
        data
    }

    /// Restore guest memory from a previously-captured snapshot.
    ///
    /// Overwrites the entire guest physical address space with the
    /// contents of `data`.  The slice must be exactly
    /// [`size()`](Self::size) bytes long.
    ///
    /// # Errors
    ///
    /// - [`MemoryError::SnapshotSizeMismatch`] if `data.len()` does not
    ///   equal the guest memory size.
    /// - [`MemoryError::Write`] if the write to guest memory fails.
    pub fn restore(&self, data: &[u8]) -> Result<(), MemoryError> {
        if data.len() != self.size {
            return Err(MemoryError::SnapshotSizeMismatch {
                expected: self.size,
                actual: data.len(),
            });
        }

        self.memory
            .write_slice(data, GuestAddress(0))
            .map_err(|_| MemoryError::Write { address: 0 })?;

        info!("Guest memory restored: {} bytes", self.size);

        Ok(())
    }

    /// Get a reference to the underlying [`GuestMemoryMmap`].
    ///
    /// Use this when passing memory to `linux-loader`, the snapshot
    /// module, or other components that work directly with the
    /// `vm-memory` types.
    #[inline]
    pub fn inner(&self) -> &GuestMemoryMmap {
        &self.memory
    }

    /// Get the host virtual address of the start of guest memory.
    ///
    /// This is the `userspace_addr` value needed when constructing a
    /// [`kvm_userspace_memory_region`](kvm_bindings::kvm_userspace_memory_region)
    /// to register the guest memory with KVM.
    ///
    /// # Panics
    ///
    /// Panics if the address cannot be resolved, which should never
    /// happen on a validly-constructed `GuestMemoryManager`.
    pub fn host_address(&self) -> u64 {
        self.memory
            .get_host_address(GuestAddress(0))
            .expect("failed to resolve host address for guest memory — this is a bug")
            as u64
    }

    /// Total size of the guest memory in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  GDT helper functions
// ═══════════════════════════════════════════════════════════════════════

/// Construct a raw 8-byte GDT descriptor from flags, base, and limit.
///
/// The `flags` parameter packs both the access byte and the flags nibble
/// of the x86 segment descriptor:
///
/// ```text
/// flags[7:0]   → descriptor byte 5  (access: P, DPL, S, Type)
/// flags[15:12] → descriptor byte 6  high nibble (G, D/B, L, AVL)
/// ```
///
/// # Standard Boot Entries
///
/// | Segment | Flags    | Base | Limit     |
/// |---------|----------|------|-----------|
/// | CODE64  | `0xa09b` | 0    | `0xfffff` |
/// | DATA    | `0xc093` | 0    | `0xfffff` |
/// | TSS     | `0x808b` | 0    | `0xfffff` |
///
/// # Example
///
/// ```
/// use chaoscontrol_vmm::memory::gdt_entry;
///
/// let code64 = gdt_entry(0xa09b, 0, 0xfffff);
/// assert_ne!(code64, 0);
/// ```
pub fn gdt_entry(flags: u16, base: u32, limit: u32) -> u64 {
    crate::verified::memory::gdt_entry(flags, base, limit)
}

/// Convert a raw GDT descriptor entry into a KVM segment register.
///
/// Decodes all fields from the 8-byte GDT descriptor and produces a
/// [`kvm_segment`] suitable for loading into `sregs.cs`, `sregs.ds`,
/// `sregs.tr`, etc.
///
/// `table_index` is the GDT entry number (0–3), which is multiplied by
/// 8 to produce the segment selector value.
///
/// If the Present bit is clear, the segment is marked `unusable = 1`.
///
/// # Example
///
/// ```
/// use chaoscontrol_vmm::memory::{gdt_entry, kvm_segment_from_gdt};
///
/// let entry = gdt_entry(0xa09b, 0, 0xfffff);
/// let cs = kvm_segment_from_gdt(entry, 1);
/// assert_eq!(cs.selector, 0x08);
/// assert_eq!(cs.l, 1);  // 64-bit mode
/// ```
pub fn kvm_segment_from_gdt(entry: u64, table_index: u8) -> kvm_segment {
    kvm_segment {
        base: get_base(entry),
        limit: get_limit(entry),
        selector: u16::from(table_index) * 8,
        type_: get_type(entry),
        present: get_p(entry),
        dpl: get_dpl(entry),
        db: get_db(entry),
        s: get_s(entry),
        l: get_l(entry),
        g: get_g(entry),
        avl: get_avl(entry),
        padding: 0,
        unusable: if get_p(entry) == 0 { 1 } else { 0 },
    }
}

/// Build the 64-bit code segment register (GDT index 1, selector `0x08`).
///
/// Convenience wrapper around [`kvm_segment_from_gdt`] with the standard
/// [`GDT_FLAGS_CODE64`] flags.  Use this for `sregs.cs`.
pub fn code64_segment() -> kvm_segment {
    kvm_segment_from_gdt(gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff), GDT_INDEX_CODE)
}

/// Build the data segment register (GDT index 2, selector `0x10`).
///
/// Convenience wrapper around [`kvm_segment_from_gdt`] with the standard
/// [`GDT_FLAGS_DATA`] flags.  Use this for `sregs.ds`, `sregs.es`,
/// `sregs.fs`, `sregs.gs`, and `sregs.ss`.
pub fn data_segment() -> kvm_segment {
    kvm_segment_from_gdt(gdt_entry(GDT_FLAGS_DATA, 0, 0xfffff), GDT_INDEX_DATA)
}

/// Build the TSS segment register (GDT index 3, selector `0x18`).
///
/// Convenience wrapper around [`kvm_segment_from_gdt`] with the standard
/// [`GDT_FLAGS_TSS`] flags.  Use this for `sregs.tr`.
pub fn tss_segment() -> kvm_segment {
    kvm_segment_from_gdt(gdt_entry(GDT_FLAGS_TSS, 0, 0xfffff), GDT_INDEX_TSS)
}

// ─── GDT field extraction helpers ────────────────────────────────────
//
// These decode individual fields from a raw 8-byte x86 segment
// descriptor.  The bit layout is:
//
//   Bits 63:56 — Base address [31:24]
//   Bit  55    — Granularity (G)
//   Bit  54    — Default operation size (D/B)
//   Bit  53    — 64-bit code segment (L)
//   Bit  52    — Available for system use (AVL)
//   Bits 51:48 — Segment limit [19:16]
//   Bit  47    — Present (P)
//   Bits 46:45 — Descriptor Privilege Level (DPL)
//   Bit  44    — Descriptor type: 0=system, 1=code/data (S)
//   Bits 43:40 — Type
//   Bits 39:16 — Base address [23:0]
//   Bits 15:0  — Segment limit [15:0]

/// Extract the segment base address (32-bit) from a GDT entry.
fn get_base(entry: u64) -> u64 {
    crate::verified::memory::get_base(entry)
}

/// Extract the segment limit from a GDT entry.
///
/// If the granularity bit (G) is set, the 20-bit limit is scaled by
/// 4096 (left-shifted by 12) and the low 12 bits are filled with 1s,
/// giving a byte-granular effective limit.
fn get_limit(entry: u64) -> u32 {
    crate::verified::memory::get_limit(entry)
}

/// Extract the Granularity bit (G) — bit 55.
fn get_g(entry: u64) -> u8 {
    crate::verified::memory::get_g(entry)
}

/// Extract the Default operation size bit (D/B) — bit 54.
fn get_db(entry: u64) -> u8 {
    crate::verified::memory::get_db(entry)
}

/// Extract the Long mode bit (L) — bit 53.
fn get_l(entry: u64) -> u8 {
    crate::verified::memory::get_l(entry)
}

/// Extract the Available for system use bit (AVL) — bit 52.
fn get_avl(entry: u64) -> u8 {
    crate::verified::memory::get_avl(entry)
}

/// Extract the Present bit (P) — bit 47.
fn get_p(entry: u64) -> u8 {
    crate::verified::memory::get_p(entry)
}

/// Extract the Descriptor Privilege Level (DPL) — bits 46:45.
fn get_dpl(entry: u64) -> u8 {
    crate::verified::memory::get_dpl(entry)
}

/// Extract the Descriptor type bit (S) — bit 44.
///
/// Returns 0 for system segments (LDT, TSS, gates) or 1 for code/data.
fn get_s(entry: u64) -> u8 {
    crate::verified::memory::get_s(entry)
}

/// Extract the Type field — bits 43:40.
fn get_type(entry: u64) -> u8 {
    crate::verified::memory::get_type(entry)
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Memory layout constants ─────────────────────────────────────

    #[test]
    fn memory_layout_constants_are_ordered() {
        // Compile-time checks that the memory layout is correctly ordered.
        const {
            assert!(BOOT_GDT_OFFSET < BOOT_IDT_OFFSET);
            assert!(BOOT_IDT_OFFSET < ZERO_PAGE_START);
            assert!(ZERO_PAGE_START < BOOT_STACK_POINTER);
            assert!(BOOT_STACK_POINTER < PML4_START);
            assert!(PML4_START < PDPTE_START);
            assert!(PDPTE_START < PDE_START);
            assert!(PDE_START < CMDLINE_START);
            assert!(CMDLINE_START < HIMEM_START);
        }
    }

    #[test]
    fn page_tables_do_not_overlap_cmdline() {
        // PDE table occupies PDE_START .. PDE_START + 512 * 8.
        const {
            assert!(PDE_START + PDE_ENTRY_COUNT * 8 <= CMDLINE_START);
        }
    }

    // ─── E820 ────────────────────────────────────────────────────────

    #[test]
    fn e820_map_has_two_entries() {
        let map = build_e820_map(128 * 1024 * 1024);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn e820_low_memory_entry() {
        let map = build_e820_map(128 * 1024 * 1024);
        assert_eq!(map[0].addr, 0);
        assert_eq!(map[0].size, LOW_MEMORY_END);
        assert_eq!(map[0].type_, E820_RAM);
    }

    #[test]
    fn e820_high_memory_entry() {
        let mem_size: u64 = 128 * 1024 * 1024;
        let map = build_e820_map(mem_size);
        assert_eq!(map[1].addr, HIMEM_START);
        assert_eq!(map[1].size, mem_size - HIMEM_START);
        assert_eq!(map[1].type_, E820_RAM);
    }

    #[test]
    #[should_panic(expected = "must be larger than HIMEM_START")]
    fn e820_panics_on_tiny_memory() {
        build_e820_map(HIMEM_START);
    }

    // ─── GDT entry construction ──────────────────────────────────────

    #[test]
    fn gdt_null_entry_is_zero() {
        assert_eq!(gdt_entry(0, 0, 0), 0);
    }

    #[test]
    fn gdt_code64_entry_nonzero() {
        let entry = gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff);
        assert_ne!(entry, 0);
    }

    #[test]
    fn gdt_code64_is_long_mode() {
        let entry = gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff);
        assert_eq!(get_l(entry), 1, "L bit must be set for 64-bit code");
        assert_eq!(get_db(entry), 0, "D/B must be 0 when L=1 (64-bit mode)");
    }

    #[test]
    fn gdt_data_is_not_long_mode() {
        let entry = gdt_entry(GDT_FLAGS_DATA, 0, 0xfffff);
        assert_eq!(get_l(entry), 0, "L bit must be clear for data segment");
        assert_eq!(get_db(entry), 1, "D/B must be set for 32-bit data");
    }

    #[test]
    fn gdt_entries_are_present() {
        for &flags in &[GDT_FLAGS_CODE64, GDT_FLAGS_DATA, GDT_FLAGS_TSS] {
            let entry = gdt_entry(flags, 0, 0xfffff);
            assert_eq!(get_p(entry), 1, "Boot GDT entries must be present");
        }
    }

    #[test]
    fn gdt_entries_are_ring0() {
        for &flags in &[GDT_FLAGS_CODE64, GDT_FLAGS_DATA, GDT_FLAGS_TSS] {
            let entry = gdt_entry(flags, 0, 0xfffff);
            assert_eq!(get_dpl(entry), 0, "Boot GDT entries must be ring 0");
        }
    }

    #[test]
    fn gdt_tss_is_system_segment() {
        let entry = gdt_entry(GDT_FLAGS_TSS, 0, 0xfffff);
        assert_eq!(get_s(entry), 0, "TSS must be a system segment (S=0)");
    }

    #[test]
    fn gdt_code_and_data_are_non_system() {
        let code = gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff);
        let data = gdt_entry(GDT_FLAGS_DATA, 0, 0xfffff);
        assert_eq!(get_s(code), 1, "Code segment must have S=1");
        assert_eq!(get_s(data), 1, "Data segment must have S=1");
    }

    #[test]
    fn gdt_base_address_encoding() {
        let entry = gdt_entry(GDT_FLAGS_DATA, 0x12345678, 0xfffff);
        assert_eq!(get_base(entry), 0x12345678);
    }

    #[test]
    fn gdt_base_zero() {
        let entry = gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff);
        assert_eq!(get_base(entry), 0);
    }

    // ─── kvm_segment conversion ──────────────────────────────────────

    #[test]
    fn kvm_segment_code64_selector() {
        let seg = kvm_segment_from_gdt(gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff), 1);
        assert_eq!(seg.selector, 0x08);
    }

    #[test]
    fn kvm_segment_data_selector() {
        let seg = kvm_segment_from_gdt(gdt_entry(GDT_FLAGS_DATA, 0, 0xfffff), 2);
        assert_eq!(seg.selector, 0x10);
    }

    #[test]
    fn kvm_segment_tss_selector() {
        let seg = kvm_segment_from_gdt(gdt_entry(GDT_FLAGS_TSS, 0, 0xfffff), 3);
        assert_eq!(seg.selector, 0x18);
    }

    #[test]
    fn kvm_segment_present_entries_are_usable() {
        let seg = code64_segment();
        assert_eq!(seg.present, 1);
        assert_eq!(seg.unusable, 0);
    }

    #[test]
    fn kvm_segment_null_is_unusable() {
        let seg = kvm_segment_from_gdt(gdt_entry(0, 0, 0), 0);
        assert_eq!(seg.present, 0);
        assert_eq!(seg.unusable, 1);
    }

    // ─── Convenience segment builders ────────────────────────────────

    #[test]
    fn code64_segment_properties() {
        let cs = code64_segment();
        assert_eq!(cs.selector, 0x08);
        assert_eq!(cs.l, 1, "must be 64-bit");
        assert_eq!(cs.db, 0, "D/B must be 0 for 64-bit");
        assert_eq!(cs.present, 1);
        assert_eq!(cs.dpl, 0, "ring 0");
    }

    #[test]
    fn data_segment_properties() {
        let ds = data_segment();
        assert_eq!(ds.selector, 0x10);
        assert_eq!(ds.l, 0, "data is not 64-bit code");
        assert_eq!(ds.db, 1, "32-bit operand size");
        assert_eq!(ds.present, 1);
        assert_eq!(ds.dpl, 0, "ring 0");
    }

    #[test]
    fn tss_segment_properties() {
        let tr = tss_segment();
        assert_eq!(tr.selector, 0x18);
        assert_eq!(tr.s, 0, "TSS is a system segment");
        assert_eq!(tr.present, 1);
        assert_eq!(tr.dpl, 0, "ring 0");
    }

    // ─── GuestMemoryManager ─────────────────────────────────────────

    #[test]
    fn create_guest_memory() {
        let mgr = GuestMemoryManager::new(128 * 1024 * 1024).unwrap();
        assert_eq!(mgr.size(), 128 * 1024 * 1024);
    }

    #[test]
    fn guest_memory_inner_has_one_region() {
        let mgr = GuestMemoryManager::new(64 * 1024 * 1024).unwrap();
        assert_eq!(mgr.inner().num_regions(), 1);
    }

    #[test]
    fn host_address_is_nonzero() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        assert_ne!(mgr.host_address(), 0);
    }

    #[test]
    fn setup_page_tables_writes_correct_entries() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        mgr.setup_page_tables().unwrap();

        // PML4[0] → PDPTE.
        let pml4: u64 = mgr.inner().read_obj(GuestAddress(PML4_START)).unwrap();
        assert_eq!(pml4, PDPTE_START | PTE_PRESENT_WRITABLE);

        // PDPTE[0] → PDE.
        let pdpte: u64 = mgr.inner().read_obj(GuestAddress(PDPTE_START)).unwrap();
        assert_eq!(pdpte, PDE_START | PTE_PRESENT_WRITABLE);

        // PDE[0]: maps 0x0000_0000–0x001F_FFFF.
        let pde0: u64 = mgr.inner().read_obj(GuestAddress(PDE_START)).unwrap();
        assert_eq!(pde0, PDE_PRESENT_WRITABLE_PS);

        // PDE[1]: maps 0x0020_0000–0x003F_FFFF.
        let pde1: u64 = mgr.inner().read_obj(GuestAddress(PDE_START + 8)).unwrap();
        assert_eq!(pde1, (1u64 << 21) | PDE_PRESENT_WRITABLE_PS);

        // PDE[511]: maps 0x3FE0_0000–0x3FFF_FFFF.
        let pde511: u64 = mgr
            .inner()
            .read_obj(GuestAddress(PDE_START + 511 * 8))
            .unwrap();
        assert_eq!(pde511, (511u64 << 21) | PDE_PRESENT_WRITABLE_PS);
    }

    #[test]
    fn setup_gdt_writes_correct_entries() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        mgr.setup_gdt().unwrap();

        // NULL entry.
        let null: u64 = mgr.inner().read_obj(GuestAddress(BOOT_GDT_OFFSET)).unwrap();
        assert_eq!(null, 0);

        // CODE64 entry: must have L=1.
        let code: u64 = mgr
            .inner()
            .read_obj(GuestAddress(BOOT_GDT_OFFSET + 8))
            .unwrap();
        assert_ne!(code, 0);
        assert_eq!(get_l(code), 1);

        // DATA entry: must have L=0, D/B=1.
        let data: u64 = mgr
            .inner()
            .read_obj(GuestAddress(BOOT_GDT_OFFSET + 16))
            .unwrap();
        assert_ne!(data, 0);
        assert_eq!(get_l(data), 0);
        assert_eq!(get_db(data), 1);

        // IDT placeholder is zeroed.
        let idt: u64 = mgr.inner().read_obj(GuestAddress(BOOT_IDT_OFFSET)).unwrap();
        assert_eq!(idt, 0);
    }

    #[test]
    fn write_cmdline_with_nul_terminator() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        let cmdline = b"console=ttyS0\0";
        mgr.write_cmdline(cmdline).unwrap();

        let mut buf = vec![0u8; cmdline.len()];
        mgr.inner()
            .read_slice(&mut buf, GuestAddress(CMDLINE_START))
            .unwrap();
        assert_eq!(&buf, cmdline);
    }

    #[test]
    fn write_cmdline_auto_appends_nul() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        mgr.write_cmdline(b"console=ttyS0").unwrap();

        // Read back with room for the auto-appended NUL.
        let mut buf = vec![0u8; 14]; // "console=ttyS0" (13) + NUL (1)
        mgr.inner()
            .read_slice(&mut buf, GuestAddress(CMDLINE_START))
            .unwrap();
        assert_eq!(&buf, b"console=ttyS0\0");
    }

    #[test]
    fn write_cmdline_rejects_oversized() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        let huge = vec![b'x'; CMDLINE_MAX_SIZE + 1];
        let err = mgr.write_cmdline(&huge).unwrap_err();
        assert!(matches!(err, MemoryError::CmdlineTooLong { .. }));
    }

    #[test]
    fn dump_and_restore_roundtrip() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();

        // Write recognisable data structures.
        mgr.setup_page_tables().unwrap();
        mgr.setup_gdt().unwrap();

        // Take a snapshot.
        let snapshot = mgr.dump();
        assert_eq!(snapshot.len(), 4 * 1024 * 1024);

        // Overwrite memory with zeros to simulate state change.
        let zeros = vec![0u8; 4 * 1024 * 1024];
        mgr.inner().write_slice(&zeros, GuestAddress(0)).unwrap();

        // Verify memory was actually zeroed.
        let pml4: u64 = mgr.inner().read_obj(GuestAddress(PML4_START)).unwrap();
        assert_eq!(pml4, 0, "memory should be zeroed before restore");

        // Restore from snapshot.
        mgr.restore(&snapshot).unwrap();

        // Verify page tables are back.
        let pml4: u64 = mgr.inner().read_obj(GuestAddress(PML4_START)).unwrap();
        assert_eq!(pml4, PDPTE_START | PTE_PRESENT_WRITABLE);
    }

    #[test]
    fn restore_rejects_wrong_size() {
        let mgr = GuestMemoryManager::new(4 * 1024 * 1024).unwrap();
        let bad_data = vec![0u8; 1024];
        let err = mgr.restore(&bad_data).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::SnapshotSizeMismatch {
                expected: 4194304,
                actual: 1024,
            }
        ));
    }

    #[test]
    fn dump_returns_correct_size() {
        let mgr = GuestMemoryManager::new(2 * 1024 * 1024).unwrap();
        let data = mgr.dump();
        assert_eq!(data.len(), mgr.size());
    }
}
