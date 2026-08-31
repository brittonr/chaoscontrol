//! Pure OCI and directory intake planning with bounded provenance identities.

use crate::guest_process::{
    admit_manifest, AdmittedManifest, ProcessManifest, ProcessSpec, RestartPolicy,
    SharedDirectorySpec, PROCESS_MANIFEST_SCHEMA,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const OCI_TOPOLOGY_SCHEMA: &str = "chaoscontrol.oci-topology.v1";
pub const OCI_BUNDLE_PLAN_SCHEMA: &str = "chaoscontrol.oci-bundle-plan.v1";
pub const OCI_INTAKE_RECEIPT_SCHEMA: &str = "chaoscontrol.oci-intake-receipt.v1";
pub const OCI_CLAIM_SCOPE: &str = "image-to-guest-bundle-only";
pub const MAX_SERVICES: usize = 32;
pub const MAX_LAYERS_PER_SERVICE: usize = 64;
pub const MAX_SOURCE_PATH_BYTES: usize = 1024;
const HASH_DOMAIN: &[u8] = b"chaoscontrol.oci-intake.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    OciLayers,
    Directory,
    TarArchive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDescriptor {
    pub path: String,
    pub identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSource {
    pub format: String,
    pub path: String,
    pub image_identity: String,
    pub layers: Vec<LayerDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageService {
    pub role: String,
    pub source: ImageSource,
    pub entrypoint: String,
    pub arguments: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
    pub shared_directories: Vec<String>,
    pub restart: RestartPolicy,
    pub instrumented: bool,
    pub transport_slot: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OciTopology {
    pub schema: String,
    pub topology_id: String,
    pub shared_directories: Vec<SharedDirectorySpec>,
    pub services: Vec<ImageService>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceBundlePlan {
    pub role: String,
    pub format: ImageFormat,
    pub source_path: String,
    pub image_identity: String,
    pub layers: Vec<LayerDescriptor>,
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundlePlan {
    pub schema: String,
    pub topology_id: String,
    pub bundle_identity: String,
    pub process_manifest: AdmittedManifest,
    pub services: Vec<ServiceBundlePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntakePlanError {
    InvalidSchema,
    InvalidTopologyId,
    ServiceLimit,
    DuplicateRole,
    InvalidSourcePath,
    InvalidEntrypoint,
    InvalidIdentity,
    LayerLimit,
    InvalidLayer,
    UnsupportedFormat(String),
    LayersNotAllowed,
    MissingLayers,
    Manifest(crate::guest_process::ManifestError),
}

pub fn lower_topology(topology: &OciTopology) -> Result<BundlePlan, IntakePlanError> {
    if topology.schema != OCI_TOPOLOGY_SCHEMA {
        return Err(IntakePlanError::InvalidSchema);
    }
    validate_token(&topology.topology_id).map_err(|()| IntakePlanError::InvalidTopologyId)?;
    if topology.services.is_empty() || topology.services.len() > MAX_SERVICES {
        return Err(IntakePlanError::ServiceLimit);
    }
    let mut roles = BTreeSet::new();
    let mut process_specs = Vec::with_capacity(topology.services.len());
    let mut service_plans = Vec::with_capacity(topology.services.len());
    for service in &topology.services {
        validate_token(&service.role).map_err(|()| IntakePlanError::DuplicateRole)?;
        if !roles.insert(service.role.clone()) {
            return Err(IntakePlanError::DuplicateRole);
        }
        validate_absolute_path(&service.source.path)
            .map_err(|()| IntakePlanError::InvalidSourcePath)?;
        validate_relative_path(&service.entrypoint)
            .map_err(|()| IntakePlanError::InvalidEntrypoint)?;
        validate_b3(&service.source.image_identity)
            .map_err(|()| IntakePlanError::InvalidIdentity)?;
        if service.source.layers.len() > MAX_LAYERS_PER_SERVICE {
            return Err(IntakePlanError::LayerLimit);
        }
        let format = parse_format(&service.source.format)?;
        match format {
            ImageFormat::OciLayers if service.source.layers.is_empty() => {
                return Err(IntakePlanError::MissingLayers);
            }
            ImageFormat::Directory | ImageFormat::TarArchive
                if !service.source.layers.is_empty() =>
            {
                return Err(IntakePlanError::LayersNotAllowed);
            }
            ImageFormat::OciLayers | ImageFormat::Directory | ImageFormat::TarArchive => {}
        }
        if format == ImageFormat::OciLayers
            && image_identity_from_layers(&service.source.layers) != service.source.image_identity
        {
            return Err(IntakePlanError::InvalidIdentity);
        }
        let mut layer_paths = BTreeSet::new();
        for layer in &service.source.layers {
            validate_relative_path(&layer.path).map_err(|()| IntakePlanError::InvalidLayer)?;
            validate_b3(&layer.identity).map_err(|()| IntakePlanError::InvalidIdentity)?;
            if !layer_paths.insert(layer.path.clone()) {
                return Err(IntakePlanError::InvalidLayer);
            }
        }
        let root_path = format!("/services/{}/root", service.role);
        let executable = format!("{root_path}/{}", service.entrypoint);
        process_specs.push(ProcessSpec {
            role: service.role.clone(),
            executable,
            arguments: service.arguments.clone(),
            environment: service.environment.clone(),
            shared_directories: service.shared_directories.clone(),
            restart: service.restart.clone(),
            instrumented: service.instrumented,
            transport_slot: service.transport_slot,
        });
        service_plans.push(ServiceBundlePlan {
            role: service.role.clone(),
            format,
            source_path: service.source.path.clone(),
            image_identity: service.source.image_identity.clone(),
            layers: service.source.layers.clone(),
            root_path,
        });
    }
    let manifest = admit_manifest(&ProcessManifest {
        schema: PROCESS_MANIFEST_SCHEMA.to_string(),
        guest: topology.topology_id.clone(),
        shared_directories: topology.shared_directories.clone(),
        processes: process_specs,
    })
    .map_err(IntakePlanError::Manifest)?;
    let mut plan = BundlePlan {
        schema: OCI_BUNDLE_PLAN_SCHEMA.to_string(),
        topology_id: topology.topology_id.clone(),
        bundle_identity: String::new(),
        process_manifest: manifest,
        services: service_plans,
    };
    plan.bundle_identity = bundle_plan_identity(&plan);
    Ok(plan)
}

fn parse_format(value: &str) -> Result<ImageFormat, IntakePlanError> {
    match value {
        "oci_layers" => Ok(ImageFormat::OciLayers),
        "directory" => Ok(ImageFormat::Directory),
        "tar_archive" => Ok(ImageFormat::TarArchive),
        other => Err(IntakePlanError::UnsupportedFormat(other.to_string())),
    }
}

pub fn image_identity_from_layers(layers: &[LayerDescriptor]) -> String {
    let mut parts = Vec::with_capacity(layers.len().saturating_mul(2));
    for layer in layers {
        parts.push(layer.path.as_bytes());
        parts.push(layer.identity.as_bytes());
    }
    digest(&parts)
}

pub fn bundle_plan_identity(plan: &BundlePlan) -> String {
    let mut projection = plan.clone();
    projection.bundle_identity.clear();
    let bytes = serde_json::to_vec(&projection).expect("bundle plan serialization is infallible");
    digest(&[b"bundle-plan", &bytes])
}

fn digest(parts: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(HASH_DOMAIN);
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("b3:{}", hasher.finalize().to_hex())
}

fn validate_token(value: &str) -> Result<(), ()> {
    if crate::process::validate_process_token(value) {
        Ok(())
    } else {
        Err(())
    }
}

fn validate_b3(value: &str) -> Result<(), ()> {
    let Some(hex) = value.strip_prefix("b3:") else {
        return Err(());
    };
    const BLAKE3_HEX_BYTES: usize = 64;
    if hex.len() == BLAKE3_HEX_BYTES && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(())
    }
}

fn validate_absolute_path(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > MAX_SOURCE_PATH_BYTES
        || !value.starts_with('/')
        || value.split('/').any(|component| component == "..")
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_relative_path(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > MAX_SOURCE_PATH_BYTES
        || value.starts_with('/')
        || value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        Err(())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceIntakeReceipt {
    pub role: String,
    pub image_identity: String,
    pub layer_identities: Vec<String>,
    pub root_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntakeReceipt {
    pub schema: String,
    pub bundle_identity: String,
    pub process_manifest_identity: String,
    pub services: Vec<ServiceIntakeReceipt>,
    pub claim_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntakeReceiptError {
    InvalidSchema,
    BundleIdentityMismatch,
    ManifestIdentityMismatch,
    ServiceMismatch,
    ImageIdentityMismatch,
    LayerIdentityMismatch,
    InvalidRootIdentity,
    ClaimOverreach,
}

pub fn validate_receipt(
    plan: &BundlePlan,
    receipt: &IntakeReceipt,
) -> Result<(), IntakeReceiptError> {
    if receipt.schema != OCI_INTAKE_RECEIPT_SCHEMA {
        return Err(IntakeReceiptError::InvalidSchema);
    }
    if receipt.bundle_identity != plan.bundle_identity {
        return Err(IntakeReceiptError::BundleIdentityMismatch);
    }
    if receipt.process_manifest_identity != plan.process_manifest.manifest_identity {
        return Err(IntakeReceiptError::ManifestIdentityMismatch);
    }
    if receipt.services.len() != plan.services.len() {
        return Err(IntakeReceiptError::ServiceMismatch);
    }
    for (expected, actual) in plan.services.iter().zip(&receipt.services) {
        if expected.role != actual.role {
            return Err(IntakeReceiptError::ServiceMismatch);
        }
        if expected.image_identity != actual.image_identity {
            return Err(IntakeReceiptError::ImageIdentityMismatch);
        }
        let expected_layers = expected
            .layers
            .iter()
            .map(|layer| layer.identity.clone())
            .collect::<Vec<_>>();
        if expected_layers != actual.layer_identities {
            return Err(IntakeReceiptError::LayerIdentityMismatch);
        }
        validate_b3(&actual.root_identity).map_err(|()| IntakeReceiptError::InvalidRootIdentity)?;
    }
    if receipt.claim_scope != OCI_CLAIM_SCOPE {
        return Err(IntakeReceiptError::ClaimOverreach);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_process::{RestartMode, SharedDeviceKind};
    use std::collections::BTreeMap;

    const RESTART_LIMIT: u32 = 2;
    const DIGEST_HEX_BYTES: usize = 64;

    fn digest_value(byte: char) -> String {
        format!("b3:{}", byte.to_string().repeat(DIGEST_HEX_BYTES))
    }

    fn topology() -> OciTopology {
        let layers = vec![LayerDescriptor {
            path: "layer-0.tar".to_string(),
            identity: digest_value('b'),
        }];
        let image_identity = image_identity_from_layers(&layers);
        OciTopology {
            schema: OCI_TOPOLOGY_SCHEMA.to_string(),
            topology_id: "database-stack".to_string(),
            shared_directories: vec![SharedDirectorySpec {
                id: "data".to_string(),
                path: "/data".to_string(),
                device: SharedDeviceKind::Memory,
            }],
            services: vec![ImageService {
                role: "database".to_string(),
                source: ImageSource {
                    format: "oci_layers".to_string(),
                    path: "/images/database".to_string(),
                    image_identity,
                    layers,
                },
                entrypoint: "bin/database".to_string(),
                arguments: Vec::new(),
                environment: BTreeMap::new(),
                shared_directories: vec!["data".to_string()],
                restart: RestartPolicy {
                    mode: RestartMode::OnFailure,
                    max_restarts: RESTART_LIMIT,
                },
                instrumented: false,
                transport_slot: None,
            }],
        }
    }

    #[test]
    fn topology_lowers_to_bound_manifest_and_receipt() {
        let plan = lower_topology(&topology()).unwrap();
        assert_eq!(plan.services.len(), 1);
        assert_eq!(
            plan.process_manifest.processes[0].spec.executable,
            "/services/database/root/bin/database"
        );
        let receipt = IntakeReceipt {
            schema: OCI_INTAKE_RECEIPT_SCHEMA.to_string(),
            bundle_identity: plan.bundle_identity.clone(),
            process_manifest_identity: plan.process_manifest.manifest_identity.clone(),
            services: vec![ServiceIntakeReceipt {
                role: "database".to_string(),
                image_identity: plan.services[0].image_identity.clone(),
                layer_identities: vec![digest_value('b')],
                root_identity: digest_value('c'),
            }],
            claim_scope: OCI_CLAIM_SCOPE.to_string(),
        };
        validate_receipt(&plan, &receipt).unwrap();
    }

    #[test]
    fn unsupported_format_conflicting_role_and_provenance_fail_closed() {
        let mut unsupported = topology();
        unsupported.services[0].source.format = "docker-daemon".to_string();
        assert_eq!(
            lower_topology(&unsupported),
            Err(IntakePlanError::UnsupportedFormat(
                "docker-daemon".to_string()
            ))
        );
        let mut duplicate = topology();
        duplicate.services.push(duplicate.services[0].clone());
        assert_eq!(
            lower_topology(&duplicate),
            Err(IntakePlanError::DuplicateRole)
        );

        let plan = lower_topology(&topology()).unwrap();
        let receipt = IntakeReceipt {
            schema: OCI_INTAKE_RECEIPT_SCHEMA.to_string(),
            bundle_identity: plan.bundle_identity.clone(),
            process_manifest_identity: plan.process_manifest.manifest_identity.clone(),
            services: vec![ServiceIntakeReceipt {
                role: "database".to_string(),
                image_identity: digest_value('f'),
                layer_identities: vec![digest_value('b')],
                root_identity: digest_value('c'),
            }],
            claim_scope: OCI_CLAIM_SCOPE.to_string(),
        };
        assert_eq!(
            validate_receipt(&plan, &receipt),
            Err(IntakeReceiptError::ImageIdentityMismatch)
        );
    }
}
