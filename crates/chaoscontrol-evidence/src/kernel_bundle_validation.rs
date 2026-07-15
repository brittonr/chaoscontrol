use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EvidenceError, EvidenceResult};

pub const KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION: u64 = 1;
pub const KERNEL_BUNDLE_SMOKE_ROLE: &str = "kernel-bundle/vm-compat-smoke";
pub const KERNEL_BUNDLE_SMOKE_SCOPE: &str = "bounded disposable-VM compatibility smoke for one exact kernel-bundle cohort; not universal bootability, not module safety proof, not eBPF safety proof, not build correctness proof, not physical readiness";
const RECEIPT_DOMAIN: &str = "chaoscontrol/kernel-bundle/vm-compat-smoke/receipt/v1";
const PROFILE_DOMAIN: &str = "chaoscontrol/kernel-bundle/vm-compat-smoke/profile/v1";
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OnixKernelBundleRefs {
    pub architecture: String,
    pub kernel_release: String,
    pub kernel_build_identity: String,
    pub bundle_identity: String,
    pub manifest_identity: String,
    pub module_pack_identity: String,
    pub bpf_pack_identity: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MantleMaterializationRefs {
    pub observation_blake3: String,
    pub module_blake3: String,
    pub module_object_ref: String,
    pub bpf_object_blake3: String,
    pub bpf_object_ref: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SmokeRunnerEvidence {
    pub runner: String,
    pub runner_receipt_blake3: String,
    pub behavior_status: String,
    pub evidence_class: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BootCase {
    pub observed_architecture: String,
    pub observed_kernel_release: String,
    pub readiness_observation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ModuleCase {
    pub pack_identity: String,
    pub member_path: String,
    pub member_blake3: String,
    pub load_observation: String,
    pub unload_observation: String,
    pub cleanup_class: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SmokeBounds {
    pub max_boot_seconds: u64,
    pub max_module_seconds: u64,
    pub max_bpf_seconds: u64,
    pub max_observations: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct KernelBundleSmokeReceipt {
    pub schema_version: u64,
    pub role: String,
    pub campaign_id: String,
    pub status: String,
    pub profile_identity_blake3: String,
    pub onix: OnixKernelBundleRefs,
    pub mantle: MantleMaterializationRefs,
    pub runner: SmokeRunnerEvidence,
    pub terminal_classes: BTreeMap<String, String>,
    pub observations: Vec<SmokeObservation>,
    pub bounds: SmokeBounds,
    pub non_claims: Vec<String>,
    pub receipt_identity_blake3: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SmokeObservation {
    pub case_id: String,
    pub class: String,
    pub detail: String,
}

pub fn validate_kernel_bundle_smoke_profile(
    profile: &KernelBundleSmokeProfile,
) -> EvidenceResult<()> {
    let mut issues = Vec::new();
    validate_profile_shape(profile, &mut issues);
    validate_identity_refs(profile, &mut issues);
    validate_cases(profile, &mut issues);
    validate_non_claims(&profile.non_claims, &mut issues);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(EvidenceError::new(issues.join("; ")))
    }
}

pub fn kernel_bundle_smoke_receipt(
    profile: &KernelBundleSmokeProfile,
) -> EvidenceResult<KernelBundleSmokeReceipt> {
    validate_kernel_bundle_smoke_profile(profile)?;
    let profile_identity_blake3 = domain_hash(PROFILE_DOMAIN, profile)?;
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

pub fn sample_mantle_private_kfunc_receipt() -> EvidenceResult<KernelBundleSmokeReceipt> {
    kernel_bundle_smoke_receipt(&sample_mantle_private_kfunc_profile())
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
    let claims: BTreeSet<&str> = non_claims.iter().map(String::as_str).collect();
    for required in REQUIRED_NON_CLAIMS {
        push_if(
            !claims.contains(required),
            issues,
            format!("missing non-claim: {required}"),
        );
    }
}

fn terminal_classes(profile: &KernelBundleSmokeProfile) -> BTreeMap<String, String> {
    let mut classes = BTreeMap::new();
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

fn domain_hash<T: Serialize>(domain: &str, value: &T) -> EvidenceResult<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0]);
    hasher.update(&bytes);
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_bound(name: &str, value: u64, issues: &mut Vec<String>) {
    push_if(
        value < MIN_BOUND_SECONDS || value > MAX_BOUND_SECONDS,
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
}
