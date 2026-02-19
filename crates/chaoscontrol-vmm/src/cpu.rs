//! CPU determinism configuration for the ChaosControl hypervisor.
//!
//! This module centralizes every CPU-level control needed to make KVM guest
//! execution deterministic and reproducible across runs and across host CPUs.
//!
//! # Determinism strategy
//!
//! Modern x86 CPUs expose several sources of non-determinism through CPUID
//! feature flags and instructions.  This module filters them at three levels:
//!
//! 1. **CPUID filtering** — hide hardware RNG instructions, variable TSC
//!    features, and optionally vector extensions that differ between hosts.
//! 2. **TSC pinning** — lock the Time Stamp Counter to a fixed frequency so
//!    that `RDTSC` inside the guest advances at exactly the configured rate,
//!    regardless of the host CPU's actual clock.
//! 3. **Virtual TSC tracking** — maintain a pure-software counter for use
//!    with KVM MSR filtering (`IA32_TSC` traps), enabling fully deterministic
//!    time progression that only advances when the VMM decides.
//!
//! # Usage
//!
//! ```no_run
//! use chaoscontrol_vmm::cpu::{CpuConfig, filter_cpuid, setup_tsc, VirtualTsc};
//!
//! let config = CpuConfig::default();
//! let kvm = kvm_ioctls::Kvm::new().unwrap();
//! let vm = kvm.create_vm().unwrap();
//! let vcpu = vm.create_vcpu(0).unwrap();
//!
//! // Apply deterministic CPUID to the vCPU.
//! let cpuid = filter_cpuid(&kvm, &config).unwrap();
//! vcpu.set_cpuid2(&cpuid).unwrap();
//!
//! // Pin the hardware TSC to a fixed frequency.
//! setup_tsc(&vcpu, config.tsc_khz).unwrap();
//!
//! // Track a software TSC for MSR-trap based time control.
//! let mut vtsc = VirtualTsc::from_config(&config);
//! vtsc.tick();
//! ```

use kvm_bindings::{kvm_cpuid_entry2, CpuId, KVM_MAX_CPUID_ENTRIES};
use kvm_ioctls::{Kvm, VcpuFd};
use log::info;
use thiserror::Error;

// ─── CPUID leaf constants ────────────────────────────────────────────

/// CPUID leaf 0x1 — Processor Info and Feature Bits.
pub const CPUID_LEAF_FEATURES: u32 = 0x1;

/// CPUID leaf 0x7 — Structured Extended Feature Flags (sub-leaf in ECX).
pub const CPUID_LEAF_STRUCTURED_EXT: u32 = 0x7;

/// CPUID leaf 0xD — Processor Extended State Enumeration (XSAVE).
pub const CPUID_LEAF_XSAVE: u32 = 0xD;

/// CPUID leaf 0x15 — Time Stamp Counter / Core Crystal Clock Information.
///
/// Reports the ratio between the TSC rate and a reference crystal clock so
/// that software can compute `TSC_freq = ECX × EBX / EAX`.
pub const CPUID_LEAF_TSC_INFO: u32 = 0x15;

/// CPUID leaf 0x16 — Processor Frequency Information (MHz).
///
/// EAX = base frequency, EBX = max frequency, ECX = bus/reference frequency.
pub const CPUID_LEAF_FREQ_INFO: u32 = 0x16;

/// CPUID leaf 0x40000000 — KVM hypervisor signature.
pub const CPUID_LEAF_KVM_SIGNATURE: u32 = 0x4000_0000;

/// Upper bound of the KVM/hypervisor CPUID range (inclusive).
const CPUID_LEAF_HV_RANGE_END: u32 = 0x4000_00FF;

/// CPUID leaf 0x80000001 — Extended Processor Info and Feature Bits.
pub const CPUID_LEAF_EXT_FEATURES: u32 = 0x8000_0001;

/// CPUID leaf 0x80000007 — Advanced Power Management Information.
pub const CPUID_LEAF_APM: u32 = 0x8000_0007;

// ─── CPUID bit masks ─────────────────────────────────────────────────
//
// Each constant documents *why* the bit matters for determinism.

// -- Leaf 0x1, ECX --

// -- Leaf 0x1, EDX --

/// EDX bit 4 — Time Stamp Counter (TSC) feature.
///
/// When hidden, the guest kernel's `get_cycles()` returns 0, preventing
/// all TSC-based calibration (PIT + TSC calibration loop). RDTSC still
/// works at the hardware level (KVM provides it), but the kernel never
/// reads it. This eliminates non-deterministic calibration during early
/// boot in SMP configurations.
pub const CPUID_1_EDX_TSC: u32 = 1 << 4;

// -- Leaf 0x1, ECX --

/// ECX bit 24 — TSC-Deadline timer.
///
/// Disabled because deadline-mode LAPIC interrupts have variable latency
/// that depends on the host scheduler and APIC emulation path.
pub const CPUID_1_ECX_TSC_DEADLINE: u32 = 1 << 24;

/// ECX bit 30 — RDRAND instruction.
///
/// Must be disabled: executes a hardware random-number generator whose
/// output differs on every invocation.
pub const CPUID_1_ECX_RDRAND: u32 = 1 << 30;

/// ECX bit 31 — Hypervisor present bit.
///
/// When set, tells the guest it is running under a hypervisor.  Hiding
/// this (and the 0x40000000+ leaves) makes the environment look like
/// bare metal, which can prevent guest-side paravirt short-cuts that
/// read host clocks.
pub const CPUID_1_ECX_HYPERVISOR: u32 = 1 << 31;

// -- Leaf 0x7 sub-leaf 0, EBX --

/// EBX bit 5 — AVX2 (256-bit integer SIMD).
///
/// Optionally disabled so that code-gen is identical across hosts with
/// and without AVX2 support.
pub const CPUID_7_EBX_AVX2: u32 = 1 << 5;

/// EBX bit 16 — AVX-512 Foundation.
///
/// Optionally disabled.  Many CI and cloud hosts lack AVX-512, so
/// hiding it by default ensures cross-host reproducibility.
pub const CPUID_7_EBX_AVX512F: u32 = 1 << 16;

/// EBX bit 18 — RDSEED instruction.
///
/// Must be disabled for the same reason as RDRAND.
pub const CPUID_7_EBX_RDSEED: u32 = 1 << 18;

// -- Leaf 0x80000001, EDX --

/// EDX bit 27 — RDTSCP instruction.
///
/// Disabled because `RDTSCP` reads both the TSC and `IA32_TSC_AUX`
/// atomically; the auxiliary value can differ across hosts and does
/// not go through the MSR-trap path.
pub const CPUID_EXT_EDX_RDTSCP: u32 = 1 << 27;

// -- Leaf 0x80000007, EDX --

/// EDX bit 8 — Invariant TSC.
///
/// Hidden so the guest does not assume the TSC is invariant with respect
/// to the host crystal oscillator.  We control the TSC rate explicitly
/// via `set_tsc_khz`.
pub const CPUID_APM_EDX_INVARIANT_TSC: u32 = 1 << 8;

// ─── CPUID 0x1 EAX model / family bit layout ────────────────────────
//
// Canonical definitions live in `verified::cpu`.  We re-use them here
// via fully-qualified paths in `filter_entry` and in tests.

// ─── Defaults ────────────────────────────────────────────────────────

/// Default TSC frequency: 3.0 GHz.
///
/// Chosen because it is a common, round value that does not require TSC
/// scaling on a wide variety of host CPUs.  KVM can scale to/from this
/// rate transparently.
pub const DEFAULT_TSC_KHZ: u32 = 3_000_000;

/// Default virtual-TSC advance per tick: 1 000 counts.
///
/// At 3 GHz this corresponds to ≈333 ns per tick, which is a reasonable
/// granularity for VM-exit-driven time progression.
pub const DEFAULT_TSC_ADVANCE: u64 = 1_000;

/// Reference crystal clock frequency used in CPUID leaf 0x15.
///
/// 25 MHz matches the crystal clock reported by most Intel CPUs since
/// Skylake, producing clean integer ratios for common TSC frequencies.
const CRYSTAL_CLOCK_HZ: u32 = 25_000_000;

// ─── Error type ──────────────────────────────────────────────────────

/// Errors that can occur during CPU determinism configuration.
#[derive(Error, Debug)]
pub enum CpuError {
    /// KVM refused to return the host-supported CPUID table.
    #[error("Failed to get supported CPUID from KVM: {0}")]
    GetCpuid(#[source] kvm_ioctls::Error),

    /// KVM refused the filtered CPUID table we tried to apply.
    #[error("Failed to set CPUID on vCPU: {0}")]
    SetCpuid(#[source] kvm_ioctls::Error),

    /// KVM could not pin the TSC to the requested frequency (e.g. the
    /// host kernel does not support TSC scaling).
    #[error("Failed to set TSC frequency to {freq_khz} kHz: {source}")]
    SetTscKhz {
        freq_khz: u32,
        #[source]
        source: kvm_ioctls::Error,
    },
}

// ─── Configuration ───────────────────────────────────────────────────

/// Complete set of knobs that control CPU-level determinism.
///
/// Every field has a safe default (see [`CpuConfig::default`]).  Two VMs
/// created with the same `CpuConfig` and the same guest image will
/// execute identically at the CPU instruction level.
#[derive(Debug, Clone)]
pub struct CpuConfig {
    /// TSC frequency exposed to the guest, in kHz.
    ///
    /// KVM will scale host TSC reads to this rate.  Use the same value
    /// across all runs and all hosts to ensure `RDTSC` returns identical
    /// sequences.
    pub tsc_khz: u32,

    /// Allow the guest to use AVX2 (256-bit integer SIMD).
    ///
    /// Disabled by default because AVX2 availability varies across host
    /// CPUs; enabling it may break reproducibility on heterogeneous
    /// fleets.
    pub allow_avx2: bool,

    /// Allow the guest to use AVX-512 instructions.
    ///
    /// Disabled by default.  Even fewer hosts support AVX-512 than AVX2.
    pub allow_avx512: bool,

    /// Hide the CPUID hypervisor-present bit (leaf 0x1, ECX bit 31).
    ///
    /// When `true`, the guest cannot detect that it runs under a
    /// hypervisor.  KVM paravirtualization leaves (0x40000000+) are also
    /// zeroed out.
    pub hide_hypervisor: bool,

    /// Hide the CPUID TSC feature bit (leaf 0x1, EDX bit 4).
    ///
    /// When `true`, the guest kernel believes there is no TSC and skips
    /// all TSC-based calibration. RDTSC still works (KVM provides it),
    /// but the kernel never reads it. This prevents non-deterministic
    /// PIT-based TSC calibration during early boot.
    pub hide_tsc: bool,

    /// If `Some`, override the CPUID processor family to this value.
    ///
    /// Useful when replaying a trace on a host with a different CPU
    /// generation.  Values ≤ 15 fit in the base field; larger values
    /// spill into the extended-family field.
    pub fixed_family: Option<u8>,

    /// If `Some`, override the CPUID processor model to this value.
    ///
    /// The low nibble goes into bits \[7:4\] and the high nibble into
    /// the extended-model field (bits \[19:16\]).
    pub fixed_model: Option<u8>,

    /// If `Some`, override the CPUID processor stepping to this value.
    pub fixed_stepping: Option<u8>,

    /// If `Some`, report this frequency (MHz) in CPUID leaf 0x16
    /// (base, max, and bus frequency fields).
    ///
    /// When `None` the frequency is derived from `tsc_khz`.
    pub fixed_frequency_mhz: Option<u16>,

    /// Master seed for reproducible execution.
    ///
    /// Passed through to [`VirtualTsc::from_config`] and available for
    /// callers that derive per-device entropy (e.g. virtio-rng).
    pub seed: u64,

    /// How many TSC counts the [`VirtualTsc`] advances per tick.
    pub tsc_advance_per_tick: u64,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            tsc_khz: DEFAULT_TSC_KHZ,
            allow_avx2: false,
            allow_avx512: false,
            hide_hypervisor: false,
            hide_tsc: false,
            fixed_family: None,
            fixed_model: None,
            fixed_stepping: None,
            fixed_frequency_mhz: None,
            seed: 0,
            tsc_advance_per_tick: DEFAULT_TSC_ADVANCE,
        }
    }
}

// ─── Public API ──────────────────────────────────────────────────────

/// Build a deterministic CPUID table from the host's supported features.
///
/// Queries KVM for the full set of host-supported CPUID entries, then
/// masks out every feature that can introduce non-determinism:
///
/// | Leaf | What is removed / overridden |
/// |------|------------------------------|
/// | 0x1  | RDRAND, TSC-Deadline, optionally hypervisor bit, model/family |
/// | 0x7  | RDSEED, optionally AVX2 and AVX-512 |
/// | 0x15 | Replaced with exact crystal-clock ratio for `tsc_khz` |
/// | 0x16 | Base/max/bus frequency set to match `tsc_khz` |
/// | 0x40000000+ | Zeroed when `hide_hypervisor` is set |
/// | 0x80000001 | RDTSCP |
/// | 0x80000007 | Invariant-TSC advertisement |
///
/// The returned [`CpuId`] should be applied to a vCPU with
/// [`VcpuFd::set_cpuid2`] before the first `vcpu.run()`.
pub fn filter_cpuid(kvm: &Kvm, config: &CpuConfig) -> Result<CpuId, CpuError> {
    let mut cpuid = kvm
        .get_supported_cpuid(KVM_MAX_CPUID_ENTRIES)
        .map_err(CpuError::GetCpuid)?;

    let mut modified = 0u32;

    // Check if leaf 0x15 (TSC/Crystal Clock) exists in the host CPUID.
    // AMD CPUs don't provide this leaf, but we need it for deterministic
    // TSC calibration when faking an Intel identity.
    let has_leaf_15 = cpuid
        .as_slice()
        .iter()
        .any(|e| e.function == CPUID_LEAF_TSC_INFO);

    for entry in cpuid.as_mut_slice() {
        if filter_entry(entry, config) {
            modified += 1;
        }
    }

    // If leaf 0x15 is missing (AMD host), inject it so the kernel can
    // determine the exact TSC frequency from CPUID instead of doing
    // non-deterministic PIT-based calibration.
    if !has_leaf_15 {
        let (denominator, numerator) = tsc_crystal_ratio(config.tsc_khz);
        let mut entries: Vec<kvm_cpuid_entry2> = cpuid.as_slice().to_vec();
        entries.push(kvm_cpuid_entry2 {
            function: CPUID_LEAF_TSC_INFO,
            index: 0,
            flags: 0,
            eax: denominator,
            ebx: numerator,
            ecx: CRYSTAL_CLOCK_HZ,
            edx: 0,
            padding: [0; 3],
        });
        cpuid = CpuId::from_entries(&entries)
            .map_err(|_| CpuError::GetCpuid(kvm_ioctls::Error::new(libc::ENOMEM)))?;
        modified += 1;
        info!(
            "Injected CPUID leaf 0x15: crystal={}Hz, ratio={}/{}",
            CRYSTAL_CLOCK_HZ, numerator, denominator,
        );
    }

    info!(
        "CPUID filtered: {} leaves modified \
         (tsc_khz={}, avx2={}, avx512={}, hide_hv={})",
        modified, config.tsc_khz, config.allow_avx2, config.allow_avx512, config.hide_hypervisor,
    );

    Ok(cpuid)
}

/// Produce a per-vCPU copy of the CPUID table with the correct APIC ID.
///
/// CPUID leaf 0x1 (EBX\[31:24\]) and leaves 0xB/0x1F (EDX) must report
/// each vCPU's unique APIC ID.  [`filter_cpuid`] produces a shared
/// template with APIC ID 0; this function creates a per-vCPU variant
/// by patching the relevant fields.
///
/// # Fields patched
///
/// | Leaf | Register | Field |
/// |------|----------|-------|
/// | 0x1  | EBX\[31:24\] | Initial APIC ID |
/// | 0x1  | EBX\[23:16\] | Max addressable logical processor IDs |
/// | 0xB (all sub-leaves) | EDX | x2APIC ID |
/// | 0x1F (all sub-leaves) | EDX | x2APIC ID |
pub fn patch_cpuid_apic_id(
    cpuid: &CpuId,
    apic_id: u32,
    num_vcpus: u32,
) -> Result<CpuId, CpuError> {
    let mut entries: Vec<kvm_cpuid_entry2> = cpuid.as_slice().to_vec();
    for entry in entries.iter_mut() {
        match entry.function {
            CPUID_LEAF_FEATURES => {
                // EBX[31:24] = Initial APIC ID for this vCPU.
                entry.ebx = (entry.ebx & 0x00FF_FFFF) | (apic_id << 24);
                // EBX[23:16] = Maximum number of addressable IDs for
                // logical processors in this physical package.  Must
                // be >= num_vcpus so the kernel builds the correct
                // topology.
                entry.ebx = (entry.ebx & 0xFF00_FFFF) | (num_vcpus << 16);
            }
            0xB | 0x1F => {
                // EDX = x2APIC ID.  Applies to every sub-leaf.
                entry.edx = apic_id;
            }
            _ => {}
        }
    }
    CpuId::from_entries(&entries)
        .map_err(|_| CpuError::GetCpuid(kvm_ioctls::Error::new(libc::ENOMEM)))
}

/// Pin the vCPU's TSC to a fixed frequency.
///
/// After this call every `RDTSC` inside the guest will advance at
/// exactly `tsc_khz` kilohertz, regardless of the host CPU's actual
/// clock rate.  KVM performs the scaling transparently via its TSC
/// offset / multiplier mechanism.
///
/// This should be called once per vCPU, before the first `vcpu.run()`.
pub fn setup_tsc(vcpu: &VcpuFd, tsc_khz: u32) -> Result<(), CpuError> {
    vcpu.set_tsc_khz(tsc_khz).map_err(|e| CpuError::SetTscKhz {
        freq_khz: tsc_khz,
        source: e,
    })?;

    info!(
        "TSC pinned to {} kHz ({:.1} GHz)",
        tsc_khz,
        tsc_khz as f64 / 1_000_000.0,
    );
    Ok(())
}

// ─── VirtualTsc ──────────────────────────────────────────────────────

/// A purely-software TSC counter for fully deterministic time control.
///
/// When the VMM installs a KVM MSR filter that traps reads to `IA32_TSC`
/// (MSR 0x10), each trapped read can be answered with [`read`](Self::read)
/// and the counter advanced with [`tick`](Self::tick).  Because the
/// counter *only* moves when the VMM says so, execution becomes perfectly
/// reproducible regardless of wall-clock time or host load.
///
/// # Snapshot / Restore
///
/// `VirtualTsc` implements [`Clone`] and can round-trip through
/// [`VirtualTscSnapshot`] for serialisation:
///
/// ```
/// # use chaoscontrol_vmm::cpu::VirtualTsc;
/// let mut tsc = VirtualTsc::new(3_000_000, 1_000);
/// tsc.tick();
/// let snap = tsc.snapshot();
/// let restored = VirtualTsc::restore(&snap);
/// assert_eq!(tsc.read(), restored.read());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualTsc {
    /// Current counter value — what a trapped `RDTSC` should return.
    counter: u64,
    /// Fixed increment applied by each [`tick`](Self::tick) call.
    advance_per_tick: u64,
    /// Frequency metadata.  Not used for counting but needed to convert
    /// counter values into wall-clock durations.
    tsc_khz: u32,
}

impl VirtualTsc {
    /// Create a new virtual TSC starting at zero.
    pub fn new(tsc_khz: u32, advance_per_tick: u64) -> Self {
        Self {
            counter: 0,
            advance_per_tick,
            tsc_khz,
        }
    }

    /// Create a virtual TSC whose parameters are taken from a
    /// [`CpuConfig`].
    pub fn from_config(config: &CpuConfig) -> Self {
        Self::new(config.tsc_khz, config.tsc_advance_per_tick)
    }

    /// Read the current TSC value *without* advancing it.
    ///
    /// Use this to answer a trapped `IA32_TSC` MSR read.
    #[inline]
    pub fn read(&self) -> u64 {
        self.counter
    }

    /// Advance the counter by one tick and return the **new** value.
    ///
    /// Call this once per VM-exit (or per scheduling quantum) to make
    /// time progress deterministically.
    ///
    /// Delegates arithmetic to [`crate::verified::cpu::vtsc_tick`].
    #[inline]
    pub fn tick(&mut self) -> u64 {
        self.counter = crate::verified::cpu::vtsc_tick(self.counter, self.advance_per_tick);
        self.counter
    }

    /// Advance the counter by exactly `n` ticks and return the new value.
    ///
    /// Equivalent to calling [`tick`](Self::tick) `n` times, but O(1).
    ///
    /// Delegates arithmetic to [`crate::verified::cpu::vtsc_advance`].
    #[inline]
    pub fn advance(&mut self, n: u64) -> u64 {
        self.counter = crate::verified::cpu::vtsc_advance(self.counter, self.advance_per_tick, n);
        self.counter
    }

    /// Overwrite the counter with an exact value.
    ///
    /// Primarily used during snapshot restore.
    #[inline]
    pub fn set(&mut self, value: u64) {
        self.counter = value;
    }

    /// Advance the counter to at least `target`, rounding up to a tick
    /// boundary.
    ///
    /// Used by the HLT handler to fast-forward virtual time to the next
    /// timer event without calling `tick()` in a loop.
    ///
    /// Delegates arithmetic to [`crate::verified::cpu::vtsc_advance_to`].
    #[inline]
    pub fn advance_to(&mut self, target: u64) {
        self.counter =
            crate::verified::cpu::vtsc_advance_to(self.counter, self.advance_per_tick, target);
    }

    /// The configured TSC frequency in kHz.
    #[inline]
    pub fn tsc_khz(&self) -> u32 {
        self.tsc_khz
    }

    /// The per-tick advance amount.
    #[inline]
    pub fn advance_per_tick(&self) -> u64 {
        self.advance_per_tick
    }

    /// Convert the current counter value to elapsed nanoseconds.
    ///
    /// Uses `u128` intermediate arithmetic so that large counter values
    /// (up to ~195 years at 3 GHz) do not overflow.
    ///
    /// Delegates to [`crate::verified::cpu::elapsed_ns`].
    pub fn elapsed_ns(&self) -> u64 {
        crate::verified::cpu::elapsed_ns(self.counter, self.tsc_khz)
    }

    /// Produce a serialisable snapshot of the current state.
    pub fn snapshot(&self) -> VirtualTscSnapshot {
        VirtualTscSnapshot {
            counter: self.counter,
            advance_per_tick: self.advance_per_tick,
            tsc_khz: self.tsc_khz,
        }
    }

    /// Reconstruct a `VirtualTsc` from a previously-saved snapshot.
    pub fn restore(snapshot: &VirtualTscSnapshot) -> Self {
        Self {
            counter: snapshot.counter,
            advance_per_tick: snapshot.advance_per_tick,
            tsc_khz: snapshot.tsc_khz,
        }
    }
}

/// Serialisable representation of [`VirtualTsc`] state.
///
/// All fields are public so the struct can be freely (de)serialised with
/// serde, bincode, or any other format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualTscSnapshot {
    /// TSC counter value at the time of the snapshot.
    pub counter: u64,
    /// Per-tick advance amount.
    pub advance_per_tick: u64,
    /// Configured TSC frequency in kHz.
    pub tsc_khz: u32,
}

// ─── Internal: per-entry CPUID filter ────────────────────────────────

/// Apply determinism filters to a single CPUID entry in place.
///
/// Returns `true` if any register in the entry was modified.
fn filter_entry(entry: &mut kvm_cpuid_entry2, config: &CpuConfig) -> bool {
    match entry.function {
        // ── Leaf 0x0: Vendor ID + max leaf ──────────────────────
        // When fixed_family is set to an Intel family (6), override the
        // vendor string so the kernel's native_calibrate_tsc() trusts
        // CPUID leaf 0x15 for exact TSC frequency. Without this, AMD
        // hosts fall back to non-deterministic PIT-based calibration.
        // Also ensure max standard leaf (EAX) >= 0x15 so the kernel
        // knows leaf 0x15 is available.
        0x0 if config.fixed_family == Some(6) => {
            let orig = (entry.eax, entry.ebx, entry.ecx, entry.edx);
            // "GenuineIntel" = EBX:EDX:ECX
            entry.ebx = u32::from_le_bytes(*b"Genu");
            entry.edx = u32::from_le_bytes(*b"ineI");
            entry.ecx = u32::from_le_bytes(*b"ntel");
            // Ensure max standard CPUID leaf >= 0x15 (TSC info)
            if entry.eax < CPUID_LEAF_TSC_INFO {
                entry.eax = CPUID_LEAF_TSC_INFO;
            }
            (entry.eax, entry.ebx, entry.ecx, entry.edx) != orig
        }

        // ── Leaf 0x1: Processor Info and Feature Bits ───────────
        CPUID_LEAF_FEATURES => {
            let orig = (entry.eax, entry.ebx, entry.ecx);

            // Unconditionally strip non-deterministic / timing-sensitive
            // features.
            entry.ecx &= !CPUID_1_ECX_RDRAND;
            entry.ecx &= !CPUID_1_ECX_TSC_DEADLINE;

            if config.hide_hypervisor {
                entry.ecx &= !CPUID_1_ECX_HYPERVISOR;
            }
            if config.hide_tsc {
                entry.edx &= !CPUID_1_EDX_TSC;
            }

            // Fix Initial APIC ID (EBX bits 31:24) to 0.
            // Without this, the host's physical APIC ID leaks through,
            // causing non-deterministic "APIC ID mismatch" warnings.
            entry.ebx &= 0x00FF_FFFF; // clear Initial APIC ID (bits 31:24)

            // Optionally lock down the processor identity so that
            // snapshots are portable across CPU generations.
            if let Some(family) = config.fixed_family {
                encode_family(&mut entry.eax, family);
            }
            if let Some(model) = config.fixed_model {
                encode_model(&mut entry.eax, model);
            }
            if let Some(stepping) = config.fixed_stepping {
                encode_stepping(&mut entry.eax, stepping);
            }

            (entry.eax, entry.ebx, entry.ecx) != orig
        }

        // ── Leaf 0x7 sub-leaf 0: Structured Extended Features ──
        CPUID_LEAF_STRUCTURED_EXT if entry.index == 0 => {
            let orig = entry.ebx;
            let orig_ecx = entry.ecx;
            let orig_edx = entry.edx;

            // RDSEED is always stripped (hardware entropy).
            entry.ebx &= !CPUID_7_EBX_RDSEED;

            // Vector extensions are opt-in because they vary across
            // host CPUs.
            if !config.allow_avx2 {
                entry.ebx &= !CPUID_7_EBX_AVX2;
            }
            if !config.allow_avx512 {
                entry.ebx &= !CPUID_7_EBX_AVX512F;
                // Also strip all AVX-512 sub-features to avoid
                // "avx512ifma enabled but avx512f disabled" warnings.
                // AVX-512 sub-features in EBX: bits 17,21,26,27,28,30,31
                entry.ebx &= !(1 << 17); // AVX512_VBMI (EBX bit 17 is actually in ECX for sub-leaf 0)
                entry.ebx &= !(1 << 21); // AVX512_IFMA
                entry.ebx &= !(1 << 26); // AVX512PF
                entry.ebx &= !(1 << 27); // AVX512ER
                entry.ebx &= !(1 << 28); // AVX512CD
                entry.ebx &= !(1 << 30); // AVX512BW
                entry.ebx &= !(1 << 31); // AVX512VL
                                         // Sub-features in ECX (leaf 7, sub-leaf 0)
                entry.ecx &= !(1 << 1); // AVX512_VBMI
                entry.ecx &= !(1 << 6); // AVX512_VBMI2
                entry.ecx &= !(1 << 11); // AVX512_VNNI
                entry.ecx &= !(1 << 12); // AVX512_BITALG
                entry.ecx &= !(1 << 14); // AVX512_VPOPCNTDQ
                                         // Sub-features in EDX (leaf 7, sub-leaf 0)
                entry.edx &= !(1 << 2); // AVX512_4VNNIW
                entry.edx &= !(1 << 3); // AVX512_4FMAPS
                entry.edx &= !(1 << 8); // AVX512_VP2INTERSECT
                entry.edx &= !(1 << 23); // AVX512_FP16
            }

            entry.ebx != orig || entry.ecx != orig_ecx || entry.edx != orig_edx
        }

        // ── Leaf 0x15: TSC / Crystal Clock ─────────────────────
        CPUID_LEAF_TSC_INFO => {
            let (denominator, numerator) = tsc_crystal_ratio(config.tsc_khz);
            entry.eax = denominator;
            entry.ebx = numerator;
            entry.ecx = CRYSTAL_CLOCK_HZ;
            true
        }

        // ── Leaf 0x16: Processor Frequency (MHz) ───────────────
        CPUID_LEAF_FREQ_INFO => {
            let mhz = config
                .fixed_frequency_mhz
                .map(|m| m as u32)
                .unwrap_or(config.tsc_khz / 1_000);
            entry.eax = mhz; // base
            entry.ebx = mhz; // max
            entry.ecx = mhz; // bus / reference
            true
        }

        // ── Leaves 0x40000000–0x400000FF: Hypervisor / KVM ─────
        f if config.hide_hypervisor
            && (CPUID_LEAF_KVM_SIGNATURE..=CPUID_LEAF_HV_RANGE_END).contains(&f) =>
        {
            entry.eax = 0;
            entry.ebx = 0;
            entry.ecx = 0;
            entry.edx = 0;
            true
        }

        // ── Leaf 0xB / 0x1F: Extended Topology ─────────────────
        // Fix x2APIC ID (EDX) to 0 so it matches our fixed APIC ID.
        0xB | 0x1F => {
            let orig = entry.edx;
            entry.edx = 0;
            entry.edx != orig
        }

        // ── Leaf 0x80000001: Extended Features ─────────────────
        CPUID_LEAF_EXT_FEATURES => {
            let orig = entry.edx;
            entry.edx &= !CPUID_EXT_EDX_RDTSCP;
            entry.edx != orig
        }

        // ── Leaf 0x80000007: Advanced Power Management ─────────
        CPUID_LEAF_APM => {
            let orig = entry.edx;
            entry.edx &= !CPUID_APM_EDX_INVARIANT_TSC;
            entry.edx != orig
        }

        // All other leaves pass through unchanged.
        _ => false,
    }
}

// ─── Internal: CPUID 0x15 ratio computation ──────────────────────────

/// Compute `(denominator, numerator)` for the CPUID 0x15 leaf such that
///
/// ```text
///   TSC_freq  =  CRYSTAL_CLOCK_HZ × numerator / denominator
///             =  tsc_khz × 1000
/// ```
///
/// The ratio is reduced to lowest terms via GCD so the values stay small.
///
/// Delegates to [`crate::verified::cpu::tsc_crystal_ratio`].
fn tsc_crystal_ratio(tsc_khz: u32) -> (u32, u32) {
    crate::verified::cpu::tsc_crystal_ratio(tsc_khz)
}

// `gcd` has moved to `crate::verified::cpu::gcd`.
// Tests below import it from there.

// ─── Internal: model / family / stepping encoding ────────────────────

/// Encode a *display* family value into the CPUID 0x1 EAX register.
///
/// For families ≤ 15 the value fits in bits \[11:8\].  For families > 15
/// the base field is set to 0xF and the excess is placed in the
/// extended-family field (bits \[27:20\]).
///
/// Delegates to [`crate::verified::cpu::encode_family`].
fn encode_family(eax: &mut u32, family: u8) {
    crate::verified::cpu::encode_family(eax, family);
}

/// Encode a *display* model value into the CPUID 0x1 EAX register.
///
/// Low nibble → bits \[7:4\], high nibble → bits \[19:16\] (extended
/// model).
///
/// Delegates to [`crate::verified::cpu::encode_model`].
fn encode_model(eax: &mut u32, model: u8) {
    crate::verified::cpu::encode_model(eax, model);
}

/// Encode a stepping value into CPUID 0x1 EAX bits \[3:0\].
///
/// Delegates to [`crate::verified::cpu::encode_stepping`].
fn encode_stepping(eax: &mut u32, stepping: u8) {
    crate::verified::cpu::encode_stepping(eax, stepping);
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    // Import pure functions and EAX field constants from the verified module.
    use crate::verified::cpu::{
        gcd, EAX_EXT_FAMILY_SHIFT, EAX_EXT_MODEL_SHIFT, EAX_FAMILY_SHIFT, EAX_MODEL_SHIFT,
    };

    // -- helpers --

    fn make_entry(
        function: u32,
        index: u32,
        eax: u32,
        ebx: u32,
        ecx: u32,
        edx: u32,
    ) -> kvm_cpuid_entry2 {
        kvm_cpuid_entry2 {
            function,
            index,
            flags: 0,
            eax,
            ebx,
            ecx,
            edx,
            padding: [0; 3],
        }
    }

    // -- CpuConfig defaults --

    #[test]
    fn default_config_values() {
        let c = CpuConfig::default();
        assert_eq!(c.tsc_khz, DEFAULT_TSC_KHZ);
        assert_eq!(c.tsc_advance_per_tick, DEFAULT_TSC_ADVANCE);
        assert!(!c.allow_avx2);
        assert!(!c.allow_avx512);
        assert!(!c.hide_hypervisor);
        assert!(c.fixed_family.is_none());
        assert!(c.fixed_model.is_none());
        assert!(c.fixed_stepping.is_none());
        assert!(c.fixed_frequency_mhz.is_none());
        assert_eq!(c.seed, 0);
    }

    // -- VirtualTsc --

    #[test]
    fn vtsc_starts_at_zero() {
        let tsc = VirtualTsc::new(DEFAULT_TSC_KHZ, 1_000);
        assert_eq!(tsc.read(), 0);
    }

    #[test]
    fn vtsc_tick_advances() {
        let mut tsc = VirtualTsc::new(DEFAULT_TSC_KHZ, 1_000);
        assert_eq!(tsc.tick(), 1_000);
        assert_eq!(tsc.tick(), 2_000);
        assert_eq!(tsc.read(), 2_000);
    }

    #[test]
    fn vtsc_advance_multiple() {
        let mut tsc = VirtualTsc::new(DEFAULT_TSC_KHZ, 500);
        assert_eq!(tsc.advance(10), 5_000);
        assert_eq!(tsc.read(), 5_000);
    }

    #[test]
    fn vtsc_set_overrides() {
        let mut tsc = VirtualTsc::new(DEFAULT_TSC_KHZ, 1_000);
        tsc.set(42_000);
        assert_eq!(tsc.read(), 42_000);
        assert_eq!(tsc.tick(), 43_000);
    }

    #[test]
    fn vtsc_elapsed_ns() {
        // At 3 GHz, 3 000 000 ticks = 1 ms = 1 000 000 ns.
        let mut tsc = VirtualTsc::new(3_000_000, 1);
        tsc.set(3_000_000);
        assert_eq!(tsc.elapsed_ns(), 1_000_000);
    }

    #[test]
    fn vtsc_elapsed_ns_large_value() {
        // 3 GHz, counter at 3 × 10^12 → 1 000 s = 10^12 ns.
        let mut tsc = VirtualTsc::new(3_000_000, 1);
        tsc.set(3_000_000_000_000);
        assert_eq!(tsc.elapsed_ns(), 1_000_000_000_000);
    }

    #[test]
    fn vtsc_snapshot_roundtrip() {
        let mut tsc = VirtualTsc::new(DEFAULT_TSC_KHZ, 1_000);
        tsc.tick();
        tsc.tick();
        let snap = tsc.snapshot();
        let restored = VirtualTsc::restore(&snap);
        assert_eq!(tsc, restored);
        assert_eq!(restored.read(), 2_000);
    }

    #[test]
    fn vtsc_wraps_at_u64_max() {
        let mut tsc = VirtualTsc::new(DEFAULT_TSC_KHZ, 1);
        tsc.set(u64::MAX);
        assert_eq!(tsc.tick(), 0);
    }

    #[test]
    fn vtsc_from_config() {
        let config = CpuConfig {
            tsc_khz: 2_500_000,
            tsc_advance_per_tick: 500,
            ..Default::default()
        };
        let tsc = VirtualTsc::from_config(&config);
        assert_eq!(tsc.tsc_khz(), 2_500_000);
        assert_eq!(tsc.advance_per_tick(), 500);
        assert_eq!(tsc.read(), 0);
    }

    // -- Leaf 0x1 filtering --

    #[test]
    fn filter_leaf1_strips_rdrand_and_tsc_deadline() {
        let c = CpuConfig::default();
        let mut e = make_entry(
            0x1,
            0,
            0,
            0,
            CPUID_1_ECX_RDRAND | CPUID_1_ECX_TSC_DEADLINE | (1 << 0), // SSE3 kept
            0,
        );
        assert!(filter_entry(&mut e, &c));
        assert_eq!(e.ecx & CPUID_1_ECX_RDRAND, 0, "RDRAND must be gone");
        assert_eq!(
            e.ecx & CPUID_1_ECX_TSC_DEADLINE,
            0,
            "TSC-Deadline must be gone"
        );
        assert_ne!(e.ecx & 1, 0, "SSE3 bit must survive");
    }

    #[test]
    fn filter_leaf1_hides_hypervisor_when_asked() {
        let c = CpuConfig {
            hide_hypervisor: true,
            ..Default::default()
        };
        let mut e = make_entry(0x1, 0, 0, 0, CPUID_1_ECX_HYPERVISOR, 0);
        filter_entry(&mut e, &c);
        assert_eq!(e.ecx & CPUID_1_ECX_HYPERVISOR, 0);
    }

    #[test]
    fn filter_leaf1_keeps_hypervisor_by_default() {
        let c = CpuConfig::default();
        let mut e = make_entry(0x1, 0, 0, 0, CPUID_1_ECX_HYPERVISOR, 0);
        filter_entry(&mut e, &c);
        assert_ne!(e.ecx & CPUID_1_ECX_HYPERVISOR, 0);
    }

    #[test]
    fn filter_leaf1_fixed_family_simple() {
        let c = CpuConfig {
            fixed_family: Some(6),
            ..Default::default()
        };
        let mut e = make_entry(0x1, 0, 0xFFFF_FFFF, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_eq!((e.eax >> EAX_FAMILY_SHIFT) & 0xF, 6);
        assert_eq!((e.eax >> EAX_EXT_FAMILY_SHIFT) & 0xFF, 0);
    }

    #[test]
    fn filter_leaf1_fixed_family_extended() {
        let c = CpuConfig {
            fixed_family: Some(0x1F),
            ..Default::default()
        };
        let mut e = make_entry(0x1, 0, 0, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_eq!((e.eax >> EAX_FAMILY_SHIFT) & 0xF, 0xF);
        assert_eq!((e.eax >> EAX_EXT_FAMILY_SHIFT) & 0xFF, 0x10);
    }

    #[test]
    fn filter_leaf1_fixed_model() {
        let c = CpuConfig {
            fixed_model: Some(0xA5),
            ..Default::default()
        };
        let mut e = make_entry(0x1, 0, 0, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_eq!((e.eax >> EAX_MODEL_SHIFT) & 0xF, 0x5);
        assert_eq!((e.eax >> EAX_EXT_MODEL_SHIFT) & 0xF, 0xA);
    }

    #[test]
    fn filter_leaf1_fixed_stepping() {
        let c = CpuConfig {
            fixed_stepping: Some(3),
            ..Default::default()
        };
        let mut e = make_entry(0x1, 0, 0xFFFF_FFF0, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_eq!(e.eax & 0xF, 3);
        // Other bits in EAX not clobbered (except those cleared by RDRAND
        // filter — but EAX is only touched by family/model/stepping).
        assert_eq!(e.eax & 0xFFFF_FFF0, 0xFFFF_FFF0);
    }

    // -- Leaf 0x7 filtering --

    #[test]
    fn filter_leaf7_strips_rdseed_avx2_avx512_by_default() {
        let c = CpuConfig::default();
        let mut e = make_entry(
            0x7,
            0,
            0,
            CPUID_7_EBX_RDSEED | CPUID_7_EBX_AVX2 | CPUID_7_EBX_AVX512F | (1 << 0), // FSGSBASE kept
            0,
            0,
        );
        filter_entry(&mut e, &c);
        assert_eq!(e.ebx & CPUID_7_EBX_RDSEED, 0);
        assert_eq!(e.ebx & CPUID_7_EBX_AVX2, 0);
        assert_eq!(e.ebx & CPUID_7_EBX_AVX512F, 0);
        assert_ne!(e.ebx & 1, 0, "FSGSBASE must survive");
    }

    #[test]
    fn filter_leaf7_allows_avx2_when_configured() {
        let c = CpuConfig {
            allow_avx2: true,
            ..Default::default()
        };
        let mut e = make_entry(0x7, 0, 0, CPUID_7_EBX_AVX2 | CPUID_7_EBX_RDSEED, 0, 0);
        filter_entry(&mut e, &c);
        assert_ne!(e.ebx & CPUID_7_EBX_AVX2, 0, "AVX2 preserved");
        assert_eq!(e.ebx & CPUID_7_EBX_RDSEED, 0, "RDSEED still gone");
    }

    #[test]
    fn filter_leaf7_allows_avx512_when_configured() {
        let c = CpuConfig {
            allow_avx512: true,
            ..Default::default()
        };
        let mut e = make_entry(
            0x7,
            0,
            0,
            CPUID_7_EBX_AVX512F | CPUID_7_EBX_AVX2 | CPUID_7_EBX_RDSEED,
            0,
            0,
        );
        filter_entry(&mut e, &c);
        assert_ne!(e.ebx & CPUID_7_EBX_AVX512F, 0, "AVX-512 preserved");
        assert_eq!(e.ebx & CPUID_7_EBX_AVX2, 0, "AVX2 still disabled");
        assert_eq!(e.ebx & CPUID_7_EBX_RDSEED, 0, "RDSEED still gone");
    }

    #[test]
    fn filter_leaf7_subleaf1_untouched() {
        let c = CpuConfig::default();
        let mut e = make_entry(0x7, 1, 0, CPUID_7_EBX_RDSEED, 0, 0);
        assert!(!filter_entry(&mut e, &c));
        assert_ne!(
            e.ebx & CPUID_7_EBX_RDSEED,
            0,
            "sub-leaf 1 must not be filtered"
        );
    }

    // -- Leaf 0x15 (TSC info) --

    #[test]
    fn filter_leaf15_encodes_3ghz() {
        let c = CpuConfig {
            tsc_khz: 3_000_000,
            ..Default::default()
        };
        let mut e = make_entry(0x15, 0, 0, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_ne!(e.eax, 0, "denominator must be non-zero");
        let computed_hz = e.ecx as u64 * e.ebx as u64 / e.eax as u64;
        assert_eq!(computed_hz, 3_000_000_000);
    }

    #[test]
    fn filter_leaf15_encodes_2_4ghz() {
        let c = CpuConfig {
            tsc_khz: 2_400_000,
            ..Default::default()
        };
        let mut e = make_entry(0x15, 0, 0, 0, 0, 0);
        filter_entry(&mut e, &c);
        let computed_hz = e.ecx as u64 * e.ebx as u64 / e.eax as u64;
        assert_eq!(computed_hz, 2_400_000_000);
    }

    // -- Leaf 0x16 (frequency info) --

    #[test]
    fn filter_leaf16_derives_from_tsc_khz() {
        let c = CpuConfig {
            tsc_khz: 2_400_000,
            ..Default::default()
        };
        let mut e = make_entry(0x16, 0, 0, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_eq!(e.eax, 2_400);
        assert_eq!(e.ebx, 2_400);
        assert_eq!(e.ecx, 2_400);
    }

    #[test]
    fn filter_leaf16_explicit_mhz() {
        let c = CpuConfig {
            fixed_frequency_mhz: Some(3500),
            ..Default::default()
        };
        let mut e = make_entry(0x16, 0, 0, 0, 0, 0);
        filter_entry(&mut e, &c);
        assert_eq!(e.eax, 3500);
        assert_eq!(e.ebx, 3500);
        assert_eq!(e.ecx, 3500);
    }

    // -- Hypervisor / KVM leaves --

    #[test]
    fn filter_kvm_leaves_zeroed_when_hidden() {
        let c = CpuConfig {
            hide_hypervisor: true,
            ..Default::default()
        };
        let mut e = make_entry(0x40000000, 0, 0x4B4D564B, 0x564B, 0, 0);
        assert!(filter_entry(&mut e, &c));
        assert_eq!(e.eax, 0);
        assert_eq!(e.ebx, 0);
    }

    #[test]
    fn filter_kvm_leaves_kept_by_default() {
        let c = CpuConfig::default();
        let mut e = make_entry(0x40000000, 0, 0x4B4D564B, 0, 0, 0);
        assert!(!filter_entry(&mut e, &c));
        assert_ne!(e.eax, 0);
    }

    // -- Extended leaves --

    #[test]
    fn filter_ext_features_strips_rdtscp() {
        let c = CpuConfig::default();
        let mut e = make_entry(0x80000001, 0, 0, 0, 0, CPUID_EXT_EDX_RDTSCP | (1 << 20));
        filter_entry(&mut e, &c);
        assert_eq!(e.edx & CPUID_EXT_EDX_RDTSCP, 0);
        assert_ne!(e.edx & (1 << 20), 0, "NX bit must survive");
    }

    #[test]
    fn filter_apm_strips_invariant_tsc() {
        let c = CpuConfig::default();
        let mut e = make_entry(0x80000007, 0, 0, 0, 0, CPUID_APM_EDX_INVARIANT_TSC);
        filter_entry(&mut e, &c);
        assert_eq!(e.edx & CPUID_APM_EDX_INVARIANT_TSC, 0);
    }

    // -- Unrelated leaf passes through --

    #[test]
    fn filter_unknown_leaf_unchanged() {
        let c = CpuConfig::default();
        let mut e = make_entry(0x4, 0, 0xDEAD, 0xBEEF, 0xCAFE, 0xF00D);
        assert!(!filter_entry(&mut e, &c));
        assert_eq!(e.eax, 0xDEAD);
        assert_eq!(e.ebx, 0xBEEF);
        assert_eq!(e.ecx, 0xCAFE);
        assert_eq!(e.edx, 0xF00D);
    }

    // -- Model / family / stepping encoding --

    #[test]
    fn encode_family_simple() {
        let mut eax: u32 = 0;
        encode_family(&mut eax, 6);
        assert_eq!((eax >> EAX_FAMILY_SHIFT) & 0xF, 6);
        assert_eq!((eax >> EAX_EXT_FAMILY_SHIFT) & 0xFF, 0);
    }

    #[test]
    fn encode_family_extended() {
        let mut eax: u32 = 0;
        encode_family(&mut eax, 0x1F);
        assert_eq!((eax >> EAX_FAMILY_SHIFT) & 0xF, 0xF);
        assert_eq!((eax >> EAX_EXT_FAMILY_SHIFT) & 0xFF, 0x10);
    }

    #[test]
    fn encode_model_split_nibbles() {
        let mut eax: u32 = 0;
        encode_model(&mut eax, 0xA5);
        assert_eq!((eax >> EAX_MODEL_SHIFT) & 0xF, 0x5);
        assert_eq!((eax >> EAX_EXT_MODEL_SHIFT) & 0xF, 0xA);
    }

    #[test]
    fn encode_stepping_preserves_other_bits() {
        let mut eax: u32 = 0xFFFF_FFF0;
        encode_stepping(&mut eax, 7);
        assert_eq!(eax & 0xF, 7);
        assert_eq!(eax & 0xFFFF_FFF0, 0xFFFF_FFF0);
    }

    // -- GCD / crystal ratio --

    #[test]
    fn gcd_basic() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(100, 25), 25);
        assert_eq!(gcd(17, 13), 1);
        assert_eq!(gcd(0, 5), 5);
    }

    #[test]
    fn tsc_crystal_ratio_3ghz() {
        let (d, n) = tsc_crystal_ratio(3_000_000);
        let freq = CRYSTAL_CLOCK_HZ as u64 * n as u64 / d as u64;
        assert_eq!(freq, 3_000_000_000);
    }

    #[test]
    fn tsc_crystal_ratio_2_4ghz() {
        let (d, n) = tsc_crystal_ratio(2_400_000);
        let freq = CRYSTAL_CLOCK_HZ as u64 * n as u64 / d as u64;
        assert_eq!(freq, 2_400_000_000);
    }

    #[test]
    fn tsc_crystal_ratio_is_reduced() {
        // 3 GHz: 3_000_000 kHz → ratio should be 120/1, not 3_000_000_000/25_000_000.
        let (d, n) = tsc_crystal_ratio(3_000_000);
        assert_eq!(d, 1);
        assert_eq!(n, 120);
    }

    // -- patch_cpuid_apic_id --

    #[test]
    fn patch_apic_id_leaf1_initial_apic_id() {
        // Build a minimal CpuId with leaf 0x1 (EBX cleared to 0).
        let entries = vec![make_entry(0x1, 0, 0, 0x00FF_FFFF, 0, 0)];
        let cpuid = CpuId::from_entries(&entries).unwrap();

        let patched = patch_cpuid_apic_id(&cpuid, 3, 4).unwrap();
        let e = &patched.as_slice()[0];

        // EBX[31:24] = APIC ID = 3
        assert_eq!((e.ebx >> 24) & 0xFF, 3);
        // EBX[23:16] = num_vcpus = 4
        assert_eq!((e.ebx >> 16) & 0xFF, 4);
        // EBX[15:0] unchanged
        assert_eq!(e.ebx & 0xFFFF, 0xFFFF);
    }

    #[test]
    fn patch_apic_id_leaf0xb_edx() {
        // Leaf 0xB sub-leaf 0 (SMT level) and sub-leaf 1 (Core level).
        let entries = vec![
            make_entry(0xB, 0, 1, 1, 0x0100, 0),
            make_entry(0xB, 1, 4, 8, 0x0201, 0),
        ];
        let cpuid = CpuId::from_entries(&entries).unwrap();

        let patched = patch_cpuid_apic_id(&cpuid, 5, 8).unwrap();
        let sl = patched.as_slice();
        assert_eq!(sl[0].edx, 5, "sub-leaf 0 EDX must be APIC ID");
        assert_eq!(sl[1].edx, 5, "sub-leaf 1 EDX must be APIC ID");
        // EAX/EBX/ECX unchanged
        assert_eq!(sl[0].eax, 1);
        assert_eq!(sl[1].ebx, 8);
    }

    #[test]
    fn patch_apic_id_leaf0x1f_edx() {
        let entries = vec![make_entry(0x1F, 0, 0, 0, 0, 0)];
        let cpuid = CpuId::from_entries(&entries).unwrap();

        let patched = patch_cpuid_apic_id(&cpuid, 7, 8).unwrap();
        assert_eq!(patched.as_slice()[0].edx, 7);
    }

    #[test]
    fn patch_apic_id_bsp_is_zero() {
        let entries = vec![
            make_entry(0x1, 0, 0, 0, 0, 0),
            make_entry(0xB, 0, 0, 0, 0, 99),
        ];
        let cpuid = CpuId::from_entries(&entries).unwrap();

        let patched = patch_cpuid_apic_id(&cpuid, 0, 2).unwrap();
        let sl = patched.as_slice();
        assert_eq!((sl[0].ebx >> 24) & 0xFF, 0, "BSP APIC ID = 0");
        assert_eq!(sl[1].edx, 0, "BSP x2APIC ID = 0");
    }

    #[test]
    fn patch_apic_id_does_not_touch_other_leaves() {
        let entries = vec![
            make_entry(0x7, 0, 0xAAAA, 0xBBBB, 0xCCCC, 0xDDDD),
            make_entry(0xB, 0, 1, 2, 3, 0),
        ];
        let cpuid = CpuId::from_entries(&entries).unwrap();

        let patched = patch_cpuid_apic_id(&cpuid, 2, 4).unwrap();
        let sl = patched.as_slice();
        // Leaf 0x7 untouched
        assert_eq!(sl[0].eax, 0xAAAA);
        assert_eq!(sl[0].ebx, 0xBBBB);
        assert_eq!(sl[0].ecx, 0xCCCC);
        assert_eq!(sl[0].edx, 0xDDDD);
        // Leaf 0xB EDX patched
        assert_eq!(sl[1].edx, 2);
    }
}
