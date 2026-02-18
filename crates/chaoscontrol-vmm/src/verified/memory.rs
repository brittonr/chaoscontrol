//! Pure, verifiable functions for guest memory layout and GDT encoding.
//!
//! Every function in this module is:
//! - **Pure**: no I/O, no system calls, no side effects beyond the return value.
//! - **Deterministic**: same inputs always produce the same outputs.
//! - **Assertion-guarded**: Tiger Style `debug_assert!` preconditions and
//!   postconditions on every non-trivial function.
//!
//! These properties make the functions suitable for formal verification with
//! [Verus](https://github.com/verus-lang/verus).  The corresponding spec
//! file is `verus/memory_spec.rs`.
//!
//! # Mapping to `memory.rs`
//!
//! | Verified function   | Delegated from in `memory.rs`              |
//! |---------------------|--------------------------------------------|
//! | [`gdt_entry`]       | `gdt_entry()`                              |
//! | [`get_base`]        | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_limit`]       | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_g`]           | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_db`]          | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_l`]           | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_avl`]         | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_p`]           | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_dpl`]         | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_s`]           | `kvm_segment_from_gdt()` (internal)        |
//! | [`get_type`]        | `kvm_segment_from_gdt()` (internal)        |
//! | [`build_e820_map`]  | `build_e820_map()`                         |

// ═══════════════════════════════════════════════════════════════════════
//  Constants (duplicated from memory.rs so this module is self-contained)
// ═══════════════════════════════════════════════════════════════════════

/// Start of high memory (1 MB).  Must equal `crate::memory::HIMEM_START`.
pub const HIMEM_START: u64 = 0x0010_0000;

/// Boot stack pointer.  Must equal `crate::memory::BOOT_STACK_POINTER`.
pub const BOOT_STACK_POINTER: u64 = 0x8ff0;

/// Zero page address.  Must equal `crate::memory::ZERO_PAGE_START`.
pub const ZERO_PAGE_START: u64 = 0x7000;

/// Kernel command-line address.  Must equal `crate::memory::CMDLINE_START`.
pub const CMDLINE_START: u64 = 0x20000;

/// GDT offset.  Must equal `crate::memory::BOOT_GDT_OFFSET`.
pub const BOOT_GDT_OFFSET: u64 = 0x500;

/// IDT offset.  Must equal `crate::memory::BOOT_IDT_OFFSET`.
pub const BOOT_IDT_OFFSET: u64 = 0x520;

/// PML4 page table address.  Must equal `crate::memory::PML4_START`.
pub const PML4_START: u64 = 0x9000;

/// PDPTE page table address.  Must equal `crate::memory::PDPTE_START`.
pub const PDPTE_START: u64 = 0xa000;

/// PDE page table address.  Must equal `crate::memory::PDE_START`.
pub const PDE_START: u64 = 0xb000;

/// Number of 2 MB page-directory entries (512 × 2 MB = 1 GB).
pub const PDE_ENTRY_COUNT: u64 = 512;

/// 64-bit code segment flags.  Must equal `crate::memory::GDT_FLAGS_CODE64`.
pub const GDT_FLAGS_CODE64: u16 = 0xa09b;

/// Data segment flags.  Must equal `crate::memory::GDT_FLAGS_DATA`.
pub const GDT_FLAGS_DATA: u16 = 0xc093;

/// TSS segment flags.  Must equal `crate::memory::GDT_FLAGS_TSS`.
pub const GDT_FLAGS_TSS: u16 = 0x808b;

/// E820 memory type: usable RAM.  Must equal `crate::memory::E820_RAM`.
pub const E820_RAM: u32 = 1;

/// End of conventional low memory.  Must equal `crate::memory::LOW_MEMORY_END`.
pub const LOW_MEMORY_END: u64 = 0x9fc00;

// ═══════════════════════════════════════════════════════════════════════
//  Compile-time layout verification
// ═══════════════════════════════════════════════════════════════════════
//
// These fire at compile time if any constant relationship is violated.
// No test runner needed — a broken invariant is a build failure.

// Memory regions are in strictly ascending address order.
const _: () = assert!(BOOT_GDT_OFFSET < BOOT_IDT_OFFSET);
const _: () = assert!(BOOT_IDT_OFFSET < ZERO_PAGE_START);
const _: () = assert!(ZERO_PAGE_START < BOOT_STACK_POINTER);
const _: () = assert!(BOOT_STACK_POINTER < PML4_START);
const _: () = assert!(PML4_START < PDPTE_START);
const _: () = assert!(PDPTE_START < PDE_START);
const _: () = assert!(PDE_START < CMDLINE_START);
const _: () = assert!(CMDLINE_START < HIMEM_START);

// Page table region (PDE_START .. PDE_START + 512×8 = 0xC000) fits before cmdline.
const _: () = assert!(PDE_START + PDE_ENTRY_COUNT * 8 <= CMDLINE_START);

// Low memory ends before high memory starts (E820 gap is valid).
const _: () = assert!(LOW_MEMORY_END < HIMEM_START);

// All standard GDT entries have the Present bit set (bit 7 of access byte).
const _: () = assert!(GDT_FLAGS_CODE64 & 0x0080 != 0);
const _: () = assert!(GDT_FLAGS_DATA & 0x0080 != 0);
const _: () = assert!(GDT_FLAGS_TSS & 0x0080 != 0);

// CODE64: flags nibble = 0xA → G=1, D/B=0, L=1, AVL=0.
const _: () = assert!((GDT_FLAGS_CODE64 >> 12) & 0xF == 0xA);

// DATA: flags nibble = 0xC → G=1, D/B=1, L=0, AVL=0.
const _: () = assert!((GDT_FLAGS_DATA >> 12) & 0xF == 0xC);

// TSS: S=0 (system segment — bit 4 of access byte is clear).
const _: () = assert!(GDT_FLAGS_TSS & 0x0010 == 0);

// CODE64 and DATA: S=1 (code/data segment — bit 4 of access byte is set).
const _: () = assert!(GDT_FLAGS_CODE64 & 0x0010 != 0);
const _: () = assert!(GDT_FLAGS_DATA & 0x0010 != 0);

// Bits [11:8] of the standard flags words are zero (gdt_entry masks them out).
const _: () = assert!(GDT_FLAGS_CODE64 & 0x0F00 == 0);
const _: () = assert!(GDT_FLAGS_DATA & 0x0F00 == 0);
const _: () = assert!(GDT_FLAGS_TSS & 0x0F00 == 0);

// ═══════════════════════════════════════════════════════════════════════
//  E820 memory map
// ═══════════════════════════════════════════════════════════════════════

/// A single entry in the E820 memory map.
///
/// This mirrors the layout of the Linux boot protocol's `boot_e820_entry`.
/// Use [`build_e820_map`] to construct the standard two-entry map for a
/// given memory size, then copy the entries into `boot_params.e820_table`.
///
/// # Example
///
/// ```
/// use chaoscontrol_vmm::memory::{build_e820_map, E820_RAM};
///
/// let map = build_e820_map(128 * 1024 * 1024);
/// assert_eq!(map.len(), 2);
/// assert_eq!(map[0].type_, E820_RAM);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct E820Entry {
    /// Start of the memory region (guest physical address).
    pub addr: u64,
    /// Size of the region in bytes.
    pub size: u64,
    /// Memory type (see [`E820_RAM`]).
    pub type_: u32,
}

/// Build the standard E820 memory map for a guest with `memory_size` bytes.
///
/// Returns exactly two entries:
///
/// 1. **Low memory** — from 0 to [`LOW_MEMORY_END`].
/// 2. **High memory** — from [`HIMEM_START`] to end of guest RAM.
///
/// # Panics
///
/// Panics if `memory_size <= HIMEM_START`.
///
/// # Properties verified by `verus/memory_spec.rs`
///
/// - `result.len() == 2`
/// - Both entries have type [`E820_RAM`]
/// - Entries do not overlap: `low.addr + low.size <= high.addr`
/// - High entry covers exactly `memory_size - HIMEM_START` bytes
pub fn build_e820_map(memory_size: u64) -> Vec<E820Entry> {
    // Precondition.
    assert!(
        memory_size > HIMEM_START,
        "Guest memory ({memory_size:#x}) must be larger than HIMEM_START ({HIMEM_START:#x})"
    );

    let result = vec![
        E820Entry {
            addr: 0,
            size: LOW_MEMORY_END,
            type_: E820_RAM,
        },
        E820Entry {
            addr: HIMEM_START,
            size: memory_size - HIMEM_START,
            type_: E820_RAM,
        },
    ];

    // Postcondition: exactly two entries.
    debug_assert_eq!(result.len(), 2, "E820 map must have exactly 2 entries");
    // Postcondition: entries do not overlap.
    debug_assert!(
        result[0].addr + result[0].size <= result[1].addr,
        "E820 low memory must not overlap high memory"
    );
    // Postcondition: both are RAM.
    debug_assert_eq!(result[0].type_, E820_RAM);
    debug_assert_eq!(result[1].type_, E820_RAM);

    result
}

// ═══════════════════════════════════════════════════════════════════════
//  GDT entry construction
// ═══════════════════════════════════════════════════════════════════════

/// Construct a raw 8-byte GDT descriptor from flags, base, and limit.
///
/// The `flags` parameter packs the access byte and flags nibble:
///
/// ```text
/// flags[7:0]   → descriptor byte 5  (access: P, DPL, S, Type)
/// flags[15:12] → descriptor byte 6  high nibble (G, D/B, L, AVL)
/// flags[11:8]  → masked out (unused)
/// ```
///
/// # Properties verified by `verus/memory_spec.rs`
///
/// - `get_base(gdt_entry(f, b, l)) == b as u64` (base round-trips)
/// - `get_type(gdt_entry(f, b, l)) == (f & 0xF) as u8` (type round-trips)
/// - `gdt_entry(0, 0, 0) == 0` (null entry)
pub fn gdt_entry(flags: u16, base: u32, limit: u32) -> u64 {
    let result = ((u64::from(base) & 0xff00_0000u64) << (56 - 24))
        | ((u64::from(flags) & 0x0000_f0ffu64) << 40)
        | ((u64::from(limit) & 0x000f_0000u64) << (48 - 16))
        | ((u64::from(base) & 0x00ff_ffffu64) << 16)
        | (u64::from(limit) & 0x0000_ffffu64);

    // Postcondition: base address round-trips through extraction.
    debug_assert_eq!(
        get_base(result),
        base as u64,
        "gdt_entry: base must round-trip (base={base:#x}, got {:#x})",
        get_base(result)
    );
    // Postcondition: type field round-trips through extraction.
    debug_assert_eq!(
        get_type(result),
        (flags & 0xF) as u8,
        "gdt_entry: type field must round-trip (flags={flags:#x})"
    );

    result
}

// ═══════════════════════════════════════════════════════════════════════
//  GDT field extraction
// ═══════════════════════════════════════════════════════════════════════
//
// x86 segment descriptor bit layout (8 bytes):
//
//   Bits 63:56 — Base address [31:24]
//   Bit  55    — Granularity (G)
//   Bit  54    — Default operation size (D/B)
//   Bit  53    — 64-bit code segment (L)
//   Bit  52    — Available for system use (AVL)
//   Bits 51:48 — Segment limit [19:16]
//   Bit  47    — Present (P)
//   Bits 46:45 — Descriptor Privilege Level (DPL)
//   Bit  44    — Descriptor type: S (0=system, 1=code/data)
//   Bits 43:40 — Type
//   Bits 39:16 — Base address [23:0]
//   Bits 15:0  — Segment limit [15:0]

/// Extract the segment base address from a GDT entry.
///
/// Reassembles the 32-bit base from three non-contiguous fields in the
/// descriptor.
///
/// # Properties verified by `verus/memory_spec.rs`
///
/// - `result <= 0xFFFF_FFFF` (fits in 32 bits)
/// - `get_base(gdt_entry(f, b, l)) == b as u64` (inverse of encoding)
pub fn get_base(entry: u64) -> u64 {
    let result = ((entry & 0xFF00_0000_0000_0000) >> 32)
        | ((entry & 0x0000_00FF_0000_0000) >> 16)
        | ((entry & 0x0000_0000_FFFF_0000) >> 16);

    // Postcondition: result fits in 32 bits.
    debug_assert!(
        result <= 0xFFFF_FFFF,
        "get_base: result {result:#x} exceeds 32 bits"
    );
    // Postcondition: high 32 bits are zero.
    debug_assert_eq!(result >> 32, 0, "get_base: upper 32 bits must be zero");

    result
}

/// Extract the segment limit from a GDT entry.
///
/// When the granularity bit (G) is clear, returns the raw 20-bit limit.
/// When G is set, the 20-bit value is scaled: `(raw << 12) | 0xFFF`.
///
/// # Properties verified by `verus/memory_spec.rs`
///
/// - When `G=0`: `result <= 0xFFFFF` (20-bit)
/// - When `G=1`: `result == (raw << 12) | 0xFFF`
pub fn get_limit(entry: u64) -> u32 {
    let raw: u32 =
        ((((entry) & 0x000F_0000_0000_0000) >> 32) | ((entry) & 0x0000_0000_0000_FFFF)) as u32;

    // Invariant: raw limit is at most 20 bits.
    debug_assert!(
        raw <= 0xFFFFF,
        "get_limit: raw limit {raw:#x} exceeds 20 bits"
    );

    let result = match get_g(entry) {
        0 => raw,
        _ => (raw << 12) | 0xFFF,
    };

    // Postcondition: when G=0, limit fits in 20 bits.
    debug_assert!(
        get_g(entry) != 0 || result <= 0xFFFFF,
        "get_limit: with G=0, result {result:#x} exceeds 20 bits"
    );

    result
}

/// Extract the Granularity bit (G) — bit 55.  Returns 0 or 1.
pub fn get_g(entry: u64) -> u8 {
    let result = ((entry & 0x0080_0000_0000_0000) >> 55) as u8;
    debug_assert!(result <= 1, "get_g: single bit must be 0 or 1");
    result
}

/// Extract the Default operation size bit (D/B) — bit 54.  Returns 0 or 1.
pub fn get_db(entry: u64) -> u8 {
    let result = ((entry & 0x0040_0000_0000_0000) >> 54) as u8;
    debug_assert!(result <= 1, "get_db: single bit must be 0 or 1");
    result
}

/// Extract the Long mode bit (L) — bit 53.  Returns 0 or 1.
pub fn get_l(entry: u64) -> u8 {
    let result = ((entry & 0x0020_0000_0000_0000) >> 53) as u8;
    debug_assert!(result <= 1, "get_l: single bit must be 0 or 1");
    result
}

/// Extract the Available for system use bit (AVL) — bit 52.  Returns 0 or 1.
pub fn get_avl(entry: u64) -> u8 {
    let result = ((entry & 0x0010_0000_0000_0000) >> 52) as u8;
    debug_assert!(result <= 1, "get_avl: single bit must be 0 or 1");
    result
}

/// Extract the Present bit (P) — bit 47.  Returns 0 or 1.
pub fn get_p(entry: u64) -> u8 {
    let result = ((entry & 0x0000_8000_0000_0000) >> 47) as u8;
    debug_assert!(result <= 1, "get_p: single bit must be 0 or 1");
    result
}

/// Extract the Descriptor Privilege Level (DPL) — bits 46:45.  Returns 0–3.
pub fn get_dpl(entry: u64) -> u8 {
    let result = ((entry & 0x0000_6000_0000_0000) >> 45) as u8;
    debug_assert!(result <= 3, "get_dpl: 2-bit field must be 0..=3");
    result
}

/// Extract the Descriptor type bit (S) — bit 44.  Returns 0 or 1.
///
/// 0 = system segment (LDT, TSS, gate), 1 = code or data segment.
pub fn get_s(entry: u64) -> u8 {
    let result = ((entry & 0x0000_1000_0000_0000) >> 44) as u8;
    debug_assert!(result <= 1, "get_s: single bit must be 0 or 1");
    result
}

/// Extract the Type field — bits 43:40.  Returns 0–15.
pub fn get_type(entry: u64) -> u8 {
    let result = ((entry & 0x0000_0F00_0000_0000) >> 40) as u8;
    debug_assert!(result <= 0xF, "get_type: 4-bit field must be 0..=0xF");
    result
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Cross-module constant consistency ───────────────────────
    //
    // These catch any drift between the constants here and the
    // canonical values in crate::memory.

    #[test]
    fn constants_match_memory_module() {
        use crate::memory;
        assert_eq!(HIMEM_START, memory::HIMEM_START);
        assert_eq!(BOOT_STACK_POINTER, memory::BOOT_STACK_POINTER);
        assert_eq!(ZERO_PAGE_START, memory::ZERO_PAGE_START);
        assert_eq!(CMDLINE_START, memory::CMDLINE_START);
        assert_eq!(BOOT_GDT_OFFSET, memory::BOOT_GDT_OFFSET);
        assert_eq!(BOOT_IDT_OFFSET, memory::BOOT_IDT_OFFSET);
        assert_eq!(PML4_START, memory::PML4_START);
        assert_eq!(PDPTE_START, memory::PDPTE_START);
        assert_eq!(PDE_START, memory::PDE_START);
        assert_eq!(GDT_FLAGS_CODE64, memory::GDT_FLAGS_CODE64);
        assert_eq!(GDT_FLAGS_DATA, memory::GDT_FLAGS_DATA);
        assert_eq!(GDT_FLAGS_TSS, memory::GDT_FLAGS_TSS);
        assert_eq!(E820_RAM, memory::E820_RAM);
        assert_eq!(LOW_MEMORY_END, memory::LOW_MEMORY_END);
    }

    // ─── E820 ────────────────────────────────────────────────────

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
    fn e820_entries_do_not_overlap() {
        for &size in &[
            HIMEM_START + 1,
            2 * 1024 * 1024,
            128 * 1024 * 1024,
            4 * 1024 * 1024 * 1024u64,
        ] {
            let map = build_e820_map(size);
            assert!(
                map[0].addr + map[0].size <= map[1].addr,
                "E820 entries overlap for memory_size={size:#x}"
            );
        }
    }

    #[test]
    #[should_panic(expected = "must be larger than HIMEM_START")]
    fn e820_panics_on_tiny_memory() {
        build_e820_map(HIMEM_START);
    }

    // ─── gdt_entry construction ──────────────────────────────────

    #[test]
    fn gdt_null_entry_is_zero() {
        assert_eq!(gdt_entry(0, 0, 0), 0);
    }

    #[test]
    fn gdt_code64_entry_nonzero() {
        assert_ne!(gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff), 0);
    }

    #[test]
    fn gdt_entry_base_roundtrip_zero() {
        let entry = gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff);
        assert_eq!(get_base(entry), 0);
    }

    #[test]
    fn gdt_entry_base_roundtrip_nonzero() {
        for &base in &[0x1u32, 0xFFFF, 0x1234_5678, 0xFF00_0000, 0xFFFF_FFFF] {
            let entry = gdt_entry(GDT_FLAGS_DATA, base, 0xfffff);
            assert_eq!(
                get_base(entry),
                base as u64,
                "base {base:#x} did not round-trip"
            );
        }
    }

    #[test]
    fn gdt_entry_limit_roundtrip_g0() {
        // When G=0, flags nibble high bit is clear → raw limit should match.
        // Use flags with G=0: flags nibble = 0x0 (no granularity, no D/B, no L).
        let flags_g0: u16 = 0x0093; // access=0x93, flags nibble=0x0 (G=0)
        for &limit in &[0x0u32, 0x1, 0xFFFF, 0xFFFFF] {
            let entry = gdt_entry(flags_g0, 0, limit);
            assert_eq!(get_g(entry), 0, "G bit should be 0 for flags {flags_g0:#x}");
            assert_eq!(
                get_limit(entry),
                limit,
                "limit {limit:#x} did not round-trip with G=0"
            );
        }
    }

    #[test]
    fn gdt_entry_type_roundtrip() {
        // Type is the low 4 bits of the flags access byte.
        for type_val in 0..=0xFu8 {
            let flags: u16 = 0x8000 | (type_val as u16); // P=1 + type
            let entry = gdt_entry(flags, 0, 0);
            assert_eq!(
                get_type(entry),
                type_val,
                "type {type_val:#x} did not round-trip"
            );
        }
    }

    #[test]
    fn gdt_entry_present_bit_roundtrip() {
        // P is bit 7 of the access byte = flags[7].
        let flags_present: u16 = 0x80; // only P set
        let flags_not_present: u16 = 0x00;
        assert_eq!(get_p(gdt_entry(flags_present, 0, 0)), 1);
        assert_eq!(get_p(gdt_entry(flags_not_present, 0, 0)), 0);
    }

    #[test]
    fn gdt_code64_fields() {
        let entry = gdt_entry(GDT_FLAGS_CODE64, 0, 0xfffff);
        assert_eq!(get_l(entry), 1, "L must be set for 64-bit code");
        assert_eq!(get_db(entry), 0, "D/B must be 0 when L=1");
        assert_eq!(get_g(entry), 1, "G must be set");
        assert_eq!(get_p(entry), 1, "P must be set");
        assert_eq!(get_s(entry), 1, "S=1 for code/data");
        assert_eq!(get_dpl(entry), 0, "DPL=0 (ring 0)");
    }

    #[test]
    fn gdt_data_fields() {
        let entry = gdt_entry(GDT_FLAGS_DATA, 0, 0xfffff);
        assert_eq!(get_l(entry), 0, "L must be clear for data");
        assert_eq!(get_db(entry), 1, "D/B=1 for 32-bit data");
        assert_eq!(get_g(entry), 1, "G must be set");
        assert_eq!(get_p(entry), 1, "P must be set");
        assert_eq!(get_s(entry), 1, "S=1 for code/data");
        assert_eq!(get_dpl(entry), 0, "DPL=0 (ring 0)");
    }

    #[test]
    fn gdt_tss_fields() {
        let entry = gdt_entry(GDT_FLAGS_TSS, 0, 0xfffff);
        assert_eq!(get_s(entry), 0, "S=0 for system segment (TSS)");
        assert_eq!(get_p(entry), 1, "P must be set");
        assert_eq!(get_dpl(entry), 0, "DPL=0 (ring 0)");
    }

    // ─── GDT field extraction ranges ─────────────────────────────

    #[test]
    fn get_base_fits_in_32_bits() {
        // Exhaustive on interesting values — the extractor must never
        // produce a value wider than 32 bits.
        for &entry in &[
            0u64,
            0xFFFF_FFFF_FFFF_FFFF,
            0xFF00_00FF_FFFF_0000,
            0x0000_0000_FFFF_FFFF,
        ] {
            assert!(
                get_base(entry) <= 0xFFFF_FFFF,
                "get_base({entry:#x}) = {:#x} exceeds 32 bits",
                get_base(entry)
            );
        }
    }

    #[test]
    fn get_limit_raw_is_20_bits() {
        // When G=0, the limit must be at most 0xFFFFF.
        // Build an entry with G=0 and maximum limit bits set.
        let entry_max_limit: u64 = 0x000F_0000_0000_FFFF; // bits [51:48]=0xF, [15:0]=0xFFFF
                                                          // But G (bit 55) is 0 in this value.
        assert_eq!(get_g(entry_max_limit), 0);
        assert_eq!(get_limit(entry_max_limit), 0xFFFFF);
    }

    #[test]
    fn get_limit_with_granularity() {
        // Set G=1 (bit 55) and maximum raw limit.
        let entry: u64 = 0x008F_0000_0000_FFFF; // G=1, limit[19:16]=0xF, limit[15:0]=0xFFFF
        assert_eq!(get_g(entry), 1);
        assert_eq!(get_limit(entry), 0xFFFF_FFFF);
    }

    #[test]
    fn single_bit_extractors_return_0_or_1() {
        for &entry in &[0u64, 0xFFFF_FFFF_FFFF_FFFF, 0xAAAA_AAAA_AAAA_AAAA] {
            assert!(get_g(entry) <= 1, "get_g out of range for {entry:#x}");
            assert!(get_db(entry) <= 1, "get_db out of range for {entry:#x}");
            assert!(get_l(entry) <= 1, "get_l out of range for {entry:#x}");
            assert!(get_avl(entry) <= 1, "get_avl out of range for {entry:#x}");
            assert!(get_p(entry) <= 1, "get_p out of range for {entry:#x}");
            assert!(get_s(entry) <= 1, "get_s out of range for {entry:#x}");
        }
    }

    #[test]
    fn get_dpl_returns_0_to_3() {
        for &entry in &[
            0u64,
            0xFFFF_FFFF_FFFF_FFFF,
            0x0000_2000_0000_0000,
            0x0000_6000_0000_0000,
        ] {
            assert!(get_dpl(entry) <= 3, "get_dpl out of range for {entry:#x}");
        }
    }

    #[test]
    fn get_type_returns_0_to_f() {
        for &entry in &[0u64, 0xFFFF_FFFF_FFFF_FFFF, 0x0000_0500_0000_0000] {
            assert!(
                get_type(entry) <= 0xF,
                "get_type out of range for {entry:#x}"
            );
        }
    }

    // ─── Specific DPL extraction ─────────────────────────────────

    #[test]
    fn get_dpl_specific_values() {
        // DPL is bits [46:45].  Construct entries with each DPL value.
        assert_eq!(get_dpl(0x0000_0000_0000_0000), 0); // DPL=0
        assert_eq!(get_dpl(0x0000_2000_0000_0000), 1); // DPL=1
        assert_eq!(get_dpl(0x0000_4000_0000_0000), 2); // DPL=2
        assert_eq!(get_dpl(0x0000_6000_0000_0000), 3); // DPL=3
    }

    // ─── Round-trip: build → extract on standard entries ─────────

    #[test]
    fn standard_entries_roundtrip_all_fields() {
        let cases: &[(u16, u32, u32)] = &[
            (GDT_FLAGS_CODE64, 0, 0xfffff),
            (GDT_FLAGS_DATA, 0, 0xfffff),
            (GDT_FLAGS_TSS, 0, 0xfffff),
            (GDT_FLAGS_DATA, 0x1234_5678, 0xabcde),
        ];
        for &(flags, base, limit) in cases {
            let entry = gdt_entry(flags, base, limit);
            assert_eq!(
                get_base(entry),
                base as u64,
                "base mismatch for flags={flags:#x} base={base:#x}"
            );
            assert_eq!(
                get_type(entry),
                (flags & 0xF) as u8,
                "type mismatch for flags={flags:#x}"
            );
            assert_eq!(
                get_p(entry),
                ((flags >> 7) & 1) as u8,
                "P mismatch for flags={flags:#x}"
            );
            assert_eq!(
                get_s(entry),
                ((flags >> 4) & 1) as u8,
                "S mismatch for flags={flags:#x}"
            );
            assert_eq!(
                get_g(entry),
                ((flags >> 15) & 1) as u8,
                "G mismatch for flags={flags:#x}"
            );
            assert_eq!(
                get_l(entry),
                ((flags >> 13) & 1) as u8,
                "L mismatch for flags={flags:#x}"
            );
            assert_eq!(
                get_db(entry),
                ((flags >> 14) & 1) as u8,
                "D/B mismatch for flags={flags:#x}"
            );
            assert_eq!(
                get_avl(entry),
                ((flags >> 12) & 1) as u8,
                "AVL mismatch for flags={flags:#x}"
            );
        }
    }
}
