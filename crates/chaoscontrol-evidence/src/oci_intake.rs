use bounded_tree_cap::{execute as execute_tree_copy, prepare as prepare_tree};
use bounded_tree_core::{EntryKind, LimitValues, SymlinkPolicy, TreeLimits, TreePlan};
use cap_std::{ambient_authority, fs::Dir};
use chaoscontrol_protocol::guest_process::{ProcessManifest, PROCESS_MANIFEST_SCHEMA};
use chaoscontrol_protocol::oci_intake::{
    image_identity_from_layers, lower_topology, validate_receipt, ImageFormat, IntakeReceipt,
    IntakeReceiptError, OciTopology, ServiceBundlePlan, ServiceIntakeReceipt, OCI_CLAIM_SCOPE,
    OCI_INTAKE_RECEIPT_SCHEMA,
};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

const KIBIBYTE: u64 = 1024;
const MEBIBYTE: u64 = KIBIBYTE * KIBIBYTE;
const GIBIBYTE: u64 = KIBIBYTE * MEBIBYTE;
const MAX_TREE_ENTRIES: u64 = 8192;
const MAX_TREE_DEPTH: u64 = 64;
const MAX_TREE_PATH_BYTES: u64 = 4096;
const MAX_FILE_BYTES: u64 = 128 * MEBIBYTE;
const MAX_TREE_BYTES: u64 = GIBIBYTE;
const MAX_SYMLINK_TARGET_BYTES: u64 = 4096;
const MAX_ARCHIVE_ENTRIES: u64 = MAX_TREE_ENTRIES;
const TREE_HASH_DOMAIN: &[u8] = b"chaoscontrol.oci-tree.v1\0";
const FILE_HASH_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub enum IntakeShellError {
    Plan(String),
    Io(std::io::Error),
    SourceIdentityMismatch { path: PathBuf },
    LayerIdentityMismatch { path: PathBuf },
    UnsupportedArchiveEntry { path: PathBuf },
    ArchiveLimit,
    UnsafePath,
    OutputExists,
    Receipt(IntakeReceiptError),
}

impl From<std::io::Error> for IntakeShellError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn materialize_bundle(
    topology: &OciTopology,
    output: &Path,
) -> Result<IntakeReceipt, IntakeShellError> {
    if output.exists() {
        return Err(IntakeShellError::OutputExists);
    }
    let plan =
        lower_topology(topology).map_err(|error| IntakeShellError::Plan(format!("{error:?}")))?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    let stage_root = staging.path();
    let mut services = Vec::with_capacity(plan.services.len());
    for service in &plan.services {
        let destination = stage_root.join("services").join(&service.role).join("root");
        std::fs::create_dir_all(&destination)?;
        materialize_service(service, &destination)?;
        let root_identity = observe_tree_identity(&destination)?;
        services.push(ServiceIntakeReceipt {
            role: service.role.clone(),
            image_identity: service.image_identity.clone(),
            layer_identities: service
                .layers
                .iter()
                .map(|layer| layer.identity.clone())
                .collect(),
            root_identity,
        });
    }
    let process_manifest = ProcessManifest {
        schema: PROCESS_MANIFEST_SCHEMA.to_string(),
        guest: plan.process_manifest.guest.clone(),
        shared_directories: plan.process_manifest.shared_directories.clone(),
        processes: plan
            .process_manifest
            .processes
            .iter()
            .map(|process| process.spec.clone())
            .collect(),
    };
    write_json(stage_root.join("process-manifest.json"), &process_manifest)?;
    write_json(stage_root.join("bundle-plan.json"), &plan)?;
    let receipt = IntakeReceipt {
        schema: OCI_INTAKE_RECEIPT_SCHEMA.to_string(),
        bundle_identity: plan.bundle_identity.clone(),
        process_manifest_identity: plan.process_manifest.manifest_identity.clone(),
        services,
        claim_scope: OCI_CLAIM_SCOPE.to_string(),
    };
    validate_receipt(&plan, &receipt).map_err(IntakeShellError::Receipt)?;
    write_json(stage_root.join("receipt.json"), &receipt)?;
    sync_tree_root(stage_root)?;
    let stage_path = staging.keep();
    std::fs::rename(stage_path, output)?;
    sync_tree_root(parent)?;
    Ok(receipt)
}

fn materialize_service(
    service: &ServiceBundlePlan,
    destination: &Path,
) -> Result<(), IntakeShellError> {
    match service.format {
        ImageFormat::Directory => {
            let source = Path::new(&service.source_path);
            let actual_identity = observe_tree_identity(source)?;
            if actual_identity != service.image_identity {
                return Err(IntakeShellError::SourceIdentityMismatch {
                    path: source.to_path_buf(),
                });
            }
            copy_bounded_tree(source, destination)?;
        }
        ImageFormat::TarArchive => {
            let source = Path::new(&service.source_path);
            if file_identity(source)? != service.image_identity {
                return Err(IntakeShellError::SourceIdentityMismatch {
                    path: source.to_path_buf(),
                });
            }
            extract_tar(source, destination)?;
        }
        ImageFormat::OciLayers => {
            if image_identity_from_layers(&service.layers) != service.image_identity {
                return Err(IntakeShellError::SourceIdentityMismatch {
                    path: PathBuf::from(&service.source_path),
                });
            }
            let source_root = Path::new(&service.source_path);
            for layer in &service.layers {
                let source = source_root.join(&layer.path);
                if file_identity(&source)? != layer.identity {
                    return Err(IntakeShellError::LayerIdentityMismatch { path: source });
                }
                extract_tar(&source, destination)?;
            }
        }
    }
    Ok(())
}

fn tree_limits() -> Result<TreeLimits, IntakeShellError> {
    TreeLimits::new(LimitValues {
        entries: MAX_TREE_ENTRIES,
        depth: MAX_TREE_DEPTH,
        path_bytes: MAX_TREE_PATH_BYTES,
        file_bytes: MAX_FILE_BYTES,
        total_bytes: MAX_TREE_BYTES,
        symlink_target_bytes: MAX_SYMLINK_TARGET_BYTES,
    })
    .map_err(|error| IntakeShellError::Plan(format!("invalid tree limits: {error:?}")))
}

fn prepare(source: &Path) -> Result<bounded_tree_cap::PreparedTree, IntakeShellError> {
    let source_dir = Dir::open_ambient_dir(source, ambient_authority())?;
    prepare_tree(&source_dir, tree_limits()?, SymlinkPolicy::PreserveInternal)
        .map_err(|error| IntakeShellError::Plan(format!("tree admission failed: {error:?}")))
}

fn copy_bounded_tree(source: &Path, destination: &Path) -> Result<(), IntakeShellError> {
    let prepared = prepare(source)?;
    let destination = Dir::open_ambient_dir(destination, ambient_authority())?;
    execute_tree_copy(&prepared, &destination)
        .map_err(|error| IntakeShellError::Plan(format!("tree copy failed: {error:?}")))?;
    Ok(())
}

pub fn observe_tree_identity(source: &Path) -> Result<String, IntakeShellError> {
    let prepared = prepare(source)?;
    Ok(tree_plan_identity(prepared.plan()))
}

fn tree_plan_identity(plan: &TreePlan) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TREE_HASH_DOMAIN);
    for member in plan.member_facts() {
        for component in member.path().components() {
            hash_part(&mut hasher, component);
        }
        let kind = match member.kind() {
            EntryKind::Directory => 0_u8,
            EntryKind::File => 1_u8,
            EntryKind::Symlink => 2_u8,
            EntryKind::Unsupported => 3_u8,
        };
        hasher.update(&[kind]);
        hasher.update(&member.mode().bits().to_le_bytes());
        hasher.update(&member.file_bytes().to_le_bytes());
        if let Some(target) = member.symlink_target() {
            hash_part(&mut hasher, target);
        } else {
            hash_part(&mut hasher, &[]);
        }
        if let Some(content) = member.file_content() {
            hash_part(&mut hasher, content.as_bytes());
        } else {
            hash_part(&mut hasher, &[]);
        }
    }
    format!("b3:{}", hasher.finalize().to_hex())
}

fn hash_part(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn file_identity(path: &Path) -> Result<String, IntakeShellError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_TREE_BYTES {
        return Err(IntakeShellError::UnsafePath);
    }
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; FILE_HASH_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("b3:{}", hasher.finalize().to_hex()))
}

fn extract_tar(source: &Path, destination: &Path) -> Result<(), IntakeShellError> {
    let file = File::open(source)?;
    let mut archive = tar::Archive::new(file);
    let mut entries_seen = 0_u64;
    let mut bytes_seen = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        entries_seen = entries_seen
            .checked_add(1)
            .ok_or(IntakeShellError::ArchiveLimit)?;
        if entries_seen > MAX_ARCHIVE_ENTRIES {
            return Err(IntakeShellError::ArchiveLimit);
        }
        let size = entry.size();
        if size > MAX_FILE_BYTES {
            return Err(IntakeShellError::ArchiveLimit);
        }
        bytes_seen = bytes_seen
            .checked_add(size)
            .ok_or(IntakeShellError::ArchiveLimit)?;
        if bytes_seen > MAX_TREE_BYTES {
            return Err(IntakeShellError::ArchiveLimit);
        }
        let relative = safe_archive_path(&entry.path()?)?;
        if apply_whiteout(destination, &relative)? {
            continue;
        }
        let output = destination.join(&relative);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            replace_non_directory(&output)?;
            std::fs::create_dir_all(&output)?;
        } else if entry_type.is_file() {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            replace_directory(&output)?;
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&output)?;
            std::io::copy(&mut entry, &mut file)?;
            file.sync_all()?;
        } else {
            return Err(IntakeShellError::UnsupportedArchiveEntry { path: relative });
        }
        if let Ok(mode) = entry.header().mode() {
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn safe_archive_path(path: &Path) -> Result<PathBuf, IntakeShellError> {
    if path.as_os_str().as_bytes().len() > MAX_TREE_PATH_BYTES as usize {
        return Err(IntakeShellError::UnsafePath);
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => result.push(value),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(IntakeShellError::UnsafePath);
            }
        }
    }
    if result.as_os_str().is_empty() {
        return Err(IntakeShellError::UnsafePath);
    }
    Ok(result)
}

fn apply_whiteout(destination: &Path, relative: &Path) -> Result<bool, IntakeShellError> {
    let Some(name) = relative.file_name().and_then(|name| name.to_str()) else {
        return Err(IntakeShellError::UnsafePath);
    };
    if name == ".wh..wh..opq" {
        let directory = relative.parent().unwrap_or_else(|| Path::new(""));
        let target = destination.join(directory);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::create_dir_all(target)?;
        return Ok(true);
    }
    let Some(removed_name) = name.strip_prefix(".wh.") else {
        return Ok(false);
    };
    if removed_name.is_empty() {
        return Err(IntakeShellError::UnsafePath);
    }
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let target = destination.join(parent).join(removed_name);
    remove_any(&target)?;
    Ok(true)
}

fn replace_non_directory(path: &Path) -> Result<(), IntakeShellError> {
    if path.exists() && !path.is_dir() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn replace_directory(path: &Path) -> Result<(), IntakeShellError> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn remove_any(path: &Path) -> Result<(), IntakeShellError> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn write_json(path: PathBuf, value: &impl serde::Serialize) -> Result<(), IntakeShellError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| IntakeShellError::Plan(error.to_string()))?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn sync_tree_root(path: &Path) -> Result<(), IntakeShellError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_protocol::guest_process::{RestartMode, RestartPolicy};
    use chaoscontrol_protocol::oci_intake::{
        ImageService, ImageSource, LayerDescriptor, OCI_TOPOLOGY_SCHEMA,
    };
    use std::collections::BTreeMap;

    const RESTART_LIMIT: u32 = 2;

    fn make_tar(path: &Path, member: &str, bytes: &[u8]) {
        let file = File::create(path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, member, bytes).unwrap();
        builder.finish().unwrap();
    }

    fn service(role: &str, source: ImageSource, entrypoint: &str) -> ImageService {
        ImageService {
            role: role.to_string(),
            source,
            entrypoint: entrypoint.to_string(),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            shared_directories: Vec::new(),
            restart: RestartPolicy {
                mode: RestartMode::Never,
                max_restarts: RESTART_LIMIT,
            },
            instrumented: false,
            transport_slot: None,
        }
    }

    #[test]
    fn multi_image_topology_materializes_oci_and_directory_roots() {
        let fixture = tempfile::tempdir().unwrap();
        let image = fixture.path().join("oci");
        let directory = fixture.path().join("directory");
        std::fs::create_dir_all(&image).unwrap();
        std::fs::create_dir_all(directory.join("bin")).unwrap();
        std::fs::write(directory.join("bin/sidecar"), b"sidecar").unwrap();
        let layer = image.join("layer-0.tar");
        make_tar(&layer, "bin/database", b"database");
        let layer_descriptor = LayerDescriptor {
            path: "layer-0.tar".to_string(),
            identity: file_identity(&layer).unwrap(),
        };
        let topology = OciTopology {
            schema: OCI_TOPOLOGY_SCHEMA.to_string(),
            topology_id: "multi-image".to_string(),
            shared_directories: Vec::new(),
            services: vec![
                service(
                    "database",
                    ImageSource {
                        format: "oci_layers".to_string(),
                        path: image.display().to_string(),
                        image_identity: image_identity_from_layers(std::slice::from_ref(
                            &layer_descriptor,
                        )),
                        layers: vec![layer_descriptor],
                    },
                    "bin/database",
                ),
                service(
                    "sidecar",
                    ImageSource {
                        format: "directory".to_string(),
                        path: directory.display().to_string(),
                        image_identity: observe_tree_identity(&directory).unwrap(),
                        layers: Vec::new(),
                    },
                    "bin/sidecar",
                ),
            ],
        };
        let output = fixture.path().join("bundle");
        let receipt = materialize_bundle(&topology, &output).unwrap();
        assert_eq!(receipt.services.len(), 2);
        assert_eq!(
            std::fs::read(output.join("services/database/root/bin/database")).unwrap(),
            b"database"
        );
        assert_eq!(
            std::fs::read(output.join("services/sidecar/root/bin/sidecar")).unwrap(),
            b"sidecar"
        );
    }

    #[test]
    fn wrong_layer_identity_and_existing_output_fail_closed() {
        let fixture = tempfile::tempdir().unwrap();
        let image = fixture.path().join("oci");
        std::fs::create_dir_all(&image).unwrap();
        let layer = image.join("layer.tar");
        make_tar(&layer, "bin/service", b"service");
        let wrong = LayerDescriptor {
            path: "layer.tar".to_string(),
            identity: format!("b3:{}", "0".repeat(64)),
        };
        let topology = OciTopology {
            schema: OCI_TOPOLOGY_SCHEMA.to_string(),
            topology_id: "negative".to_string(),
            shared_directories: Vec::new(),
            services: vec![service(
                "service",
                ImageSource {
                    format: "oci_layers".to_string(),
                    path: image.display().to_string(),
                    image_identity: image_identity_from_layers(std::slice::from_ref(&wrong)),
                    layers: vec![wrong],
                },
                "bin/service",
            )],
        };
        assert!(matches!(
            materialize_bundle(&topology, &fixture.path().join("bundle")),
            Err(IntakeShellError::LayerIdentityMismatch { .. })
        ));
        let existing = fixture.path().join("existing");
        std::fs::create_dir(&existing).unwrap();
        assert!(matches!(
            materialize_bundle(&topology, &existing),
            Err(IntakeShellError::OutputExists)
        ));
    }
}
