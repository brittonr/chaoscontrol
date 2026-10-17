use chaoscontrol_evidence::oci_intake::{materialize_bundle, verify_materialized_bundle};
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
    let mut verify_bundle = None;
    let mut manifest_output = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--topology" => topology = arguments.next().map(PathBuf::from),
            "--output" => output = arguments.next().map(PathBuf::from),
            "--verify-bundle" => verify_bundle = arguments.next().map(PathBuf::from),
            "--manifest-output" => manifest_output = arguments.next().map(PathBuf::from),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    let topology = topology.ok_or_else(|| "--topology is required".to_string())?;
    if usize::from(output.is_some())
        + usize::from(verify_bundle.is_some())
        + usize::from(manifest_output.is_some())
        != 1
    {
        return Err(
            "exactly one of --output, --verify-bundle or --manifest-output is required".to_string(),
        );
    }
    let metadata = std::fs::symlink_metadata(&topology).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_TOPOLOGY_BYTES {
        return Err("topology must be a bounded regular file".to_string());
    }
    let bytes = std::fs::read(&topology).map_err(|error| error.to_string())?;
    let topology: OciTopology =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if let Some(path) = manifest_output {
        use std::io::Write;
        let plan = chaoscontrol_protocol::oci_intake::lower_topology(&topology)
            .map_err(|error| format!("{error:?}"))?;
        let manifest = chaoscontrol_protocol::guest_process::ProcessManifest {
            schema: chaoscontrol_protocol::guest_process::PROCESS_MANIFEST_SCHEMA.to_string(),
            guest: plan.process_manifest.guest,
            shared_directories: plan.process_manifest.shared_directories,
            processes: plan
                .process_manifest
                .processes
                .into_iter()
                .map(|process| process.spec)
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        println!(
            "manifest={} scope=image-to-guest-bundle-only",
            plan.process_manifest.manifest_identity
        );
        return Ok(());
    }
    let receipt = match (output, verify_bundle) {
        (Some(output), None) => materialize_bundle(&topology, &output),
        (None, Some(bundle)) => verify_materialized_bundle(&topology, &bundle),
        _ => unreachable!("exclusive mode checked above"),
    }
    .map_err(|error| format!("{error:?}"))?;
    println!(
        "bundle={} services={} scope={}",
        receipt.bundle_identity,
        receipt.services.len(),
        receipt.claim_scope
    );
    Ok(())
}
