//! Standalone retained-corpus regeneration verifier; not VM or runtime replay.
use chaoscontrol_wasm_differential::{rail::CaseManifest, validate_profile, DifferentialProfile};
use std::{fs, path::Path};

fn bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > maximum {
        return Err(format!(
            "input outside file/byte boundary: {}",
            path.display()
        ));
    }
    fs::read(path).map_err(|error| error.to_string())
}
fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err("usage: mvp_corpus_replay PROFILE CASE_MANIFEST MODULE".into());
    }
    let profile: DifferentialProfile =
        serde_json::from_slice(&bounded(Path::new(&arguments[0]), 1_048_576)?)
            .map_err(|error| error.to_string())?;
    validate_profile(&profile)?;
    let manifest: CaseManifest = serde_json::from_slice(&bounded(
        Path::new(&arguments[1]),
        profile.bounds.maximum_output_bytes,
    )?)
    .map_err(|error| error.to_string())?;
    let bytes = bounded(Path::new(&arguments[2]), profile.bounds.maximum_input_bytes)?;
    manifest.verify(&profile, &bytes)?;
    println!(
        "corpus-replay-match profile={} module={} expected={:?} schedules={}",
        manifest.profile_identity,
        manifest.output_blake3,
        manifest.expected_class,
        manifest.schedules.len()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("corpus-replay-mismatch: {error}");
        std::process::exit(1);
    }
}
