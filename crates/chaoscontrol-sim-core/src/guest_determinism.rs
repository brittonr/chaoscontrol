//! Pure guest operating-system determinism profile and drift decisions.
//!
//! The imperative VMM shell applies this plan to Linux boot inputs. This
//! module does not read clocks, entropy, files, processes, or guest memory.

pub const GUEST_DETERMINISM_PROFILE_SCHEMA: &str = "chaoscontrol.guest-determinism-profile.v1";
pub const GUEST_DETERMINISM_PROBE_SCHEMA: &str = "chaoscontrol.guest-determinism-probe.v1";
pub const GUEST_DETERMINISM_DRIFT_SCHEMA: &str = "chaoscontrol.guest-determinism-drift.v1";
pub const LINUX_SETUP_RNG_SEED_TYPE: u32 = 9;
pub const LINUX_SETUP_DATA_HEADER_BYTES: usize = 16;
pub const BOOT_ENTROPY_SEED_BYTES: usize = 32;
pub const LINUX_RNG_SETUP_DATA_BYTES: usize =
    LINUX_SETUP_DATA_HEADER_BYTES + BOOT_ENTROPY_SEED_BYTES;

const PROFILE_ID_DOMAIN: &str = "chaoscontrol.guest-determinism.profile.v1";
const BOOT_ENTROPY_DOMAIN: &str = "chaoscontrol.guest-determinism.boot-entropy.v1";
const LAYOUT_BINDING_DOMAIN: &str = "chaoscontrol.guest-determinism.layout-binding.v1";
const PROBE_ID_DOMAIN: &str = "chaoscontrol.guest-determinism.probe.v1";

const REQUIRED_BASE_CMDLINE: [&str; 8] = [
    "nokaslr",
    "randomize_kstack_offset=off",
    "norandmaps",
    "random.trust_cpu=off",
    "random.trust_bootloader=off",
    "nohpet",
    "kfence.sample_interval=0",
    "no_hash_pointers",
];

const REQUIRED_NON_CLAIMS: [&str; 5] = [
    "no independent fresh-boot CRNG equality claim",
    "no host-side or cross-machine replay claim",
    "no reads outside the admitted surface list",
    "no arbitrary closed-binary syscall-interception claim",
    "no host signal-timing claim",
];

/// One guest-visible surface admitted by the bounded profile.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum GuestDeterminismSurface {
    BootEntropy,
    MonotonicClock,
    ProcessLayout,
    SignalOrder,
}

const ADMITTED_SURFACES: [GuestDeterminismSurface; 4] = [
    GuestDeterminismSurface::BootEntropy,
    GuestDeterminismSurface::MonotonicClock,
    GuestDeterminismSurface::ProcessLayout,
    GuestDeterminismSurface::SignalOrder,
];

/// Guest clock selected by the bounded profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GuestClockMode {
    VirtualTsc,
    DeterministicJiffies,
}

/// Inputs that fully select one guest determinism profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuestDeterminismInput {
    pub run_seed: u64,
    pub vm_id: u32,
    pub vcpu_count: u32,
    pub tsc_khz: u32,
    pub clock_mode: GuestClockMode,
}

/// Pure plan applied by the VMM and bound into validation receipts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuestDeterminismProfile {
    pub schema: String,
    pub profile_id: String,
    pub input: GuestDeterminismInput,
    pub boot_entropy_seed_blake3: String,
    pub layout_binding_blake3: String,
    pub layout_policy: String,
    pub clock_policy: String,
    pub signal_policy: String,
    pub required_cmdline: Vec<String>,
    pub admitted_surfaces: Vec<GuestDeterminismSurface>,
    pub non_claims: Vec<String>,
}

/// One guest-produced observation over every admitted surface.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuestDeterminismProbe {
    pub schema: String,
    pub entropy_hex: String,
    pub monotonic_delta_ns: u64,
    pub text_address: u64,
    pub stack_address: u64,
    pub heap_address: u64,
    pub signal_order: Vec<u32>,
}

/// Result of comparing two observations selected by one profile.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuestDeterminismDriftReport {
    pub schema: String,
    pub profile_id: String,
    pub profile: GuestDeterminismProfile,
    pub left_probe_blake3: String,
    pub right_probe_blake3: String,
    pub accepted: bool,
    pub drifted_surfaces: Vec<GuestDeterminismSurface>,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuestDeterminismError {
    InvalidSchema,
    InvalidVcpuCount,
    InvalidTscFrequency,
    InvalidProfileIdentity,
    InvalidBootEntropyIdentity,
    InvalidLayoutBinding,
    InvalidLayoutPolicy,
    InvalidClockPolicy,
    InvalidSignalPolicy,
    MissingCmdlineToken(String),
    InvalidSurfaceList,
    InvalidNonClaims,
    InvalidEntropyObservation,
    InvalidSignalObservation,
    Serialization,
}

/// Derive exact early-boot entropy bytes from the admitted run configuration.
// r[impl chaoscontrol.guest_determinism.boot_entropy]
#[must_use]
pub fn derive_boot_entropy_seed(input: GuestDeterminismInput) -> [u8; BOOT_ENTROPY_SEED_BYTES] {
    derive_material(BOOT_ENTROPY_DOMAIN, input)
}

/// Derive a binding for the fixed-layout policy and its run configuration.
// r[impl chaoscontrol.guest_determinism.layout]
#[must_use]
pub fn derive_layout_binding(input: GuestDeterminismInput) -> [u8; BOOT_ENTROPY_SEED_BYTES] {
    derive_material(LAYOUT_BINDING_DOMAIN, input)
}

/// Construct the complete deterministic Linux guest profile.
// r[impl chaoscontrol.guest_determinism.boundary]
// r[impl chaoscontrol.guest_determinism.time_surface]
// r[impl chaoscontrol.guest_determinism.signals]
#[must_use]
pub fn build_guest_determinism_profile(input: GuestDeterminismInput) -> GuestDeterminismProfile {
    let boot_entropy_seed = derive_boot_entropy_seed(input);
    let layout_binding = derive_layout_binding(input);
    let profile_id = profile_identity(input, &boot_entropy_seed, &layout_binding);
    GuestDeterminismProfile {
        schema: GUEST_DETERMINISM_PROFILE_SCHEMA.to_string(),
        profile_id,
        input,
        boot_entropy_seed_blake3: blake3_hex(&boot_entropy_seed),
        layout_binding_blake3: blake3_hex(&layout_binding),
        layout_policy: "linux-randomization-disabled-v1".to_string(),
        clock_policy: clock_policy(input.clock_mode).to_string(),
        signal_policy: "serialized-vcpu-schedule-observed-v1".to_string(),
        required_cmdline: required_cmdline(input.clock_mode),
        admitted_surfaces: ADMITTED_SURFACES.to_vec(),
        non_claims: REQUIRED_NON_CLAIMS
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

/// Validate a profile and the effective Linux command line that will apply it.
pub fn validate_guest_determinism_profile(
    profile: &GuestDeterminismProfile,
    effective_cmdline: &str,
) -> Result<(), GuestDeterminismError> {
    if profile.schema != GUEST_DETERMINISM_PROFILE_SCHEMA {
        return Err(GuestDeterminismError::InvalidSchema);
    }
    if profile.input.vcpu_count == 0 {
        return Err(GuestDeterminismError::InvalidVcpuCount);
    }
    if profile.input.tsc_khz == 0 {
        return Err(GuestDeterminismError::InvalidTscFrequency);
    }
    let entropy = derive_boot_entropy_seed(profile.input);
    let layout = derive_layout_binding(profile.input);
    if profile.boot_entropy_seed_blake3 != blake3_hex(&entropy) {
        return Err(GuestDeterminismError::InvalidBootEntropyIdentity);
    }
    if profile.layout_binding_blake3 != blake3_hex(&layout) {
        return Err(GuestDeterminismError::InvalidLayoutBinding);
    }
    if profile.profile_id != profile_identity(profile.input, &entropy, &layout) {
        return Err(GuestDeterminismError::InvalidProfileIdentity);
    }
    if profile.layout_policy != "linux-randomization-disabled-v1" {
        return Err(GuestDeterminismError::InvalidLayoutPolicy);
    }
    if profile.clock_policy != clock_policy(profile.input.clock_mode) {
        return Err(GuestDeterminismError::InvalidClockPolicy);
    }
    if profile.signal_policy != "serialized-vcpu-schedule-observed-v1" {
        return Err(GuestDeterminismError::InvalidSignalPolicy);
    }
    let required_cmdline = required_cmdline(profile.input.clock_mode);
    if profile.required_cmdline != required_cmdline {
        return Err(GuestDeterminismError::MissingCmdlineToken(
            "profile-token-set".to_string(),
        ));
    }
    for token in &profile.required_cmdline {
        if !effective_cmdline
            .split_whitespace()
            .any(|item| item == token)
        {
            return Err(GuestDeterminismError::MissingCmdlineToken(token.clone()));
        }
    }
    if profile.admitted_surfaces != ADMITTED_SURFACES {
        return Err(GuestDeterminismError::InvalidSurfaceList);
    }
    if profile.non_claims
        != REQUIRED_NON_CLAIMS
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    {
        return Err(GuestDeterminismError::InvalidNonClaims);
    }
    Ok(())
}

/// Encode a Linux x86 `SETUP_RNG_SEED` node for early boot.
// r[impl chaoscontrol.guest_determinism.boot_entropy]
#[must_use]
pub fn encode_linux_rng_seed_setup_data(
    seed: [u8; BOOT_ENTROPY_SEED_BYTES],
) -> [u8; LINUX_RNG_SETUP_DATA_BYTES] {
    let mut encoded = [0_u8; LINUX_RNG_SETUP_DATA_BYTES];
    let next_end = core::mem::size_of::<u64>();
    let type_end = next_end + core::mem::size_of::<u32>();
    let length_end = type_end + core::mem::size_of::<u32>();
    encoded[..next_end].copy_from_slice(&0_u64.to_le_bytes());
    encoded[next_end..type_end].copy_from_slice(&LINUX_SETUP_RNG_SEED_TYPE.to_le_bytes());
    encoded[type_end..length_end].copy_from_slice(&(BOOT_ENTROPY_SEED_BYTES as u32).to_le_bytes());
    encoded[length_end..].copy_from_slice(&seed);
    encoded
}

/// Validate one guest observation before it can enter the drift gate.
pub fn validate_guest_determinism_probe(
    probe: &GuestDeterminismProbe,
) -> Result<(), GuestDeterminismError> {
    const ENTROPY_HEX_CHARS: usize = BOOT_ENTROPY_SEED_BYTES * 2;
    const SIGNAL_COUNT: usize = 2;
    if probe.schema != GUEST_DETERMINISM_PROBE_SCHEMA {
        return Err(GuestDeterminismError::InvalidSchema);
    }
    if probe.entropy_hex.len() != ENTROPY_HEX_CHARS
        || !probe
            .entropy_hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(GuestDeterminismError::InvalidEntropyObservation);
    }
    if probe.signal_order.len() != SIGNAL_COUNT || probe.signal_order.contains(&0) {
        return Err(GuestDeterminismError::InvalidSignalObservation);
    }
    Ok(())
}

/// Compare every admitted surface and name all observed drift.
// r[impl chaoscontrol.guest_determinism.validation_fixture]
// r[impl chaoscontrol.guest_determinism.validation]
pub fn compare_guest_determinism_probes(
    profile: &GuestDeterminismProfile,
    left: &GuestDeterminismProbe,
    right: &GuestDeterminismProbe,
) -> Result<GuestDeterminismDriftReport, GuestDeterminismError> {
    validate_guest_determinism_probe(left)?;
    validate_guest_determinism_probe(right)?;
    let mut drifted_surfaces = Vec::new();
    if left.entropy_hex != right.entropy_hex {
        drifted_surfaces.push(GuestDeterminismSurface::BootEntropy);
    }
    if left.monotonic_delta_ns != right.monotonic_delta_ns {
        drifted_surfaces.push(GuestDeterminismSurface::MonotonicClock);
    }
    if left.text_address != right.text_address
        || left.stack_address != right.stack_address
        || left.heap_address != right.heap_address
    {
        drifted_surfaces.push(GuestDeterminismSurface::ProcessLayout);
    }
    if left.signal_order != right.signal_order {
        drifted_surfaces.push(GuestDeterminismSurface::SignalOrder);
    }
    Ok(GuestDeterminismDriftReport {
        schema: GUEST_DETERMINISM_DRIFT_SCHEMA.to_string(),
        profile_id: profile.profile_id.clone(),
        profile: profile.clone(),
        left_probe_blake3: probe_identity(left)?,
        right_probe_blake3: probe_identity(right)?,
        accepted: drifted_surfaces.is_empty(),
        drifted_surfaces,
        non_claims: profile.non_claims.clone(),
    })
}

fn derive_material(domain: &str, input: GuestDeterminismInput) -> [u8; BOOT_ENTROPY_SEED_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0]);
    hash_input(&mut hasher, input);
    *hasher.finalize().as_bytes()
}

fn profile_identity(
    input: GuestDeterminismInput,
    entropy: &[u8; BOOT_ENTROPY_SEED_BYTES],
    layout: &[u8; BOOT_ENTROPY_SEED_BYTES],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PROFILE_ID_DOMAIN.as_bytes());
    hasher.update(&[0]);
    hash_input(&mut hasher, input);
    hasher.update(entropy);
    hasher.update(layout);
    for token in required_cmdline(input.clock_mode) {
        hash_string(&mut hasher, &token);
    }
    for non_claim in REQUIRED_NON_CLAIMS {
        hash_string(&mut hasher, non_claim);
    }
    format!("b3:{}", hasher.finalize().to_hex())
}

fn probe_identity(probe: &GuestDeterminismProbe) -> Result<String, GuestDeterminismError> {
    let bytes = serde_json::to_vec(probe).map_err(|_| GuestDeterminismError::Serialization)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(PROBE_ID_DOMAIN.as_bytes());
    hasher.update(&[0]);
    hasher.update(&bytes);
    Ok(format!("b3:{}", hasher.finalize().to_hex()))
}

fn hash_input(hasher: &mut blake3::Hasher, input: GuestDeterminismInput) {
    hasher.update(&input.run_seed.to_le_bytes());
    hasher.update(&input.vm_id.to_le_bytes());
    hasher.update(&input.vcpu_count.to_le_bytes());
    hasher.update(&input.tsc_khz.to_le_bytes());
    hasher.update(&[match input.clock_mode {
        GuestClockMode::VirtualTsc => 0,
        GuestClockMode::DeterministicJiffies => 1,
    }]);
}

fn clock_policy(mode: GuestClockMode) -> &'static str {
    match mode {
        GuestClockMode::VirtualTsc => "virtual-tsc-reset-before-entry-v1",
        GuestClockMode::DeterministicJiffies => "deterministic-jiffies-from-vmm-pit-v1",
    }
}

fn required_cmdline(mode: GuestClockMode) -> Vec<String> {
    let mut required = match mode {
        GuestClockMode::VirtualTsc => {
            vec!["clocksource=tsc".to_string(), "tsc=reliable".to_string()]
        }
        GuestClockMode::DeterministicJiffies => {
            vec!["clocksource=jiffies".to_string(), "notsc".to_string()]
        }
    };
    required.extend(REQUIRED_BASE_CMDLINE.iter().map(ToString::to_string));
    required
}

fn hash_string(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn blake3_hex(bytes: &[u8]) -> String {
    format!("b3:{}", blake3::hash(bytes).to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_SEED: u64 = 47;
    const OTHER_RUN_SEED: u64 = 48;
    const VM_ID: u32 = 2;
    const VCPU_COUNT: u32 = 1;
    const TSC_KHZ: u32 = 3_000_000;
    const CLOCK_DELTA_NS: u64 = 1_000;
    const TEXT_ADDRESS: u64 = 0x40_1000;
    const STACK_ADDRESS: u64 = 0x7fff_f000;
    const HEAP_ADDRESS: u64 = 0x20_0000;
    const FIRST_SIGNAL: u32 = 10;
    const SECOND_SIGNAL: u32 = 12;

    fn input() -> GuestDeterminismInput {
        GuestDeterminismInput {
            run_seed: RUN_SEED,
            vm_id: VM_ID,
            vcpu_count: VCPU_COUNT,
            tsc_khz: TSC_KHZ,
            clock_mode: GuestClockMode::VirtualTsc,
        }
    }

    fn cmdline() -> String {
        required_cmdline(GuestClockMode::VirtualTsc).join(" ")
    }

    fn probe() -> GuestDeterminismProbe {
        GuestDeterminismProbe {
            schema: GUEST_DETERMINISM_PROBE_SCHEMA.to_string(),
            entropy_hex: "ab".repeat(BOOT_ENTROPY_SEED_BYTES),
            monotonic_delta_ns: CLOCK_DELTA_NS,
            text_address: TEXT_ADDRESS,
            stack_address: STACK_ADDRESS,
            heap_address: HEAP_ADDRESS,
            signal_order: vec![FIRST_SIGNAL, SECOND_SIGNAL],
        }
    }

    #[test]
    fn same_input_derives_same_complete_profile() {
        let left = build_guest_determinism_profile(input());
        let right = build_guest_determinism_profile(input());
        assert_eq!(left, right);
        assert_eq!(
            validate_guest_determinism_profile(&left, &cmdline()),
            Ok(())
        );
    }

    #[test]
    fn deterministic_jiffies_profile_requires_hidden_tsc_tokens() {
        let mut jiffies_input = input();
        jiffies_input.clock_mode = GuestClockMode::DeterministicJiffies;
        let profile = build_guest_determinism_profile(jiffies_input);
        let cmdline = required_cmdline(GuestClockMode::DeterministicJiffies).join(" ");
        assert_eq!(
            validate_guest_determinism_profile(&profile, &cmdline),
            Ok(())
        );
        assert_eq!(
            profile.clock_policy,
            "deterministic-jiffies-from-vmm-pit-v1"
        );
    }

    #[test]
    fn changed_seed_changes_entropy_layout_and_profile_identities() {
        let left = build_guest_determinism_profile(input());
        let mut changed = input();
        changed.run_seed = OTHER_RUN_SEED;
        let right = build_guest_determinism_profile(changed);
        assert_ne!(
            left.boot_entropy_seed_blake3,
            right.boot_entropy_seed_blake3
        );
        assert_ne!(left.layout_binding_blake3, right.layout_binding_blake3);
        assert_ne!(left.profile_id, right.profile_id);
    }

    #[test]
    fn linux_setup_data_encodes_exact_header_and_seed() {
        let seed = derive_boot_entropy_seed(input());
        let encoded = encode_linux_rng_seed_setup_data(seed);
        assert_eq!(&encoded[..8], &0_u64.to_le_bytes());
        assert_eq!(&encoded[8..12], &LINUX_SETUP_RNG_SEED_TYPE.to_le_bytes());
        assert_eq!(
            &encoded[12..LINUX_SETUP_DATA_HEADER_BYTES],
            &(BOOT_ENTROPY_SEED_BYTES as u32).to_le_bytes()
        );
        assert_eq!(&encoded[LINUX_SETUP_DATA_HEADER_BYTES..], &seed);
    }

    #[test]
    fn profile_rejects_missing_clock_token() {
        let profile = build_guest_determinism_profile(input());
        let without_tsc = cmdline().replace("clocksource=tsc", "");
        assert_eq!(
            validate_guest_determinism_profile(&profile, &without_tsc),
            Err(GuestDeterminismError::MissingCmdlineToken(
                "clocksource=tsc".to_string()
            ))
        );
    }

    #[test]
    fn identical_probes_pass_bit_exact_gate() {
        let profile = build_guest_determinism_profile(input());
        let observation = probe();
        let report = compare_guest_determinism_probes(&profile, &observation, &observation)
            .expect("valid observations");
        assert!(report.accepted);
        assert!(report.drifted_surfaces.is_empty());
        assert_eq!(report.left_probe_blake3, report.right_probe_blake3);
    }

    #[test]
    fn each_individual_surface_drift_is_named() {
        let profile = build_guest_determinism_profile(input());
        let baseline = probe();

        let mut entropy_drift = baseline.clone();
        entropy_drift.entropy_hex = "cd".repeat(BOOT_ENTROPY_SEED_BYTES);
        assert_only_drift(
            &profile,
            &baseline,
            &entropy_drift,
            GuestDeterminismSurface::BootEntropy,
        );

        let mut clock_drift = baseline.clone();
        clock_drift.monotonic_delta_ns += 1;
        assert_only_drift(
            &profile,
            &baseline,
            &clock_drift,
            GuestDeterminismSurface::MonotonicClock,
        );

        let mut layout_drift = baseline.clone();
        layout_drift.heap_address += 1;
        assert_only_drift(
            &profile,
            &baseline,
            &layout_drift,
            GuestDeterminismSurface::ProcessLayout,
        );

        let mut signal_drift = baseline.clone();
        signal_drift.signal_order.swap(0, 1);
        assert_only_drift(
            &profile,
            &baseline,
            &signal_drift,
            GuestDeterminismSurface::SignalOrder,
        );
    }

    fn assert_only_drift(
        profile: &GuestDeterminismProfile,
        baseline: &GuestDeterminismProbe,
        drifted: &GuestDeterminismProbe,
        expected: GuestDeterminismSurface,
    ) {
        let report = compare_guest_determinism_probes(profile, baseline, drifted)
            .expect("valid observations");
        assert!(!report.accepted);
        assert_eq!(report.drifted_surfaces, vec![expected]);
    }

    #[test]
    fn malformed_entropy_and_signal_observations_fail_closed() {
        let mut bad_entropy = probe();
        bad_entropy.entropy_hex = "not-hex".to_string();
        assert_eq!(
            validate_guest_determinism_probe(&bad_entropy),
            Err(GuestDeterminismError::InvalidEntropyObservation)
        );

        let mut bad_signal = probe();
        bad_signal.signal_order.clear();
        assert_eq!(
            validate_guest_determinism_probe(&bad_signal),
            Err(GuestDeterminismError::InvalidSignalObservation)
        );
    }
}
