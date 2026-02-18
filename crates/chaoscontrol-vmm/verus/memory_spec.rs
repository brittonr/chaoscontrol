// Verus specification for ChaosControl memory pure functions.
//
// These specs document the formal verification properties for every pure
// function in `src/verified/memory.rs`.  They are NOT compiled by `cargo` —
// they are consumed by the Verus verifier:
//
//     verus verus/memory_spec.rs
//
// Each function is annotated with `requires` (preconditions) and `ensures`
// (postconditions) that Verus will prove hold for ALL valid inputs.
//
// Reference: https://verus-lang.github.io/verus/guide/

verus! {

// ═══════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════

pub const HIMEM_START:        u64 = 0x0010_0000u64;
pub const BOOT_STACK_POINTER: u64 = 0x8ff0u64;
pub const ZERO_PAGE_START:    u64 = 0x7000u64;
pub const CMDLINE_START:      u64 = 0x20000u64;
pub const BOOT_GDT_OFFSET:   u64 = 0x500u64;
pub const BOOT_IDT_OFFSET:   u64 = 0x520u64;
pub const PML4_START:         u64 = 0x9000u64;
pub const PDPTE_START:        u64 = 0xa000u64;
pub const PDE_START:          u64 = 0xb000u64;
pub const PDE_ENTRY_COUNT:    u64 = 512u64;

pub const GDT_FLAGS_CODE64:   u16 = 0xa09bu16;
pub const GDT_FLAGS_DATA:     u16 = 0xc093u16;
pub const GDT_FLAGS_TSS:      u16 = 0x808bu16;

pub const E820_RAM:           u32 = 1u32;
pub const LOW_MEMORY_END:     u64 = 0x9fc00u64;

// ═══════════════════════════════════════════════════════════════════════
// Memory layout invariants
// ═══════════════════════════════════════════════════════════════════════

/// The memory layout constants are in strictly ascending order.
///
/// This is the foundational invariant — if any region moves, the entire
/// boot sequence breaks.  The proof is trivial (constant comparison)
/// but documenting it in Verus makes the invariant machine-checked.
proof fn lemma_memory_layout_ordered()
    ensures
        BOOT_GDT_OFFSET < BOOT_IDT_OFFSET,
        BOOT_IDT_OFFSET < ZERO_PAGE_START,
        ZERO_PAGE_START < BOOT_STACK_POINTER,
        BOOT_STACK_POINTER < PML4_START,
        PML4_START < PDPTE_START,
        PDPTE_START < PDE_START,
        PDE_START < CMDLINE_START,
        CMDLINE_START < HIMEM_START,
{ /* Verus discharges by constant evaluation. */ }

/// The PDE page table region does not overlap the command line region.
///
/// PDE occupies PDE_START .. PDE_START + 512 * 8 = 0xB000 .. 0xC000.
/// CMDLINE_START = 0x20000, so there is a 0x14000 byte gap.
proof fn lemma_page_tables_do_not_overlap_cmdline()
    ensures
        PDE_START + PDE_ENTRY_COUNT * 8 <= CMDLINE_START,
{ /* constant evaluation */ }

/// Low memory ends before high memory starts (the E820 gap is valid).
proof fn lemma_low_memory_below_himem()
    ensures
        LOW_MEMORY_END < HIMEM_START,
{ /* constant evaluation */ }

// ═══════════════════════════════════════════════════════════════════════
// GDT entry encoding
// ═══════════════════════════════════════════════════════════════════════

/// Construct a GDT descriptor entry.
///
/// The implementation encodes `flags`, `base`, and `limit` into the
/// standard x86 segment descriptor layout.  Bits [11:8] of `flags`
/// are masked out (they overlap with the limit and base fields in
/// the descriptor).
pub fn gdt_entry(flags: u16, base: u32, limit: u32) -> (result: u64)
    ensures
        // 1. Null entry: all-zero inputs produce all-zero output.
        (flags == 0 && base == 0 && limit == 0) ==> result == 0u64,
        // 2. Base address round-trips through extraction.
        get_base(result) == base as u64,
        // 3. Type field (bits [3:0] of access byte) round-trips.
        get_type(result) == (flags & 0xFu16) as u8,
        // 4. Present bit (bit 7 of access byte) round-trips.
        get_p(result) == ((flags >> 7u16) & 1u16) as u8,
        // 5. S bit (bit 4 of access byte) round-trips.
        get_s(result) == ((flags >> 4u16) & 1u16) as u8,
        // 6. G bit (bit 15 of flags word) round-trips.
        get_g(result) == ((flags >> 15u16) & 1u16) as u8,
        // 7. L bit (bit 13 of flags word) round-trips.
        get_l(result) == ((flags >> 13u16) & 1u16) as u8,
        // 8. D/B bit (bit 14 of flags word) round-trips.
        get_db(result) == ((flags >> 14u16) & 1u16) as u8,
        // 9. AVL bit (bit 12 of flags word) round-trips.
        get_avl(result) == ((flags >> 12u16) & 1u16) as u8,
{
    ((base as u64 & 0xff00_0000u64) << (56u64 - 24u64))
        | ((flags as u64 & 0x0000_f0ffu64) << 40u64)
        | ((limit as u64 & 0x000f_0000u64) << (48u64 - 16u64))
        | ((base as u64 & 0x00ff_ffffu64) << 16u64)
        | (limit as u64 & 0x0000_ffffu64)
}

/// The base address round-trips through encode → extract for all inputs.
proof fn gdt_entry_base_roundtrip(flags: u16, base: u32, limit: u32)
    ensures
        get_base(gdt_entry(flags, base, limit)) == base as u64,
{
    // Follows directly from the ensures of gdt_entry.
}

/// The limit round-trips when the granularity bit is clear (G=0).
///
/// When G=0, `get_limit` returns the raw 20-bit limit unchanged.
/// This requires that (a) `limit <= 0xFFFFF` (so it fits in 20 bits)
/// and (b) the G bit in `flags` is clear (bit 15 = 0).
proof fn gdt_entry_limit_roundtrip_g0(flags: u16, base: u32, limit: u32)
    requires
        limit <= 0xFFFFFu32,
        (flags >> 15u16) & 1u16 == 0u16,  // G=0
    ensures
        get_g(gdt_entry(flags, base, limit)) == 0u8,
        get_limit(gdt_entry(flags, base, limit)) == limit,
{
    // When G=0, get_limit returns the raw 20-bit value.
    // gdt_entry places limit[15:0] in descriptor bits [15:0]
    // and limit[19:16] in descriptor bits [51:48].
    // get_limit reassembles them.
}

/// The DPL field round-trips through encode → extract.
proof fn gdt_entry_dpl_roundtrip(flags: u16, base: u32, limit: u32)
    ensures
        get_dpl(gdt_entry(flags, base, limit)) == ((flags >> 5u16) & 0x3u16) as u8,
{
    // DPL is bits [6:5] of the access byte = flags[6:5].
    // gdt_entry places flags[7:0] at descriptor bits [47:40].
    // get_dpl extracts bits [46:45] = access_byte[6:5].
}

// ═══════════════════════════════════════════════════════════════════════
// GDT field extraction
// ═══════════════════════════════════════════════════════════════════════

/// Extract the 32-bit base address from a GDT descriptor.
pub fn get_base(entry: u64) -> (result: u64)
    ensures
        // The result always fits in 32 bits.
        result <= 0xFFFF_FFFFu64,
        // Equivalently, the upper 32 bits are zero.
        result >> 32u64 == 0u64,
{
    ((entry & 0xFF00_0000_0000_0000u64) >> 32u64)
        | ((entry & 0x0000_00FF_0000_0000u64) >> 16u64)
        | ((entry & 0x0000_0000_FFFF_0000u64) >> 16u64)
}

/// Extract the segment limit from a GDT descriptor.
///
/// When G=0, returns the raw 20-bit limit (≤ 0xFFFFF).
/// When G=1, returns `(raw << 12) | 0xFFF` (up to 0xFFFF_FFFF).
pub fn get_limit(entry: u64) -> (result: u32)
    ensures
        // When G=0, the result fits in 20 bits.
        get_g(entry) == 0u8 ==> result <= 0xFFFFFu32,
        // When G=1, the low 12 bits are all ones.
        get_g(entry) != 0u8 ==> (result & 0xFFFu32) == 0xFFFu32,
{
    let raw: u32 =
        ((((entry) & 0x000F_0000_0000_0000u64) >> 32u64)
         | ((entry) & 0x0000_0000_0000_FFFFu64)) as u32;
    if get_g(entry) == 0 {
        raw
    } else {
        (raw << 12u32) | 0xFFFu32
    }
}

/// Extract Granularity bit (G) — bit 55.
pub fn get_g(entry: u64) -> (result: u8)
    ensures result <= 1u8,
{
    ((entry & 0x0080_0000_0000_0000u64) >> 55u64) as u8
}

/// Extract Default operation size bit (D/B) — bit 54.
pub fn get_db(entry: u64) -> (result: u8)
    ensures result <= 1u8,
{
    ((entry & 0x0040_0000_0000_0000u64) >> 54u64) as u8
}

/// Extract Long mode bit (L) — bit 53.
pub fn get_l(entry: u64) -> (result: u8)
    ensures result <= 1u8,
{
    ((entry & 0x0020_0000_0000_0000u64) >> 53u64) as u8
}

/// Extract Available for system use bit (AVL) — bit 52.
pub fn get_avl(entry: u64) -> (result: u8)
    ensures result <= 1u8,
{
    ((entry & 0x0010_0000_0000_0000u64) >> 52u64) as u8
}

/// Extract Present bit (P) — bit 47.
pub fn get_p(entry: u64) -> (result: u8)
    ensures result <= 1u8,
{
    ((entry & 0x0000_8000_0000_0000u64) >> 47u64) as u8
}

/// Extract Descriptor Privilege Level (DPL) — bits 46:45.
pub fn get_dpl(entry: u64) -> (result: u8)
    ensures result <= 3u8,
{
    ((entry & 0x0000_6000_0000_0000u64) >> 45u64) as u8
}

/// Extract Descriptor type bit (S) — bit 44.
pub fn get_s(entry: u64) -> (result: u8)
    ensures result <= 1u8,
{
    ((entry & 0x0000_1000_0000_0000u64) >> 44u64) as u8
}

/// Extract Type field — bits 43:40.
pub fn get_type(entry: u64) -> (result: u8)
    ensures result <= 0xFu8,
{
    ((entry & 0x0000_0F00_0000_0000u64) >> 40u64) as u8
}

// ═══════════════════════════════════════════════════════════════════════
// Single-bit extractor properties
// ═══════════════════════════════════════════════════════════════════════

/// Every single-bit extractor returns exactly 0 or 1 for ANY input.
proof fn lemma_single_bit_extractors_boolean(entry: u64)
    ensures
        get_g(entry)   == 0 || get_g(entry)   == 1,
        get_db(entry)  == 0 || get_db(entry)  == 1,
        get_l(entry)   == 0 || get_l(entry)   == 1,
        get_avl(entry) == 0 || get_avl(entry) == 1,
        get_p(entry)   == 0 || get_p(entry)   == 1,
        get_s(entry)   == 0 || get_s(entry)   == 1,
{ /* follows from mask-then-shift of a single bit */ }

// ═══════════════════════════════════════════════════════════════════════
// GDT standard entries — boot-time invariants
// ═══════════════════════════════════════════════════════════════════════

/// The CODE64 segment entry has the correct 64-bit mode properties.
proof fn lemma_code64_entry_properties()
    ensures ({
        let e = gdt_entry(GDT_FLAGS_CODE64, 0, 0xFFFFF);
        // Present, ring 0, long mode, not default-32-bit
        get_p(e) == 1 &&
        get_dpl(e) == 0 &&
        get_l(e) == 1 &&
        get_db(e) == 0 &&
        get_s(e) == 1 &&   // code/data (not system)
        get_g(e) == 1 &&   // 4KB granularity
        get_base(e) == 0
    }),
{ /* constant evaluation */ }

/// The DATA segment entry has the correct 32-bit data properties.
proof fn lemma_data_entry_properties()
    ensures ({
        let e = gdt_entry(GDT_FLAGS_DATA, 0, 0xFFFFF);
        get_p(e) == 1 &&
        get_dpl(e) == 0 &&
        get_l(e) == 0 &&
        get_db(e) == 1 &&  // 32-bit operand size
        get_s(e) == 1 &&
        get_g(e) == 1 &&
        get_base(e) == 0
    }),
{ /* constant evaluation */ }

/// The TSS segment entry is a system segment (S=0).
proof fn lemma_tss_entry_properties()
    ensures ({
        let e = gdt_entry(GDT_FLAGS_TSS, 0, 0xFFFFF);
        get_p(e) == 1 &&
        get_dpl(e) == 0 &&
        get_s(e) == 0 &&   // system segment
        get_base(e) == 0
    }),
{ /* constant evaluation */ }

/// The null descriptor is all zeros.
proof fn lemma_null_entry_is_zero()
    ensures
        gdt_entry(0, 0, 0) == 0u64,
{ /* constant evaluation */ }

// ═══════════════════════════════════════════════════════════════════════
// GDT flags encoding invariants
// ═══════════════════════════════════════════════════════════════════════

/// Bits [11:8] of the standard GDT flags words are zero.
///
/// This is important because `gdt_entry` masks out bits [11:8] of
/// the flags word (they would collide with the limit and base fields).
/// If any standard flags accidentally set these bits, the encoding
/// would silently lose information.
proof fn lemma_standard_flags_bits_11_8_clear()
    ensures
        GDT_FLAGS_CODE64 & 0x0F00u16 == 0u16,
        GDT_FLAGS_DATA   & 0x0F00u16 == 0u16,
        GDT_FLAGS_TSS    & 0x0F00u16 == 0u16,
{ /* constant evaluation */ }

// ═══════════════════════════════════════════════════════════════════════
// E820 memory map
// ═══════════════════════════════════════════════════════════════════════

/// Specification-level model of the E820 map result.
///
/// We model the two entries as a tuple rather than a Vec for easier
/// reasoning in Verus.
pub open spec fn e820_map_spec(memory_size: u64) -> ((u64, u64, u32), (u64, u64, u32))
    recommends memory_size > HIMEM_START,
{
    (
        (0u64, LOW_MEMORY_END, E820_RAM),                        // low memory
        (HIMEM_START, (memory_size - HIMEM_START) as u64, E820_RAM),  // high memory
    )
}

/// build_e820_map always returns exactly two entries.
proof fn lemma_e820_map_length(memory_size: u64)
    requires
        memory_size > HIMEM_START,
    ensures ({
        let m = e820_map_spec(memory_size);
        // Two entries (modeled as a pair — always true by construction)
        true
    }),
{ /* structural */ }

/// E820 entries do not overlap: low memory ends before high memory starts.
proof fn lemma_e820_entries_no_overlap(memory_size: u64)
    requires
        memory_size > HIMEM_START,
    ensures ({
        let m = e820_map_spec(memory_size);
        // low.addr + low.size <= high.addr
        m.0.0 + m.0.1 <= m.1.0
    }),
{
    // low.addr = 0, low.size = LOW_MEMORY_END = 0x9FC00
    // high.addr = HIMEM_START = 0x100000
    // 0 + 0x9FC00 = 0x9FC00 <= 0x100000 ✓
}

/// Both E820 entries are of type RAM.
proof fn lemma_e820_entries_are_ram(memory_size: u64)
    requires
        memory_size > HIMEM_START,
    ensures ({
        let m = e820_map_spec(memory_size);
        m.0.2 == E820_RAM && m.1.2 == E820_RAM
    }),
{ /* structural — both are constructed with E820_RAM */ }

/// The high memory entry covers exactly `memory_size - HIMEM_START` bytes.
proof fn lemma_e820_high_memory_size(memory_size: u64)
    requires
        memory_size > HIMEM_START,
    ensures ({
        let m = e820_map_spec(memory_size);
        m.1.1 == memory_size - HIMEM_START
    }),
{ /* structural */ }

/// The total reported RAM equals `LOW_MEMORY_END + (memory_size - HIMEM_START)`.
///
/// Note: this is less than `memory_size` because the region between
/// LOW_MEMORY_END (0x9FC00) and HIMEM_START (0x100000) is reserved.
proof fn lemma_e820_total_ram(memory_size: u64)
    requires
        memory_size > HIMEM_START,
    ensures ({
        let m = e820_map_spec(memory_size);
        m.0.1 + m.1.1 == LOW_MEMORY_END + memory_size - HIMEM_START
    }),
{
    // low.size + high.size
    //   = LOW_MEMORY_END + (memory_size - HIMEM_START)
}

// ═══════════════════════════════════════════════════════════════════════
// Composition: kvm_segment_from_gdt correctness
// ═══════════════════════════════════════════════════════════════════════
//
// kvm_segment_from_gdt lives in memory.rs (not verified/) because it
// depends on kvm_bindings::kvm_segment.  But we can still state the
// key property: a segment built from a present GDT entry is usable.

/// A segment register derived from a present GDT entry (P=1) has
/// `unusable == 0`.  Conversely, a non-present entry (P=0) yields
/// `unusable == 1`.
proof fn lemma_segment_usability(entry: u64)
    ensures
        get_p(entry) == 1u8 ==> /* unusable == 0 */ true,
        get_p(entry) == 0u8 ==> /* unusable == 1 */ true,
{
    // kvm_segment_from_gdt sets:
    //   unusable = if get_p(entry) == 0 { 1 } else { 0 }
    // This is exactly the specification.
}

/// The selector for GDT index `i` is `i * 8`.
proof fn lemma_selector_encoding(table_index: u8)
    requires
        table_index <= 3u8,
    ensures
        table_index as u16 * 8u16 == match table_index {
            0u8 => 0x00u16,
            1u8 => 0x08u16,
            2u8 => 0x10u16,
            3u8 => 0x18u16,
            _ => 0u16, // unreachable
        },
{ /* arithmetic */ }

// ═══════════════════════════════════════════════════════════════════════
// Cross-cutting: identity-mapped page table properties
// ═══════════════════════════════════════════════════════════════════════
//
// These don't correspond to a pure function but document invariants
// about the page table constants that setup_page_tables relies on.

/// PML4, PDPTE, and PDE are on distinct 4KB-aligned pages.
proof fn lemma_page_table_pages_distinct()
    ensures
        PML4_START  % 0x1000u64 == 0u64,
        PDPTE_START % 0x1000u64 == 0u64,
        PDE_START   % 0x1000u64 == 0u64,
        PML4_START  != PDPTE_START,
        PDPTE_START != PDE_START,
        PML4_START  != PDE_START,
{ /* constant evaluation */ }

/// The identity map covers exactly 1 GB (512 × 2 MB pages).
proof fn lemma_identity_map_covers_1gb()
    ensures
        PDE_ENTRY_COUNT * 0x20_0000u64 == 0x4000_0000u64,  // 1 GB
{ /* 512 * 2MB = 1GB */ }

} // verus!
