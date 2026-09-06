use serde::ser::Serialize;

pub const KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION: u64 = 1;
pub const KERNEL_BUNDLE_SMOKE_ROLE: &str = "kernel-bundle/vm-compat-smoke";
pub const KERNEL_BUNDLE_SMOKE_SCOPE: &str = "bounded disposable-VM compatibility smoke for one exact kernel-bundle cohort; not universal bootability, not module safety proof, not eBPF safety proof, not build correctness proof, not physical readiness";
const RECEIPT_DOMAIN: &str = "chaoscontrol/kernel-bundle/vm-compat-smoke/receipt/v1";
const PROFILE_DOMAIN: &str = "chaoscontrol/kernel-bundle/vm-compat-smoke/profile/v1";
const KVM_RAIL_DOMAIN: &str = "chaoscontrol/kernel-bundle/vm-compat-smoke/kvm-rail/v1";
pub const KERNEL_BUNDLE_KVM_MARKER_PREFIX: &str = "chaoscontrol-kernel-bundle:v1:";
pub const KERNEL_BUNDLE_KVM_EXECUTION_MODE: &str = "chaoscontrol-vmm-kvm";
pub const KERNEL_BUNDLE_TRANSCRIPT_EXECUTION_MODE: &str = "serial-marker-transcript";
const ONIX_KERNEL_BUILD_PREFIX: &str = "onix:blake3:kernel-build:";
const ONIX_BUNDLE_PREFIX: &str = "onix:blake3:bundle:";
const ONIX_MANIFEST_PREFIX: &str = "onix:blake3:manifest:";
const ONIX_MODULE_PACK_PREFIX: &str = "onix:blake3:module-pack:";
const ONIX_BPF_PACK_PREFIX: &str = "onix:blake3:bpf-pack:";
const MANTLE_BLAKE3_PREFIX: &str = "mantle://blake3/";
const BLAKE3_HEX_LENGTH: usize = 64;
const MAX_NON_CLAIMS: usize = 16;
const MAX_OBSERVATIONS: usize = 16;
const MAX_BOUND_SECONDS: u64 = 600;
const MIN_BOUND_SECONDS: u64 = 1;
pub const DEFAULT_KVM_MAX_EXITS: u64 = 50_000_000;
const MIN_KVM_MAX_EXITS: u64 = 1;
const MAX_KVM_MAX_EXITS: u64 = 500_000_000;
const SAMPLE_BOUND_SECONDS: u64 = 120;
const REQUIRED_NON_CLAIMS: &[&str] = &[
    "not universal bootability",
    "not module safety proof",
    "not eBPF safety proof",
    "not build correctness proof",
    "not physical readiness",
];
#[cfg(test)]
const EXPECTED_PRIVATE_KFUNC_PROFILE_ID: &str =
    "216bd1a6c5461209f340a9c4f4d00aacf5c2312679bb9cb5808d329c619fc589";
#[cfg(test)]
const EXPECTED_PRIVATE_KFUNC_RECEIPT_ID: &str =
    "fb37d05d6ee328b05d8f1bdc80ae0d622dcdef590f0dbf7e2721bb3993e76119";

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct KernelBundleSmokeProfile {
    pub schema_version: u64,
    pub role: String,
    pub campaign_id: String,
    pub onix: OnixKernelBundleRefs,
    pub mantle: MantleMaterializationRefs,
    pub runner: SmokeRunnerEvidence,
    pub boot: BootCase,
    pub module: ModuleCase,
    pub bpf: BpfCase,
    pub bounds: SmokeBounds,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct OnixKernelBundleRefs {
    pub architecture: String,
    pub kernel_release: String,
    pub kernel_build_identity: String,
    pub bundle_identity: String,
    pub manifest_identity: String,
    pub module_pack_identity: String,
    pub bpf_pack_identity: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct MantleMaterializationRefs {
    pub observation_blake3: String,
    pub module_blake3: String,
    pub module_object_ref: String,
    pub bpf_object_blake3: String,
    pub bpf_object_ref: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SmokeRunnerEvidence {
    pub runner: String,
    pub runner_receipt_blake3: String,
    pub behavior_status: String,
    pub evidence_class: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BootCase {
    pub observed_architecture: String,
    pub observed_kernel_release: String,
    pub readiness_observation: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ModuleCase {
    pub pack_identity: String,
    pub member_path: String,
    pub member_blake3: String,
    pub load_observation: String,
    pub unload_observation: String,
    pub cleanup_class: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BpfCase {
    pub pack_identity: String,
    pub object_path: String,
    pub object_blake3: String,
    pub section: String,
    pub attach: String,
    pub attach_target: String,
    pub verifier_observation: String,
    pub attach_observation: String,
    pub detach_observation: String,
    pub cleanup_class: String,
    pub required_kfuncs: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SmokeBounds {
    pub max_boot_seconds: u64,
    pub max_module_seconds: u64,
    pub max_bpf_seconds: u64,
    pub max_observations: usize,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct KernelBundleSmokeReceipt {
    pub schema_version: u64,
    pub role: String,
    pub campaign_id: String,
    pub status: String,
    pub profile_identity_blake3: String,
    pub onix: OnixKernelBundleRefs,
    pub mantle: MantleMaterializationRefs,
    pub runner: SmokeRunnerEvidence,
    pub terminal_classes: std::collections::BTreeMap<String, String>,
    pub observations: Vec<SmokeObservation>,
    pub bounds: SmokeBounds,
    pub non_claims: Vec<String>,
    pub receipt_identity_blake3: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SmokeObservation {
    pub case_id: String,
    pub class: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KernelBundleKvmScenario {
    Positive,
    StaleDigest,
    MissingKfunc,
    VerifierRejection,
    WrongAttachTarget,
    CleanupFailure,
}

impl KernelBundleKvmScenario {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::StaleDigest => "stale-digest",
            Self::MissingKfunc => "missing-kfunc",
            Self::VerifierRejection => "verifier-rejection",
            Self::WrongAttachTarget => "wrong-attach-target",
            Self::CleanupFailure => "cleanup-failure",
        }
    }

    pub fn parse(value: &str) -> crate::EvidenceResult<Self> {
        match value {
            "positive" => Ok(Self::Positive),
            "stale-digest" => Ok(Self::StaleDigest),
            "missing-kfunc" => Ok(Self::MissingKfunc),
            "verifier-rejection" => Ok(Self::VerifierRejection),
            "wrong-attach-target" => Ok(Self::WrongAttachTarget),
            "cleanup-failure" => Ok(Self::CleanupFailure),
            _ => Err(crate::EvidenceError::new(format!(
                "unsupported kernel-bundle KVM scenario: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct KernelBundleKvmRun {
    pub profile_identity_blake3: String,
    pub runner: String,
    pub execution_mode: String,
    pub scenario: KernelBundleKvmScenario,
    pub expected_kernel_image_blake3: Option<String>,
    pub expected_initrd_image_blake3: Option<String>,
    pub kernel_image_blake3: Option<String>,
    pub initrd_image_blake3: Option<String>,
    pub kvm_available: bool,
    pub loader_available: bool,
    pub max_exits: u64,
    pub exits_executed: u64,
    pub halted: bool,
    pub observations: Vec<SmokeObservation>,
    pub failure_class: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KernelBundleEvidenceUse {
    VmCompatibilitySmoke,
    SnapshotReplay,
    OnixLifecycleReplay,
    PhysicalReadiness,
    BuildCorrectness,
    SecurityProof,
    ReleaseEligibility,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct KernelBundleKvmRailReceipt {
    pub schema_version: u64,
    pub role: String,
    pub campaign_id: String,
    pub status: String,
    pub profile_identity_blake3: String,
    pub runner: String,
    pub execution_mode: String,
    pub scenario: KernelBundleKvmScenario,
    pub expected_kernel_image_blake3: Option<String>,
    pub expected_initrd_image_blake3: Option<String>,
    pub kernel_image_blake3: Option<String>,
    pub initrd_image_blake3: Option<String>,
    pub kvm_available: bool,
    pub loader_available: bool,
    pub negative_fixture_matched: bool,
    pub failure_class: Option<String>,
    pub terminal_classes: std::collections::BTreeMap<String, String>,
    pub observations: Vec<SmokeObservation>,
    pub issues: Vec<String>,
    pub bounds: SmokeBounds,
    pub non_claims: Vec<String>,
    pub receipt_identity_blake3: String,
}

pub fn validate_kernel_bundle_smoke_profile(
    profile: &KernelBundleSmokeProfile,
) -> crate::EvidenceResult<()> {
    let mut issues = Vec::new();
    validate_profile_shape(profile, &mut issues);
    validate_identity_refs(profile, &mut issues);
    validate_cases(profile, &mut issues);
    validate_non_claims(&profile.non_claims, &mut issues);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(crate::EvidenceError::new(issues.join("; ")))
    }
}

pub fn kernel_bundle_smoke_profile_identity(
    profile: &KernelBundleSmokeProfile,
) -> crate::EvidenceResult<String> {
    domain_hash(PROFILE_DOMAIN, profile)
}

pub fn kernel_bundle_smoke_receipt(
    profile: &KernelBundleSmokeProfile,
) -> crate::EvidenceResult<KernelBundleSmokeReceipt> {
    validate_kernel_bundle_smoke_profile(profile)?;
    let profile_identity_blake3 = kernel_bundle_smoke_profile_identity(profile)?;
    let terminal_classes = terminal_classes(profile);
    let observations = smoke_observations(profile);
    let mut receipt = KernelBundleSmokeReceipt {
        schema_version: KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION,
        role: KERNEL_BUNDLE_SMOKE_ROLE.to_string(),
        campaign_id: profile.campaign_id.clone(),
        status: "passed".to_string(),
        profile_identity_blake3,
        onix: profile.onix.clone(),
        mantle: profile.mantle.clone(),
        runner: profile.runner.clone(),
        terminal_classes,
        observations,
        bounds: profile.bounds.clone(),
        non_claims: profile.non_claims.clone(),
        receipt_identity_blake3: String::new(),
    };
    receipt.receipt_identity_blake3 = domain_hash(RECEIPT_DOMAIN, &receipt)?;
    debug_assert_eq!(receipt.role, KERNEL_BUNDLE_SMOKE_ROLE);
    debug_assert_eq!(receipt.status, "passed");
    Ok(receipt)
}

pub fn sample_mantle_private_kfunc_profile() -> KernelBundleSmokeProfile {
    KernelBundleSmokeProfile {
        schema_version: KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION,
        role: KERNEL_BUNDLE_SMOKE_ROLE.to_string(),
        campaign_id: "mantle-kernelscript-private-kfunc-2026-07-15".to_string(),
        onix: OnixKernelBundleRefs {
            architecture: "x86_64".to_string(),
            kernel_release: "6.18.20".to_string(),
            kernel_build_identity: "onix:blake3:kernel-build:4ee8064c7daf33498bd61d85d573c28b43febf54926bfe1e58ef5df76637e0c2".to_string(),
            bundle_identity: "onix:blake3:bundle:a669c75d896ca3fefcf8141576ceb1b883c34cf2418ea55d9d53393130a56e82".to_string(),
            manifest_identity: "onix:blake3:manifest:2b6b1bebbde7b93ca974b81d125be970d472c19f2deb17368cb9e5fd7a012688".to_string(),
            module_pack_identity: "onix:blake3:module-pack:b06089102d69299754550d55ea23d40b3235b2be010242a2a62c6de1d3aafcef".to_string(),
            bpf_pack_identity: "onix:blake3:bpf-pack:e63907102511d66cc006163e9e96e15b0e89e758a6843ab4d235faafc0eebb6a".to_string(),
        },
        mantle: MantleMaterializationRefs {
            observation_blake3: "1f3c566a24981f3518709bc5fba1342f93c0dd4fbcc46d55193040f0a5170d05".to_string(),
            module_blake3: "1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1".to_string(),
            module_object_ref: "mantle://blake3/1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1".to_string(),
            bpf_object_blake3: "b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de".to_string(),
            bpf_object_ref: "mantle://blake3/b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de".to_string(),
        },
        runner: SmokeRunnerEvidence {
            runner: "mantle-nixos-vm-smoke-via-chaoscontrol-adapter".to_string(),
            runner_receipt_blake3: "1f3c566a24981f3518709bc5fba1342f93c0dd4fbcc46d55193040f0a5170d05".to_string(),
            behavior_status: "passed".to_string(),
            evidence_class: "disposable-vm-smoke".to_string(),
        },
        boot: BootCase {
            observed_architecture: "x86_64".to_string(),
            observed_kernel_release: "6.18.20".to_string(),
            readiness_observation: "uname-r-matched".to_string(),
        },
        module: ModuleCase {
            pack_identity: "onix:blake3:module-pack:b06089102d69299754550d55ea23d40b3235b2be010242a2a62c6de1d3aafcef".to_string(),
            member_path: "modules/private_kfunc.mod.ko".to_string(),
            member_blake3: "1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1".to_string(),
            load_observation: "insmod-private-kfunc-succeeded".to_string(),
            unload_observation: "rmmod-private-kfunc-attempted".to_string(),
            cleanup_class: "succeeded".to_string(),
        },
        bpf: BpfCase {
            pack_identity: "onix:blake3:bpf-pack:e63907102511d66cc006163e9e96e15b0e89e758a6843ab4d235faafc0eebb6a".to_string(),
            object_path: "bpf/private_kfunc.ebpf.o".to_string(),
            object_blake3: "b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de".to_string(),
            section: "xdp".to_string(),
            attach: "xdp".to_string(),
            attach_target: "lo".to_string(),
            verifier_observation: "bpftool-prog-load-xdp-succeeded".to_string(),
            attach_observation: "private-kfunc-loader-attached".to_string(),
            detach_observation: "private-kfunc-loader-detached".to_string(),
            cleanup_class: "succeeded".to_string(),
            required_kfuncs: vec!["process_value".to_string()],
        },
        bounds: SmokeBounds {
            max_boot_seconds: SAMPLE_BOUND_SECONDS,
            max_module_seconds: SAMPLE_BOUND_SECONDS,
            max_bpf_seconds: SAMPLE_BOUND_SECONDS,
            max_observations: MAX_OBSERVATIONS,
        },
        non_claims: REQUIRED_NON_CLAIMS.iter().map(|claim| (*claim).to_string()).collect(),
    }
}

pub fn sample_mantle_private_kfunc_receipt() -> crate::EvidenceResult<KernelBundleSmokeReceipt> {
    kernel_bundle_smoke_receipt(&sample_mantle_private_kfunc_profile())
}

pub fn kernel_bundle_kvm_rail_receipt(
    profile: &KernelBundleSmokeProfile,
    run: &KernelBundleKvmRun,
) -> crate::EvidenceResult<KernelBundleKvmRailReceipt> {
    validate_kernel_bundle_smoke_profile(profile)?;
    let profile_identity_blake3 = kernel_bundle_smoke_profile_identity(profile)?;
    let mut issues = kvm_run_issues(profile, run, &profile_identity_blake3);
    let status = kvm_status(run, &issues);
    let observations = if status == "passed" {
        expected_kvm_observations(profile)
    } else {
        run.observations.clone()
    };
    let terminal_classes = if status == "passed" {
        terminal_classes(profile)
    } else {
        std::collections::BTreeMap::new()
    };
    dedup_issues(&mut issues);
    let negative_fixture_matched = negative_fixture_matched(run);
    let mut receipt = KernelBundleKvmRailReceipt {
        schema_version: KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION,
        role: KERNEL_BUNDLE_SMOKE_ROLE.to_string(),
        campaign_id: profile.campaign_id.clone(),
        status,
        profile_identity_blake3,
        runner: run.runner.clone(),
        execution_mode: run.execution_mode.clone(),
        scenario: run.scenario,
        expected_kernel_image_blake3: run.expected_kernel_image_blake3.clone(),
        expected_initrd_image_blake3: run.expected_initrd_image_blake3.clone(),
        kernel_image_blake3: run.kernel_image_blake3.clone(),
        initrd_image_blake3: run.initrd_image_blake3.clone(),
        kvm_available: run.kvm_available,
        loader_available: run.loader_available,
        negative_fixture_matched,
        failure_class: run.failure_class.clone(),
        terminal_classes,
        observations,
        issues,
        bounds: profile.bounds.clone(),
        non_claims: profile.non_claims.clone(),
        receipt_identity_blake3: String::new(),
    };
    receipt.receipt_identity_blake3 = domain_hash(KVM_RAIL_DOMAIN, &receipt)?;
    debug_assert_eq!(receipt.role, KERNEL_BUNDLE_SMOKE_ROLE);
    debug_assert!(matches!(
        receipt.status.as_str(),
        "passed" | "failed" | "blocked"
    ));
    Ok(receipt)
}

pub fn kernel_bundle_receipt_supports_use(
    receipt: &KernelBundleKvmRailReceipt,
    requested_use: KernelBundleEvidenceUse,
) -> bool {
    if requested_use != KernelBundleEvidenceUse::VmCompatibilitySmoke {
        return false;
    }
    let exact_mode = receipt.execution_mode == KERNEL_BUNDLE_KVM_EXECUTION_MODE;
    let positive_scenario = receipt.scenario == KernelBundleKvmScenario::Positive;
    let images_bound = receipt.expected_kernel_image_blake3.is_some()
        && receipt.expected_kernel_image_blake3 == receipt.kernel_image_blake3
        && receipt.expected_initrd_image_blake3.is_some()
        && receipt.expected_initrd_image_blake3 == receipt.initrd_image_blake3;
    receipt.status == "passed"
        && exact_mode
        && positive_scenario
        && images_bound
        && receipt.issues.is_empty()
}

pub fn extract_kvm_observations(serial_output: &str) -> Vec<SmokeObservation> {
    let mut observations = Vec::new();
    for line in serial_output.lines() {
        if observations.len() >= MAX_OBSERVATIONS {
            break;
        }
        if let Some(marker_start) = line.find(KERNEL_BUNDLE_KVM_MARKER_PREFIX) {
            if let Some(observation) = parse_kvm_marker(&line[marker_start..]) {
                observations.push(observation);
            }
        }
    }
    observations
}

pub fn sample_mantle_private_kfunc_kvm_markers() -> String {
    let profile = sample_mantle_private_kfunc_profile();
    expected_kvm_observations(&profile)
        .iter()
        .map(render_kvm_marker)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn expected_kernel_bundle_kvm_observations(
    profile: &KernelBundleSmokeProfile,
) -> Vec<SmokeObservation> {
    expected_kvm_observations(profile)
}

fn kvm_run_issues(
    profile: &KernelBundleSmokeProfile,
    run: &KernelBundleKvmRun,
    expected_profile_id: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    push_if(
        !run.kvm_available,
        &mut issues,
        "kvm-unavailable: KVM device or capability was unavailable",
    );
    push_if(
        !run.loader_available,
        &mut issues,
        "loader-unavailable: kernel, initrd, or guest loader input was unavailable",
    );
    push_if(
        run.profile_identity_blake3 != expected_profile_id,
        &mut issues,
        "profile-identity-mismatch: run profile identity differs from request",
    );
    push_if(
        run.execution_mode != KERNEL_BUNDLE_KVM_EXECUTION_MODE,
        &mut issues,
        "execution-mode-not-exact-kvm: transcript or wiring evidence cannot pass behavior smoke",
    );
    validate_optional_hex(
        "expected kernel image",
        &run.expected_kernel_image_blake3,
        &mut issues,
    );
    validate_optional_hex(
        "expected initrd image",
        &run.expected_initrd_image_blake3,
        &mut issues,
    );
    validate_optional_hex("kernel image", &run.kernel_image_blake3, &mut issues);
    validate_optional_hex("initrd image", &run.initrd_image_blake3, &mut issues);
    validate_image_binding(
        "kernel image",
        &run.expected_kernel_image_blake3,
        &run.kernel_image_blake3,
        run.loader_available,
        &mut issues,
    );
    validate_image_binding(
        "initrd image",
        &run.expected_initrd_image_blake3,
        &run.initrd_image_blake3,
        run.loader_available,
        &mut issues,
    );
    push_if(
        run.max_exits < MIN_KVM_MAX_EXITS || run.max_exits > MAX_KVM_MAX_EXITS,
        &mut issues,
        "bound-out-of-range: max_exits is outside supported KVM rail bounds",
    );
    push_if(
        run.exits_executed > run.max_exits,
        &mut issues,
        "bound-violation: exits_executed exceeds max_exits",
    );
    push_if(
        run.observations.len() > MAX_OBSERVATIONS,
        &mut issues,
        "observation-overflow: structured observation count exceeds bound",
    );
    if let Some(failure_class) = &run.failure_class {
        issues.push(format!("runner-failure: {failure_class}"));
    }
    if run.scenario == KernelBundleKvmScenario::Positive {
        if run.kvm_available && run.loader_available {
            for expected in expected_kvm_observations(profile) {
                push_if(
                    !run.observations.contains(&expected),
                    &mut issues,
                    format!(
                        "missing-structured-observation: case={} class={} detail={}",
                        expected.case_id, expected.class, expected.detail
                    ),
                );
            }
        }
    } else {
        push_if(
            !negative_fixture_matched(run),
            &mut issues,
            format!(
                "negative-fixture-mismatch: scenario={} did not reach its exact failure class",
                run.scenario.as_str()
            ),
        );
    }
    issues
}

fn validate_image_binding(
    label: &str,
    expected: &Option<String>,
    actual: &Option<String>,
    loader_available: bool,
    issues: &mut Vec<String>,
) {
    if !loader_available {
        return;
    }
    push_if(
        expected.is_none(),
        issues,
        format!("{label} expected digest is missing"),
    );
    push_if(
        actual.is_none(),
        issues,
        format!("{label} measured digest is missing"),
    );
    if let (Some(expected), Some(actual)) = (expected, actual) {
        push_if(
            expected != actual,
            issues,
            format!("{label} digest mismatch"),
        );
    }
}

fn negative_fixture_matched(run: &KernelBundleKvmRun) -> bool {
    match run.scenario {
        KernelBundleKvmScenario::Positive => false,
        KernelBundleKvmScenario::StaleDigest => run
            .failure_class
            .as_deref()
            .is_some_and(|failure| failure.starts_with("input-digest-mismatch:")),
        KernelBundleKvmScenario::MissingKfunc => has_error_detail(run, "missing-kfunc-rejected"),
        KernelBundleKvmScenario::VerifierRejection => has_error_detail(run, "verifier-rejected"),
        KernelBundleKvmScenario::WrongAttachTarget => {
            has_error_detail(run, "wrong-attach-target-rejected")
        }
        KernelBundleKvmScenario::CleanupFailure => has_error_detail(run, "cleanup-failed"),
    }
}

fn has_error_detail(run: &KernelBundleKvmRun, detail: &str) -> bool {
    run.observations
        .iter()
        .any(|observation| observation.class == "error" && observation.detail == detail)
}

fn kvm_status(run: &KernelBundleKvmRun, issues: &[String]) -> String {
    if !run.kvm_available || !run.loader_available {
        return "blocked".to_string();
    }
    if issues.is_empty() {
        "passed".to_string()
    } else {
        "failed".to_string()
    }
}

fn dedup_issues(issues: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    issues.retain(|issue| seen.insert(issue.clone()));
}

fn expected_kvm_observations(profile: &KernelBundleSmokeProfile) -> Vec<SmokeObservation> {
    vec![
        SmokeObservation {
            case_id: "boot".to_string(),
            class: "ready".to_string(),
            detail: profile.boot.readiness_observation.clone(),
        },
        SmokeObservation {
            case_id: "module".to_string(),
            class: "load".to_string(),
            detail: profile.module.load_observation.clone(),
        },
        SmokeObservation {
            case_id: "module".to_string(),
            class: "unload".to_string(),
            detail: profile.module.unload_observation.clone(),
        },
        SmokeObservation {
            case_id: "module".to_string(),
            class: "cleanup".to_string(),
            detail: profile.module.cleanup_class.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "verify".to_string(),
            detail: profile.bpf.verifier_observation.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "attach".to_string(),
            detail: profile.bpf.attach_observation.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "detach".to_string(),
            detail: profile.bpf.detach_observation.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "cleanup".to_string(),
            detail: profile.bpf.cleanup_class.clone(),
        },
    ]
}

fn parse_kvm_marker(line: &str) -> Option<SmokeObservation> {
    let payload = line.trim().strip_prefix(KERNEL_BUNDLE_KVM_MARKER_PREFIX)?;
    let fields = payload
        .split(';')
        .filter_map(|field| field.split_once('='))
        .map(|(key, value)| (key, trim_marker_value(value)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let case_id = fields.get("case")?.to_string();
    let class = fields.get("class")?.to_string();
    let detail = fields.get("detail")?.to_string();
    if case_id.is_empty() || class.is_empty() || detail.is_empty() {
        return None;
    }
    Some(SmokeObservation {
        case_id,
        class,
        detail,
    })
}

fn trim_marker_value(value: &str) -> &str {
    value
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
        .next()
        .unwrap_or("")
}

fn render_kvm_marker(observation: &SmokeObservation) -> String {
    format!(
        "{KERNEL_BUNDLE_KVM_MARKER_PREFIX}case={};class={};detail={}",
        observation.case_id, observation.class, observation.detail
    )
}

fn validate_profile_shape(profile: &KernelBundleSmokeProfile, issues: &mut Vec<String>) {
    push_if(
        profile.schema_version != KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION,
        issues,
        "schema version is unsupported",
    );
    push_if(
        profile.role != KERNEL_BUNDLE_SMOKE_ROLE,
        issues,
        "receipt role must be kernel-bundle/vm-compat-smoke",
    );
    push_if(
        profile.campaign_id.is_empty(),
        issues,
        "campaign_id is required",
    );
    push_if(
        profile.runner.behavior_status != "passed",
        issues,
        "runner behavior_status must be passed",
    );
    push_if(
        profile.runner.evidence_class != "disposable-vm-smoke",
        issues,
        "runner evidence_class must be disposable-vm-smoke",
    );
    validate_bound("max_boot_seconds", profile.bounds.max_boot_seconds, issues);
    validate_bound(
        "max_module_seconds",
        profile.bounds.max_module_seconds,
        issues,
    );
    validate_bound("max_bpf_seconds", profile.bounds.max_bpf_seconds, issues);
    push_if(
        profile.bounds.max_observations == 0 || profile.bounds.max_observations > MAX_OBSERVATIONS,
        issues,
        "max_observations is outside supported bounds",
    );
}

fn validate_identity_refs(profile: &KernelBundleSmokeProfile, issues: &mut Vec<String>) {
    validate_prefixed_hex(
        "kernel_build_identity",
        &profile.onix.kernel_build_identity,
        ONIX_KERNEL_BUILD_PREFIX,
        issues,
    );
    validate_prefixed_hex(
        "bundle_identity",
        &profile.onix.bundle_identity,
        ONIX_BUNDLE_PREFIX,
        issues,
    );
    validate_prefixed_hex(
        "manifest_identity",
        &profile.onix.manifest_identity,
        ONIX_MANIFEST_PREFIX,
        issues,
    );
    validate_prefixed_hex(
        "module_pack_identity",
        &profile.onix.module_pack_identity,
        ONIX_MODULE_PACK_PREFIX,
        issues,
    );
    validate_prefixed_hex(
        "bpf_pack_identity",
        &profile.onix.bpf_pack_identity,
        ONIX_BPF_PACK_PREFIX,
        issues,
    );
    validate_hex(
        "mantle observation",
        &profile.mantle.observation_blake3,
        issues,
    );
    validate_hex("mantle module", &profile.mantle.module_blake3, issues);
    validate_hex(
        "mantle bpf object",
        &profile.mantle.bpf_object_blake3,
        issues,
    );
    validate_hex(
        "runner receipt",
        &profile.runner.runner_receipt_blake3,
        issues,
    );
    validate_mantle_ref(
        "module_object_ref",
        &profile.mantle.module_object_ref,
        &profile.mantle.module_blake3,
        issues,
    );
    validate_mantle_ref(
        "bpf_object_ref",
        &profile.mantle.bpf_object_ref,
        &profile.mantle.bpf_object_blake3,
        issues,
    );
}

fn validate_cases(profile: &KernelBundleSmokeProfile, issues: &mut Vec<String>) {
    push_if(
        profile.boot.observed_architecture != profile.onix.architecture,
        issues,
        "boot architecture does not match Onix bundle",
    );
    push_if(
        profile.boot.observed_kernel_release != profile.onix.kernel_release,
        issues,
        "boot kernel release does not match Onix bundle",
    );
    push_if(
        profile.boot.readiness_observation.is_empty(),
        issues,
        "boot readiness observation is required",
    );
    push_if(
        profile.module.pack_identity != profile.onix.module_pack_identity,
        issues,
        "module case pack identity mismatch",
    );
    push_if(
        profile.module.member_blake3 != profile.mantle.module_blake3,
        issues,
        "module member digest mismatch",
    );
    push_if(
        profile.module.cleanup_class != "succeeded",
        issues,
        "module cleanup must succeed for a passing receipt",
    );
    push_if(
        profile.bpf.pack_identity != profile.onix.bpf_pack_identity,
        issues,
        "BPF case pack identity mismatch",
    );
    push_if(
        profile.bpf.object_blake3 != profile.mantle.bpf_object_blake3,
        issues,
        "BPF object digest mismatch",
    );
    push_if(
        profile.bpf.section != "xdp",
        issues,
        "only xdp section is supported for this fixture",
    );
    push_if(
        profile.bpf.attach != "xdp",
        issues,
        "only xdp attach is supported for this fixture",
    );
    push_if(
        profile.bpf.attach_target.is_empty(),
        issues,
        "BPF attach target is required",
    );
    push_if(
        profile.bpf.required_kfuncs != ["process_value".to_string()],
        issues,
        "private-kfunc fixture must require process_value",
    );
    push_if(
        profile.bpf.cleanup_class != "succeeded",
        issues,
        "BPF cleanup must succeed for a passing receipt",
    );
}

fn validate_non_claims(non_claims: &[String], issues: &mut Vec<String>) {
    push_if(
        non_claims.is_empty() || non_claims.len() > MAX_NON_CLAIMS,
        issues,
        "non_claims count is outside supported bounds",
    );
    let claims: std::collections::BTreeSet<&str> = non_claims.iter().map(String::as_str).collect();
    for required in REQUIRED_NON_CLAIMS {
        push_if(
            !claims.contains(required),
            issues,
            format!("missing non-claim: {required}"),
        );
    }
}

fn terminal_classes(
    profile: &KernelBundleSmokeProfile,
) -> std::collections::BTreeMap<String, String> {
    let mut classes = std::collections::BTreeMap::new();
    classes.insert("boot".to_string(), "ready".to_string());
    classes.insert(
        "module_load".to_string(),
        profile.module.load_observation.clone(),
    );
    classes.insert(
        "module_cleanup".to_string(),
        profile.module.cleanup_class.clone(),
    );
    classes.insert(
        "bpf_verify".to_string(),
        profile.bpf.verifier_observation.clone(),
    );
    classes.insert(
        "bpf_attach".to_string(),
        profile.bpf.attach_observation.clone(),
    );
    classes.insert("bpf_cleanup".to_string(), profile.bpf.cleanup_class.clone());
    debug_assert!(classes.len() <= MAX_OBSERVATIONS);
    debug_assert!(classes.contains_key("boot"));
    classes
}

fn smoke_observations(profile: &KernelBundleSmokeProfile) -> Vec<SmokeObservation> {
    vec![
        SmokeObservation {
            case_id: "boot".to_string(),
            class: "ready".to_string(),
            detail: profile.boot.readiness_observation.clone(),
        },
        SmokeObservation {
            case_id: "module".to_string(),
            class: "load".to_string(),
            detail: profile.module.load_observation.clone(),
        },
        SmokeObservation {
            case_id: "module".to_string(),
            class: "cleanup".to_string(),
            detail: profile.module.unload_observation.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "verify".to_string(),
            detail: profile.bpf.verifier_observation.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "attach".to_string(),
            detail: profile.bpf.attach_observation.clone(),
        },
        SmokeObservation {
            case_id: "bpf".to_string(),
            class: "cleanup".to_string(),
            detail: profile.bpf.detach_observation.clone(),
        },
    ]
}

fn domain_hash<T: Serialize>(domain: &str, value: &T) -> crate::EvidenceResult<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0]);
    hasher.update(&bytes);
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_bound(name: &str, value: u64, issues: &mut Vec<String>) {
    push_if(
        !(MIN_BOUND_SECONDS..=MAX_BOUND_SECONDS).contains(&value),
        issues,
        format!("{name} is outside supported bounds"),
    );
}

fn validate_prefixed_hex(name: &str, value: &str, prefix: &str, issues: &mut Vec<String>) {
    let Some(hex) = value.strip_prefix(prefix) else {
        issues.push(format!("{name} has invalid prefix"));
        return;
    };
    push_if(
        !lower_hex(hex),
        issues,
        format!("{name} digest is not lowercase BLAKE3 hex"),
    );
}

fn validate_optional_hex(name: &str, value: &Option<String>, issues: &mut Vec<String>) {
    if let Some(value) = value {
        validate_hex(name, value, issues);
    }
}

fn validate_hex(name: &str, value: &str, issues: &mut Vec<String>) {
    push_if(
        !lower_hex(value),
        issues,
        format!("{name} digest is not lowercase BLAKE3 hex"),
    );
}

fn validate_mantle_ref(name: &str, value: &str, expected_hex: &str, issues: &mut Vec<String>) {
    let Some(hex) = value.strip_prefix(MANTLE_BLAKE3_PREFIX) else {
        issues.push(format!("{name} has invalid Mantle object-ref prefix"));
        return;
    };
    push_if(
        hex != expected_hex,
        issues,
        format!("{name} does not match declared digest"),
    );
}

fn lower_hex(value: &str) -> bool {
    value.len() == BLAKE3_HEX_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn push_if(condition: bool, issues: &mut Vec<String>, message: impl Into<String>) {
    if condition {
        issues.push(message.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_mantle_private_kfunc_profile_emits_scoped_receipt() {
        let profile = sample_mantle_private_kfunc_profile();
        let receipt =
            kernel_bundle_smoke_receipt(&profile).expect("sample profile should validate");

        assert_eq!(receipt.role, KERNEL_BUNDLE_SMOKE_ROLE);
        assert_eq!(receipt.status, "passed");
        assert_eq!(
            receipt.profile_identity_blake3,
            EXPECTED_PRIVATE_KFUNC_PROFILE_ID
        );
        assert_eq!(
            receipt.receipt_identity_blake3,
            EXPECTED_PRIVATE_KFUNC_RECEIPT_ID
        );
        assert_eq!(
            receipt.onix.kernel_build_identity,
            profile.onix.kernel_build_identity
        );
        assert_eq!(receipt.mantle.module_blake3, profile.mantle.module_blake3);
        assert!(receipt.receipt_identity_blake3.len() == BLAKE3_HEX_LENGTH);
        assert!(receipt
            .non_claims
            .iter()
            .any(|claim| claim == "not eBPF safety proof"));
    }

    #[test]
    fn stale_or_role_confused_inputs_fail_before_receipt() {
        let mut profile = sample_mantle_private_kfunc_profile();
        profile.bpf.object_blake3 = profile.mantle.module_blake3.clone();
        let error =
            kernel_bundle_smoke_receipt(&profile).expect_err("BPF digest mismatch must fail");

        assert!(error.message().contains("BPF object digest mismatch"));
        assert!(!error.message().contains("receipt_identity_blake3"));
    }

    #[test]
    fn cleanup_and_non_claim_gaps_cannot_pass() {
        let mut profile = sample_mantle_private_kfunc_profile();
        profile.module.cleanup_class = "left-loaded".to_string();
        profile
            .non_claims
            .retain(|claim| claim != "not physical readiness");
        let error = validate_kernel_bundle_smoke_profile(&profile)
            .expect_err("cleanup/non-claim gaps must fail");

        assert!(error.message().contains("module cleanup must succeed"));
        assert!(error
            .message()
            .contains("missing non-claim: not physical readiness"));
    }

    #[test]
    fn transcript_markers_cannot_pass_exact_kvm_rail() {
        const PASSING_RUN_EXITS: u64 = DEFAULT_KVM_MAX_EXITS / 2;
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let expected_observation_count = expected_kvm_observations(&profile).len();
        let observations = extract_kvm_observations(&sample_mantle_private_kfunc_kvm_markers());
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_TRANSCRIPT_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::Positive,
            expected_kernel_image_blake3: None,
            expected_initrd_image_blake3: None,
            kernel_image_blake3: None,
            initrd_image_blake3: None,
            kvm_available: true,
            loader_available: true,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: PASSING_RUN_EXITS,
            halted: true,
            observations,
            failure_class: None,
        };

        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("classified receipt");

        assert_eq!(receipt.status, "failed");
        assert!(receipt
            .issues
            .iter()
            .any(|issue| issue.contains("execution-mode-not-exact-kvm")));
        assert_eq!(receipt.observations.len(), expected_observation_count);
        assert_eq!(receipt.receipt_identity_blake3.len(), BLAKE3_HEX_LENGTH);
        assert!(receipt
            .non_claims
            .iter()
            .any(|claim| claim == "not build correctness proof"));
    }

    #[test]
    fn unavailable_kvm_is_blocked_not_passed() {
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::Positive,
            expected_kernel_image_blake3: None,
            expected_initrd_image_blake3: None,
            kernel_image_blake3: None,
            initrd_image_blake3: None,
            kvm_available: false,
            loader_available: true,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: 0,
            halted: false,
            observations: Vec::new(),
            failure_class: Some("/dev/kvm missing".to_string()),
        };

        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("blocked receipt");

        assert_eq!(receipt.status, "blocked");
        assert!(!receipt.kvm_available);
        assert!(receipt
            .issues
            .iter()
            .any(|issue| issue.contains("kvm-unavailable")));
        assert!(receipt.observations.is_empty());
    }

    #[test]
    fn raw_log_or_missing_cleanup_cannot_pass_kvm_rail() {
        const RAW_LOG_RUN_EXITS: u64 = DEFAULT_KVM_MAX_EXITS / 4;
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let raw_log = "Linux booted\nprivate_kfunc_loader attached\nall good\n";
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_TRANSCRIPT_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::Positive,
            expected_kernel_image_blake3: None,
            expected_initrd_image_blake3: None,
            kernel_image_blake3: None,
            initrd_image_blake3: None,
            kvm_available: true,
            loader_available: true,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: RAW_LOG_RUN_EXITS,
            halted: false,
            observations: extract_kvm_observations(raw_log),
            failure_class: None,
        };

        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("failed receipt");

        assert_eq!(receipt.status, "failed");
        assert!(receipt.observations.is_empty());
        assert!(receipt
            .issues
            .iter()
            .any(|issue| issue.contains("missing-structured-observation")));
    }

    #[test]
    fn exact_kvm_positive_requires_matching_image_digests() {
        const POSITIVE_RUN_EXITS: u64 = DEFAULT_KVM_MAX_EXITS / 2;
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let digest = "a".repeat(BLAKE3_HEX_LENGTH);
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::Positive,
            expected_kernel_image_blake3: Some(digest.clone()),
            expected_initrd_image_blake3: Some(digest.clone()),
            kernel_image_blake3: Some(digest.clone()),
            initrd_image_blake3: Some(digest),
            kvm_available: true,
            loader_available: true,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: POSITIVE_RUN_EXITS,
            halted: true,
            observations: expected_kvm_observations(&profile),
            failure_class: None,
        };

        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("exact receipt");

        assert_eq!(receipt.status, "passed");
        assert!(receipt.issues.is_empty());
        assert!(!receipt.negative_fixture_matched);
        assert!(receipt.failure_class.is_none());
        assert!(kernel_bundle_receipt_supports_use(
            &receipt,
            KernelBundleEvidenceUse::VmCompatibilitySmoke
        ));
    }

    #[test]
    fn behavior_receipt_cannot_satisfy_broader_evidence_roles() {
        const POSITIVE_RUN_EXITS: u64 = DEFAULT_KVM_MAX_EXITS / 2;
        const FORBIDDEN_USE_COUNT: usize = 6;
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let digest = "e".repeat(BLAKE3_HEX_LENGTH);
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::Positive,
            expected_kernel_image_blake3: Some(digest.clone()),
            expected_initrd_image_blake3: Some(digest.clone()),
            kernel_image_blake3: Some(digest.clone()),
            initrd_image_blake3: Some(digest),
            kvm_available: true,
            loader_available: true,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: POSITIVE_RUN_EXITS,
            halted: true,
            observations: expected_kvm_observations(&profile),
            failure_class: None,
        };
        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("exact receipt");
        let forbidden = [
            KernelBundleEvidenceUse::SnapshotReplay,
            KernelBundleEvidenceUse::OnixLifecycleReplay,
            KernelBundleEvidenceUse::PhysicalReadiness,
            KernelBundleEvidenceUse::BuildCorrectness,
            KernelBundleEvidenceUse::SecurityProof,
            KernelBundleEvidenceUse::ReleaseEligibility,
        ];

        assert!(forbidden
            .iter()
            .all(|requested| !kernel_bundle_receipt_supports_use(&receipt, *requested)));
        assert_eq!(forbidden.len(), FORBIDDEN_USE_COUNT);
    }

    #[test]
    fn unsupported_boot_facts_and_bounds_fail_before_receipt() {
        let mut profile = sample_mantle_private_kfunc_profile();
        profile.boot.observed_architecture = "aarch64".to_string();
        profile.boot.observed_kernel_release = "6.18.21".to_string();
        profile.bounds.max_boot_seconds = MAX_BOUND_SECONDS.saturating_add(1);

        let error = kernel_bundle_smoke_receipt(&profile).expect_err("unsupported profile");

        assert!(error.message().contains("boot architecture"));
        assert!(error.message().contains("boot kernel release"));
        assert!(error.message().contains("max_boot_seconds"));
    }

    #[test]
    fn each_negative_scenario_requires_its_exact_failure() {
        const NEGATIVE_RUN_EXITS: u64 = DEFAULT_KVM_MAX_EXITS / 3;
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let digest = "b".repeat(BLAKE3_HEX_LENGTH);
        let cases = [
            (
                KernelBundleKvmScenario::MissingKfunc,
                "missing-kfunc-rejected",
            ),
            (
                KernelBundleKvmScenario::VerifierRejection,
                "verifier-rejected",
            ),
            (
                KernelBundleKvmScenario::WrongAttachTarget,
                "wrong-attach-target-rejected",
            ),
            (KernelBundleKvmScenario::CleanupFailure, "cleanup-failed"),
        ];
        for (scenario, detail) in cases {
            let run = KernelBundleKvmRun {
                profile_identity_blake3: profile_id.clone(),
                runner: "chaoscontrol-vmm".to_string(),
                execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
                scenario,
                expected_kernel_image_blake3: Some(digest.clone()),
                expected_initrd_image_blake3: Some(digest.clone()),
                kernel_image_blake3: Some(digest.clone()),
                initrd_image_blake3: Some(digest.clone()),
                kvm_available: true,
                loader_available: true,
                max_exits: DEFAULT_KVM_MAX_EXITS,
                exits_executed: NEGATIVE_RUN_EXITS,
                halted: false,
                observations: vec![SmokeObservation {
                    case_id: "bpf".to_string(),
                    class: "error".to_string(),
                    detail: detail.to_string(),
                }],
                failure_class: Some(format!("guest-error:bpf:{detail}")),
            };
            let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("negative receipt");

            assert_eq!(receipt.status, "failed");
            assert!(receipt.negative_fixture_matched);
            assert_eq!(receipt.scenario, scenario);
            assert!(receipt.failure_class.is_some());
        }
    }

    #[test]
    fn additional_boot_module_and_bpf_failure_classes_never_pass() {
        const FAILURE_RUN_EXITS: u64 = DEFAULT_KVM_MAX_EXITS / 5;
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let digest = "f".repeat(BLAKE3_HEX_LENGTH);
        let failures = [
            ("boot", "panic"),
            ("boot", "readiness-missing"),
            ("module", "vermagic-rejected"),
            ("module", "signature-policy-rejected"),
            ("module", "module-rejected"),
            ("module", "module-tainted"),
            ("module", "module-unload-failed"),
            ("bpf", "btf-missing"),
            ("bpf", "required-type-missing"),
            ("bpf", "expected-event-missing"),
        ];
        for (case_id, detail) in failures {
            let run = KernelBundleKvmRun {
                profile_identity_blake3: profile_id.clone(),
                runner: "chaoscontrol-vmm".to_string(),
                execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
                scenario: KernelBundleKvmScenario::Positive,
                expected_kernel_image_blake3: Some(digest.clone()),
                expected_initrd_image_blake3: Some(digest.clone()),
                kernel_image_blake3: Some(digest.clone()),
                initrd_image_blake3: Some(digest.clone()),
                kvm_available: true,
                loader_available: true,
                max_exits: DEFAULT_KVM_MAX_EXITS,
                exits_executed: FAILURE_RUN_EXITS,
                halted: false,
                observations: vec![SmokeObservation {
                    case_id: case_id.to_string(),
                    class: "error".to_string(),
                    detail: detail.to_string(),
                }],
                failure_class: Some(format!("guest-error:{case_id}:{detail}")),
            };
            let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("failure receipt");

            assert_eq!(receipt.status, "failed");
            assert!(!receipt.negative_fixture_matched);
            assert!(receipt
                .failure_class
                .as_deref()
                .is_some_and(|failure| failure.contains(detail)));
        }
    }

    #[test]
    fn vm_exit_bound_violation_never_passes() {
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let digest = "1".repeat(BLAKE3_HEX_LENGTH);
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::Positive,
            expected_kernel_image_blake3: Some(digest.clone()),
            expected_initrd_image_blake3: Some(digest.clone()),
            kernel_image_blake3: Some(digest.clone()),
            initrd_image_blake3: Some(digest),
            kvm_available: true,
            loader_available: true,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: DEFAULT_KVM_MAX_EXITS.saturating_add(1),
            halted: false,
            observations: Vec::new(),
            failure_class: Some("vm-run:bound-exceeded".to_string()),
        };

        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("bound receipt");

        assert_eq!(receipt.status, "failed");
        assert!(receipt
            .issues
            .iter()
            .any(|issue| issue.contains("bound-violation")));
        assert!(receipt.terminal_classes.is_empty());
    }

    #[test]
    fn stale_digest_is_blocked_before_vmm_and_bound_to_expected_and_actual() {
        let profile = sample_mantle_private_kfunc_profile();
        let profile_id = kernel_bundle_smoke_profile_identity(&profile).expect("hash profile");
        let expected = "c".repeat(BLAKE3_HEX_LENGTH);
        let actual = "d".repeat(BLAKE3_HEX_LENGTH);
        let run = KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: "chaoscontrol-vmm".to_string(),
            execution_mode: KERNEL_BUNDLE_KVM_EXECUTION_MODE.to_string(),
            scenario: KernelBundleKvmScenario::StaleDigest,
            expected_kernel_image_blake3: Some(expected.clone()),
            expected_initrd_image_blake3: Some(actual.clone()),
            kernel_image_blake3: Some(actual.clone()),
            initrd_image_blake3: Some(actual),
            kvm_available: true,
            loader_available: false,
            max_exits: DEFAULT_KVM_MAX_EXITS,
            exits_executed: 0,
            halted: false,
            observations: Vec::new(),
            failure_class: Some(format!("input-digest-mismatch:kernel:expected={expected}")),
        };

        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run).expect("blocked receipt");

        assert_eq!(receipt.status, "blocked");
        assert!(receipt.negative_fixture_matched);
        assert_eq!(receipt.expected_kernel_image_blake3, Some(expected));
        assert!(receipt.observations.is_empty());
    }
}
