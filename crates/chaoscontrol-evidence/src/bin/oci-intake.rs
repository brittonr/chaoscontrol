use chaoscontrol_evidence::oci_intake::materialize_bundle;
use chaoscontrol_protocol::oci_intake::OciTopology;
use std::path::PathBuf;

const MAX_TOPOLOGY_BYTES: u64 = 1024 * 1024;

fn main() {
    if let Err(error) = run() {
        eprintln!("OCI intake failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut topology = None;
    let mut output = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--topology" => topology = arguments.next().map(PathBuf::from),
            "--output" => output = arguments.next().map(PathBuf::from),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    let topology = topology.ok_or_else(|| "--topology is required".to_string())?;
    let output = output.ok_or_else(|| "--output is required".to_string())?;
    let metadata = std::fs::symlink_metadata(&topology).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_TOPOLOGY_BYTES {
        return Err("topology must be a bounded regular file".to_string());
    }
    let bytes = std::fs::read(&topology).map_err(|error| error.to_string())?;
    let topology: OciTopology =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let receipt = materialize_bundle(&topology, &output).map_err(|error| format!("{error:?}"))?;
    println!(
        "bundle={} services={} scope={}",
        receipt.bundle_identity,
        receipt.services.len(),
        receipt.claim_scope
    );
    Ok(())
}
