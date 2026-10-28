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
    let bytes = read_topology(&topology)?;
    let topology: OciTopology =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if let Some(path) = manifest_output {
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
        let mut bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        chaoscontrol_evidence::bounded_file::write_new_bytes(&path, &bytes)
            .map_err(|error| error.message().to_string())?;
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

fn read_topology(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_TOPOLOGY_BYTES {
        return Err("topology must be a bounded regular file".to_string());
    }
    let mut bytes = Vec::new();
    file.take(MAX_TOPOLOGY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_TOPOLOGY_BYTES {
        return Err("topology must be a bounded regular file".to_string());
    }
    if bytes.len() as u64 != metadata.len() {
        return Err("topology changed while reading".to_string());
    }
    chaoscontrol_evidence::preflight_json_document(&String::from_utf8_lossy(&bytes))
        .map_err(|error| error.message().to_string())?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn topology_reads_reject_length_changes() {
        assert!(read_topology(std::path::Path::new("/proc/self/cmdline")).is_err());
    }

    #[test]
    fn topology_reads_reject_excessive_nesting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("topology.json");
        std::fs::write(&path, format!("{}{}", "[".repeat(512), "]".repeat(512))).unwrap();
        assert!(read_topology(&path).is_err());
    }

    #[test]
    fn topology_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        std::fs::write(&target, b"{}").unwrap();
        let link = root.path().join("topology.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_topology(&link).is_err());

        let fifo = root.path().join("topology.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_topology(&fifo).is_err());
    }
}
