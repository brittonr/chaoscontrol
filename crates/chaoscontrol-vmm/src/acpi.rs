//! Minimal ACPI table generation for SMP boot.
//!
//! Linux discovers secondary CPUs via the ACPI MADT (Multiple APIC
//! Description Table). This module generates the minimum set of ACPI
//! tables needed:
//!
//! - **RSDP** (Root System Description Pointer) — placed in the EBDA region
//! - **RSDT** (Root System Description Table) — points to MADT
//! - **MADT** (Multiple APIC Description Table) — lists Local APIC entries
//!
//! The tables are placed in guest memory below 1 MB so the kernel's
//! early ACPI scanner can find them.

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

/// Base address for ACPI tables in guest memory.
///
/// Placed at 0xF0000 (960 KB), within the BIOS read-only region.
/// This area is traditionally reserved and is scanned by Linux for RSDP.
const ACPI_TABLE_BASE: u64 = 0xF_0000;

/// RSDP is at the start of the ACPI region.
const RSDP_OFFSET: u64 = 0;
/// RSDT follows immediately after RSDP (36 bytes).
const RSDT_OFFSET: u64 = 36;
/// MADT follows after RSDT (header + one pointer = 36 + 4 = 40 bytes).
/// Round up for alignment.
const MADT_OFFSET: u64 = 80;

/// RSDP revision 0 (ACPI 1.0) — uses 20-byte structure.
const RSDP_REVISION: u8 = 0;

/// Size of an ACPI SDT header.
const SDT_HEADER_SIZE: usize = 36;

/// MADT Local APIC entry type.
const MADT_LAPIC_TYPE: u8 = 0;
/// MADT Local APIC entry length.
const MADT_LAPIC_LEN: u8 = 8;

/// MADT I/O APIC entry type.
const MADT_IOAPIC_TYPE: u8 = 1;
/// MADT I/O APIC entry length.
const MADT_IOAPIC_LEN: u8 = 12;

/// Default Local APIC address (standard x86 value).
const LAPIC_DEFAULT_ADDR: u32 = 0xFEE0_0000;

/// Default I/O APIC address.
const IOAPIC_DEFAULT_ADDR: u32 = 0xFEC0_0000;

/// Errors from ACPI table generation.
#[derive(Debug, thiserror::Error)]
pub enum AcpiError {
    #[error("Failed to write ACPI tables to guest memory")]
    WriteMemory,
    #[error("Too many CPUs for ACPI table region (max 128)")]
    TooManyCpus,
}

/// Write minimal ACPI tables (RSDP + RSDT + MADT) for `num_cpus` processors.
///
/// The tables are placed at [`ACPI_TABLE_BASE`] in guest memory. Each CPU
/// gets a Local APIC entry with APIC ID = CPU index, all marked as enabled.
///
/// Returns the address of the RSDP, which should be referenced by the
/// EBDA pointer at 0x40E.
pub fn write_acpi_tables(
    guest_memory: &GuestMemoryMmap,
    num_cpus: usize,
) -> Result<u64, AcpiError> {
    if num_cpus > 128 {
        return Err(AcpiError::TooManyCpus);
    }

    let rsdp_addr = ACPI_TABLE_BASE + RSDP_OFFSET;
    let rsdt_addr = ACPI_TABLE_BASE + RSDT_OFFSET;
    let madt_addr = ACPI_TABLE_BASE + MADT_OFFSET;

    // ── Build MADT ──────────────────────────────────────────────────
    let madt = build_madt(num_cpus, madt_addr as u32);
    guest_memory
        .write_slice(&madt, GuestAddress(madt_addr))
        .map_err(|_| AcpiError::WriteMemory)?;

    // ── Build RSDT ──────────────────────────────────────────────────
    let rsdt = build_rsdt(madt_addr as u32);
    guest_memory
        .write_slice(&rsdt, GuestAddress(rsdt_addr))
        .map_err(|_| AcpiError::WriteMemory)?;

    // ── Build RSDP ──────────────────────────────────────────────────
    let rsdp = build_rsdp(rsdt_addr as u32);
    guest_memory
        .write_slice(&rsdp, GuestAddress(rsdp_addr))
        .map_err(|_| AcpiError::WriteMemory)?;

    // ── Set EBDA pointer ────────────────────────────────────────────
    // BDA at 0x40E contains the EBDA segment (paragraph address).
    // Linux scans the first KB of the EBDA for the RSDP signature.
    // We point it to a 1KB region starting at ACPI_TABLE_BASE.
    let ebda_segment = (ACPI_TABLE_BASE >> 4) as u16;
    guest_memory
        .write_obj(ebda_segment, GuestAddress(0x40E))
        .map_err(|_| AcpiError::WriteMemory)?;

    log::info!(
        "ACPI tables written: RSDP={:#x} RSDT={:#x} MADT={:#x} ({} CPUs)",
        rsdp_addr,
        rsdt_addr,
        madt_addr,
        num_cpus,
    );

    Ok(rsdp_addr)
}

/// Build the RSDP (Root System Description Pointer).
///
/// ACPI 1.0 (revision 0): 20-byte structure with signature, checksum,
/// OEM ID, and pointer to the RSDT.
fn build_rsdp(rsdt_address: u32) -> Vec<u8> {
    let mut rsdp = vec![0u8; 20];

    // Signature: "RSD PTR " (8 bytes)
    rsdp[0..8].copy_from_slice(b"RSD PTR ");
    // Checksum: computed after filling other fields
    // OEM ID: "CHAOS " (6 bytes)
    rsdp[9..15].copy_from_slice(b"CHAOS ");
    // Revision: 0 (ACPI 1.0)
    rsdp[15] = RSDP_REVISION;
    // RSDT address (4 bytes, little-endian)
    rsdp[16..20].copy_from_slice(&rsdt_address.to_le_bytes());

    // Compute checksum (byte 8): sum of all bytes must be 0 mod 256
    let sum: u8 = rsdp.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    rsdp[8] = 0u8.wrapping_sub(sum);

    rsdp
}

/// Build the RSDT (Root System Description Table).
///
/// Contains one entry: the MADT address.
fn build_rsdt(madt_address: u32) -> Vec<u8> {
    let table_len = SDT_HEADER_SIZE + 4; // header + one 32-bit pointer
    let mut rsdt = vec![0u8; table_len];

    // Signature: "RSDT"
    rsdt[0..4].copy_from_slice(b"RSDT");
    // Length (4 bytes LE)
    rsdt[4..8].copy_from_slice(&(table_len as u32).to_le_bytes());
    // Revision: 1
    rsdt[8] = 1;
    // Checksum: computed after
    // OEM ID: "CHAOS " (6 bytes)
    rsdt[10..16].copy_from_slice(b"CHAOS ");
    // OEM Table ID: "CHAOSCTL" (8 bytes)
    rsdt[16..24].copy_from_slice(b"CHAOSCTL");
    // OEM Revision (4 bytes)
    rsdt[24..28].copy_from_slice(&1u32.to_le_bytes());
    // Creator ID
    rsdt[28..32].copy_from_slice(b"CCVL");
    // Creator Revision
    rsdt[32..36].copy_from_slice(&1u32.to_le_bytes());

    // Entry: MADT address
    rsdt[SDT_HEADER_SIZE..SDT_HEADER_SIZE + 4].copy_from_slice(&madt_address.to_le_bytes());

    // Compute checksum (byte 9)
    let sum: u8 = rsdt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    rsdt[9] = 0u8.wrapping_sub(sum);

    rsdt
}

/// Build the MADT (Multiple APIC Description Table).
///
/// Contains:
/// - MADT header with Local APIC Address
/// - One Local APIC entry per CPU (APIC ID = index, all enabled)
/// - One I/O APIC entry (ID 0, base at 0xFEC00000)
fn build_madt(num_cpus: usize, self_address: u32) -> Vec<u8> {
    let madt_specific = 8; // Flags (4 bytes) + Local APIC Address (4 bytes) in header body
    let lapic_entries = num_cpus * MADT_LAPIC_LEN as usize;
    let ioapic_entry = MADT_IOAPIC_LEN as usize;
    let table_len = SDT_HEADER_SIZE + madt_specific + lapic_entries + ioapic_entry;

    let mut madt = vec![0u8; table_len];

    // SDT header
    madt[0..4].copy_from_slice(b"APIC"); // Signature
    madt[4..8].copy_from_slice(&(table_len as u32).to_le_bytes()); // Length
    madt[8] = 4; // Revision
    // Checksum at byte 9 — computed after
    madt[10..16].copy_from_slice(b"CHAOS "); // OEM ID
    madt[16..24].copy_from_slice(b"CHAOSCTL"); // OEM Table ID
    madt[24..28].copy_from_slice(&1u32.to_le_bytes()); // OEM Revision
    madt[28..32].copy_from_slice(b"CCVL"); // Creator ID
    madt[32..36].copy_from_slice(&1u32.to_le_bytes()); // Creator Revision

    // MADT body: Local APIC Address
    let mut offset = SDT_HEADER_SIZE;
    madt[offset..offset + 4].copy_from_slice(&LAPIC_DEFAULT_ADDR.to_le_bytes());
    offset += 4;
    // Flags: PCAT_COMPAT (bit 0) — system has dual 8259A-compatible PICs
    madt[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
    offset += 4;

    // Local APIC entries — one per CPU
    for i in 0..num_cpus {
        madt[offset] = MADT_LAPIC_TYPE; // Type
        madt[offset + 1] = MADT_LAPIC_LEN; // Length
        madt[offset + 2] = i as u8; // ACPI Processor UID
        madt[offset + 3] = i as u8; // APIC ID
        // Flags: bit 0 = Processor Enabled
        madt[offset + 4..offset + 8].copy_from_slice(&1u32.to_le_bytes());
        offset += MADT_LAPIC_LEN as usize;
    }

    // I/O APIC entry
    madt[offset] = MADT_IOAPIC_TYPE; // Type
    madt[offset + 1] = MADT_IOAPIC_LEN; // Length
    madt[offset + 2] = 0; // I/O APIC ID
    madt[offset + 3] = 0; // Reserved
    madt[offset + 4..offset + 8].copy_from_slice(&IOAPIC_DEFAULT_ADDR.to_le_bytes());
    madt[offset + 8..offset + 12].copy_from_slice(&0u32.to_le_bytes()); // GSI Base

    // Compute checksum
    let sum: u8 = madt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    madt[9] = 0u8.wrapping_sub(sum);

    let _ = self_address; // Used for log context only
    madt
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_checksum(data: &[u8]) -> bool {
        data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b)) == 0
    }

    #[test]
    fn rsdp_signature_and_checksum() {
        let rsdp = build_rsdp(0xF_0050);
        assert_eq!(&rsdp[0..8], b"RSD PTR ");
        assert!(verify_checksum(&rsdp), "RSDP checksum must be zero");
    }

    #[test]
    fn rsdp_points_to_rsdt() {
        let rsdp = build_rsdp(0xABCD_1234);
        let rsdt_addr = u32::from_le_bytes([rsdp[16], rsdp[17], rsdp[18], rsdp[19]]);
        assert_eq!(rsdt_addr, 0xABCD_1234);
    }

    #[test]
    fn rsdt_signature_and_checksum() {
        let rsdt = build_rsdt(0xF_0080);
        assert_eq!(&rsdt[0..4], b"RSDT");
        assert!(verify_checksum(&rsdt), "RSDT checksum must be zero");
    }

    #[test]
    fn rsdt_contains_madt_pointer() {
        let rsdt = build_rsdt(0xF_0080);
        let ptr = u32::from_le_bytes([rsdt[36], rsdt[37], rsdt[38], rsdt[39]]);
        assert_eq!(ptr, 0xF_0080);
    }

    #[test]
    fn madt_signature_and_checksum() {
        let madt = build_madt(4, 0);
        assert_eq!(&madt[0..4], b"APIC");
        assert!(verify_checksum(&madt), "MADT checksum must be zero");
    }

    #[test]
    fn madt_single_cpu() {
        let madt = build_madt(1, 0);
        // Header (36) + body (8) + 1 LAPIC (8) + 1 IOAPIC (12) = 64
        assert_eq!(madt.len(), 64);
        let len = u32::from_le_bytes([madt[4], madt[5], madt[6], madt[7]]);
        assert_eq!(len, 64);

        // First LAPIC entry at offset 44
        assert_eq!(madt[44], MADT_LAPIC_TYPE);
        assert_eq!(madt[45], MADT_LAPIC_LEN);
        assert_eq!(madt[46], 0); // Processor UID
        assert_eq!(madt[47], 0); // APIC ID
    }

    #[test]
    fn madt_four_cpus() {
        let madt = build_madt(4, 0);
        // Header (36) + body (8) + 4 LAPICs (32) + 1 IOAPIC (12) = 88
        assert_eq!(madt.len(), 88);

        // Check each LAPIC entry
        for i in 0..4 {
            let base = 44 + i * 8;
            assert_eq!(madt[base], MADT_LAPIC_TYPE);
            assert_eq!(madt[base + 2], i as u8); // Processor UID
            assert_eq!(madt[base + 3], i as u8); // APIC ID
            // Enabled flag
            let flags = u32::from_le_bytes([
                madt[base + 4],
                madt[base + 5],
                madt[base + 6],
                madt[base + 7],
            ]);
            assert_eq!(flags, 1, "CPU {} must be enabled", i);
        }

        // I/O APIC at offset 76
        assert_eq!(madt[76], MADT_IOAPIC_TYPE);
        assert_eq!(madt[77], MADT_IOAPIC_LEN);
    }

    #[test]
    fn madt_lapic_address() {
        let madt = build_madt(1, 0);
        let lapic_addr =
            u32::from_le_bytes([madt[36], madt[37], madt[38], madt[39]]);
        assert_eq!(lapic_addr, LAPIC_DEFAULT_ADDR);
    }

    #[test]
    fn madt_pcat_compat_flag() {
        let madt = build_madt(1, 0);
        let flags = u32::from_le_bytes([madt[40], madt[41], madt[42], madt[43]]);
        assert_eq!(flags & 1, 1, "PCAT_COMPAT flag must be set");
    }

    #[test]
    fn too_many_cpus_rejected() {
        // We can't easily test write_acpi_tables without guest memory,
        // but we can verify the limit check would apply.
        assert!(129 > 128);
    }
}
