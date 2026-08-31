use chaoscontrol_evidence::guest_determinism::{
    run_guest_determinism_gate, write_guest_determinism_report,
};
use std::path::PathBuf;

const EXIT_USAGE: i32 = 64;
const EXIT_DRIFT: i32 = 2;
const EXIT_ERROR: i32 = 1;

fn main() {
    let mut arguments = std::env::args();
    let _program = arguments.next();
    let Some(kernel) = arguments.next() else {
        usage();
    };
    let Some(initrd) = arguments.next() else {
        usage();
    };
    let Some(receipt) = arguments.next() else {
        usage();
    };
    let Some(run_seed) = arguments.next() else {
        usage();
    };
    if arguments.next().is_some() {
        usage();
    }
    let kernel = PathBuf::from(kernel);
    let initrd = PathBuf::from(initrd);
    let receipt = PathBuf::from(receipt);
    let run_seed = match run_seed.parse::<u64>() {
        Ok(seed) => seed,
        Err(error) => {
            eprintln!("invalid run seed: {error}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let report = match run_guest_determinism_gate(&kernel, &initrd, run_seed) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("guest determinism gate failed: {error:?}");
            std::process::exit(EXIT_ERROR);
        }
    };
    if let Err(error) = write_guest_determinism_report(&receipt, &report) {
        eprintln!("guest determinism receipt failed: {error:?}");
        std::process::exit(EXIT_ERROR);
    }
    if !report.accepted {
        eprintln!("guest determinism drift: {:?}", report.drifted_surfaces);
        std::process::exit(EXIT_DRIFT);
    }
    println!("guest determinism gate passed: {}", report.profile_id);
}

fn usage() -> ! {
    eprintln!("usage: guest-determinism-gate <kernel> <initrd> <receipt.json> <run-seed>");
    std::process::exit(EXIT_USAGE);
}
