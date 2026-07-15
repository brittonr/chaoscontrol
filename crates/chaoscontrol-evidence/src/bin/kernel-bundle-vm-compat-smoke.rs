use std::fs;
use std::path::Path;

use chaoscontrol_evidence::{
    kernel_bundle_smoke_receipt, sample_mantle_private_kfunc_profile,
    validate_kernel_bundle_smoke_profile, EvidenceError, EvidenceResult, KernelBundleSmokeProfile,
};

const ARG_SAMPLE_PROFILE: &str = "--sample-profile";
const ARG_SAMPLE_RECEIPT: &str = "--sample-receipt";
const ARG_CHECK_PROFILE: &str = "--check-profile";
const ARG_HELP_SHORT: &str = "-h";
const ARG_HELP_LONG: &str = "--help";
const EXPECTED_CHECK_ARG_COUNT: usize = 3;
const EXPECTED_SINGLE_ARG_COUNT: usize = 2;

fn main() {
    if let Err(error) = run(std::env::args().collect()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> EvidenceResult<()> {
    if args.len() == EXPECTED_SINGLE_ARG_COUNT && args[1] == ARG_SAMPLE_PROFILE {
        print_json(&sample_mantle_private_kfunc_profile())?;
        return Ok(());
    }
    if args.len() == EXPECTED_SINGLE_ARG_COUNT && args[1] == ARG_SAMPLE_RECEIPT {
        let receipt = kernel_bundle_smoke_receipt(&sample_mantle_private_kfunc_profile())?;
        print_json(&receipt)?;
        return Ok(());
    }
    if args.len() == EXPECTED_CHECK_ARG_COUNT && args[1] == ARG_CHECK_PROFILE {
        let profile = read_profile(Path::new(&args[2]))?;
        validate_kernel_bundle_smoke_profile(&profile)?;
        let receipt = kernel_bundle_smoke_receipt(&profile)?;
        print_json(&receipt)?;
        return Ok(());
    }
    if args.len() == EXPECTED_SINGLE_ARG_COUNT
        && matches!(args[1].as_str(), ARG_HELP_SHORT | ARG_HELP_LONG)
    {
        print_usage();
        return Ok(());
    }
    Err(EvidenceError::new(
        "usage error: expected --sample-profile, --sample-receipt, or --check-profile <path>",
    ))
}

fn read_profile(path: &Path) -> EvidenceResult<KernelBundleSmokeProfile> {
    let text = fs::read_to_string(path)?;
    let profile = serde_json::from_str(&text)?;
    Ok(profile)
}

fn print_json<T: serde::Serialize>(value: &T) -> EvidenceResult<()> {
    let rendered = serde_json::to_string_pretty(value)?;
    println!("{rendered}");
    Ok(())
}

fn print_usage() {
    println!("kernel-bundle-vm-compat-smoke --sample-profile");
    println!("kernel-bundle-vm-compat-smoke --sample-receipt");
    println!("kernel-bundle-vm-compat-smoke --check-profile <profile.json>");
}
