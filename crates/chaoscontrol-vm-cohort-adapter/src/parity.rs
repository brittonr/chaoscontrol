use std::collections::BTreeMap;
use std::sync::Arc;

use chaoscontrol_vmm::devices::block::DeterministicBlock;
use chaoscontrol_vmm::snapshot::{SnapshotMemory, PAGE_SIZE};
use serde::{Deserialize, Serialize};
use vm_cohort_kvm::{identify_bytes, ImmutableBase, SparseOverlay};

use crate::AdapterError;

const CORPUS_BYTES: usize = PAGE_SIZE * 2;
const WRITE_OFFSET: usize = PAGE_SIZE;
const WRITE_VALUE_A: u8 = 0x5a;
const WRITE_VALUE_B: u8 = 0xa5;
const WRITE_BYTES: usize = 4;
const MAXIMUM_DIRTY_PAGES: usize = 2;
const NON_CLAIMS: &[&str] = &[
    "behavioral parity does not prove either implementation correct",
    "normalized identities are not cross-format storage identities",
    "parity does not transfer fault, replay, evidence, or release authority",
];

/// One normalized legacy/shared parity row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityRow {
    /// Case label.
    pub case: String,
    /// Normalized legacy observation.
    pub legacy_observation: String,
    /// Normalized shared observation.
    pub shared_observation: String,
    /// Exact equality verdict.
    pub agrees: bool,
}

/// Complete bounded migration parity report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityReport {
    /// Every required row.
    pub rows: Vec<ParityRow>,
    /// Aggregate verdict.
    pub passed: bool,
    /// VM Cohort did not grant product authority.
    pub product_authority_granted: bool,
    /// Required claim boundaries.
    pub non_claims: Vec<String>,
}

/// Runs the bounded legacy/shared memory and disk migration corpus.
///
/// # Errors
///
/// Returns a bounded adapter error for identity, overlay, I/O, or mapping failure.
// r[impl chaoscontrol.vm_cohort.parity]
pub fn run_parity_corpus() -> Result<ParityReport, AdapterError> {
    let base = vec![0_u8; CORPUS_BYTES];
    let base_ref =
        identify_bytes(&base).map_err(|_| AdapterError::Admission("parity base identity"))?;
    let rows = vec![
        initial_disk_row(&base, &base_ref)?,
        write_and_restore_disk_row(&base, &base_ref)?,
        divergent_disk_row(&base, &base_ref)?,
        memory_overlay_row(&base, &base_ref)?,
        out_of_bounds_row(&base, &base_ref)?,
    ];
    let is_passed = rows.iter().all(|row| row.agrees);
    Ok(ParityReport {
        rows,
        passed: is_passed,
        product_authority_granted: false,
        non_claims: NON_CLAIMS.iter().map(ToString::to_string).collect(),
    })
}

/// Validates report completeness, equality, and claim boundaries.
#[must_use]
pub fn validate_parity_report(report: &ParityReport) -> bool {
    const REQUIRED_ROWS: usize = 5;
    report.rows.len() == REQUIRED_ROWS
        && report.passed
        && report.rows.iter().all(|row| row.agrees)
        && !report.product_authority_granted
        && report.non_claims == NON_CLAIMS
}

fn initial_disk_row(
    base: &[u8],
    base_ref: &vm_cohort_core::ResourceRef,
) -> Result<ParityRow, AdapterError> {
    let mut legacy = DeterministicBlock::from_image(base.to_vec());
    let shared = SparseOverlay::new(
        base_ref.clone(),
        Arc::<[u8]>::from(base),
        PAGE_SIZE,
        MAXIMUM_DIRTY_PAGES,
    )
    .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let legacy_bytes = read_legacy(&mut legacy, 0)?;
    let shared_bytes = read_shared(&shared, 0)?;
    Ok(row("initial-read", &legacy_bytes, &shared_bytes))
}

fn write_and_restore_disk_row(
    base: &[u8],
    base_ref: &vm_cohort_core::ResourceRef,
) -> Result<ParityRow, AdapterError> {
    let payload = [WRITE_VALUE_A; WRITE_BYTES];
    let mut legacy = DeterministicBlock::from_image(base.to_vec());
    legacy
        .write(
            u64::try_from(WRITE_OFFSET).map_err(|_| AdapterError::Admission("write offset"))?,
            &payload,
        )
        .map_err(|_| AdapterError::Admission("legacy write"))?;
    let legacy_snapshot = legacy.snapshot();
    let mut legacy_restored = DeterministicBlock::restore(&legacy_snapshot);
    let mut shared = SparseOverlay::new(
        base_ref.clone(),
        Arc::<[u8]>::from(base),
        PAGE_SIZE,
        MAXIMUM_DIRTY_PAGES,
    )
    .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    shared
        .write(WRITE_OFFSET, &payload)
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let shared_restored = shared.clone();
    let legacy_bytes = read_legacy(&mut legacy_restored, WRITE_OFFSET)?;
    let shared_bytes = read_shared(&shared_restored, WRITE_OFFSET)?;
    Ok(row("write-snapshot-restore", &legacy_bytes, &shared_bytes))
}

fn divergent_disk_row(
    base: &[u8],
    base_ref: &vm_cohort_core::ResourceRef,
) -> Result<ParityRow, AdapterError> {
    let mut legacy_a = DeterministicBlock::from_image(base.to_vec());
    let mut legacy_b = DeterministicBlock::from_image(base.to_vec());
    let mut shared_a = SparseOverlay::new(
        base_ref.clone(),
        Arc::<[u8]>::from(base),
        PAGE_SIZE,
        MAXIMUM_DIRTY_PAGES,
    )
    .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let mut shared_b = shared_a.clone();
    write_legacy(&mut legacy_a, WRITE_VALUE_A)?;
    write_legacy(&mut legacy_b, WRITE_VALUE_B)?;
    shared_a
        .write(WRITE_OFFSET, &[WRITE_VALUE_A; WRITE_BYTES])
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    shared_b
        .write(WRITE_OFFSET, &[WRITE_VALUE_B; WRITE_BYTES])
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let legacy = [
        read_legacy(&mut legacy_a, WRITE_OFFSET)?,
        read_legacy(&mut legacy_b, WRITE_OFFSET)?,
    ]
    .concat();
    let shared = [
        read_shared(&shared_a, WRITE_OFFSET)?,
        read_shared(&shared_b, WRITE_OFFSET)?,
    ]
    .concat();
    Ok(row("clone-divergence", &legacy, &shared))
}

fn memory_overlay_row(
    base: &[u8],
    base_ref: &vm_cohort_core::ResourceRef,
) -> Result<ParityRow, AdapterError> {
    let mut page = Box::new([0_u8; PAGE_SIZE]);
    page[..WRITE_BYTES].fill(WRITE_VALUE_A);
    let legacy = SnapshotMemory::Overlay {
        base: Arc::new(base.to_vec()),
        dirty_pages: BTreeMap::from([(1, page)]),
    }
    .materialize();
    let immutable = ImmutableBase::create("chaos-parity-memory", base, base_ref)
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let mut shared = immutable
        .map_private()
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    shared
        .write(WRITE_OFFSET, &[WRITE_VALUE_A; WRITE_BYTES])
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let mut shared_bytes = vec![0_u8; CORPUS_BYTES];
    shared
        .read(0, &mut shared_bytes)
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    Ok(row("memory-overlay", &legacy, &shared_bytes))
}

fn out_of_bounds_row(
    base: &[u8],
    base_ref: &vm_cohort_core::ResourceRef,
) -> Result<ParityRow, AdapterError> {
    let mut legacy = DeterministicBlock::from_image(base.to_vec());
    let shared = SparseOverlay::new(
        base_ref.clone(),
        Arc::<[u8]>::from(base),
        PAGE_SIZE,
        MAXIMUM_DIRTY_PAGES,
    )
    .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    let is_legacy_error = legacy
        .read(
            u64::try_from(base.len()).map_err(|_| AdapterError::Admission("base length"))?,
            &mut [0_u8; 1],
        )
        .is_err();
    let is_shared_error = shared.read(base.len(), &mut [0_u8; 1]).is_err();
    Ok(row(
        "out-of-bounds-error",
        &[u8::from(is_legacy_error)],
        &[u8::from(is_shared_error)],
    ))
}

fn write_legacy(block: &mut DeterministicBlock, value: u8) -> Result<(), AdapterError> {
    block
        .write(
            u64::try_from(WRITE_OFFSET).map_err(|_| AdapterError::Admission("write offset"))?,
            &[value; WRITE_BYTES],
        )
        .map_err(|_| AdapterError::Admission("legacy write"))?;
    Ok(())
}

fn read_legacy(block: &mut DeterministicBlock, offset: usize) -> Result<Vec<u8>, AdapterError> {
    let mut output = vec![0_u8; WRITE_BYTES];
    block
        .read(
            u64::try_from(offset).map_err(|_| AdapterError::Admission("read offset"))?,
            &mut output,
        )
        .map_err(|_| AdapterError::Admission("legacy read"))?;
    Ok(output)
}

fn read_shared(block: &SparseOverlay, offset: usize) -> Result<Vec<u8>, AdapterError> {
    let mut output = vec![0_u8; WRITE_BYTES];
    block
        .read(offset, &mut output)
        .map_err(|error| AdapterError::Kvm(error.to_string()))?;
    Ok(output)
}

fn row(case: &str, legacy: &[u8], shared: &[u8]) -> ParityRow {
    ParityRow {
        case: case.to_string(),
        legacy_observation: observation(legacy),
        shared_observation: observation(shared),
        agrees: legacy == shared,
    }
}

fn observation(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
