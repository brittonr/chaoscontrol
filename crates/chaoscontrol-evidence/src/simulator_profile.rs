use serde::{Deserialize, Serialize};

use crate::in_process_simulator::{
    validate_simulator_config, DiskProfile, FaultScheduleRef, NetworkProfile, RngPolicy,
    SchedulerPolicy, SimulatorConfig, VirtualClockPolicy, WorkloadIdentity,
};
use crate::{EvidenceError, EvidenceResult};

const MAX_ARTIFACTS: usize = 64;
const MAX_PROFILE_BYTES: u64 = 1024 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 128;
const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_BYTES: usize = 64;
const REQUIRED_SCOPE: &str = "adapter-based in-process simulator evidence; not VM replay proof, not arbitrary binary support, not full FoundationDB parity";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SimulatorProfile {
    pub schema_version: u64,
    pub run_id: String,
    pub seed: u64,
    pub workload: WorkloadIdentity,
    pub scheduler: SchedulerPolicy,
    pub virtual_clock: VirtualClockPolicy,
    pub rng: RngPolicy,
    pub network: NetworkProfile,
    pub disk: DiskProfile,
    pub fault_schedule: FaultScheduleRef,
    pub artifacts: Vec<ArtifactBinding>,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactBinding {
    pub name: String,
    pub digest: String,
}

impl SimulatorProfile {
    pub fn try_into_config(self) -> EvidenceResult<SimulatorConfig> {
        validate_profile_shape(&self)?;
        let artifacts = self
            .artifacts
            .into_iter()
            .map(|artifact| (artifact.name, artifact.digest))
            .collect::<std::collections::BTreeMap<_, _>>();
        let config = SimulatorConfig {
            schema_version: self.schema_version,
            run_id: self.run_id,
            seed: self.seed,
            workload: self.workload,
            scheduler: self.scheduler,
            virtual_clock: self.virtual_clock,
            rng: self.rng,
            network: self.network,
            disk: self.disk,
            fault_schedule: self.fault_schedule,
            artifacts,
            scope: self.scope,
        };
        validate_simulator_config(&config)?;
        Ok(config)
    }
}

pub fn load_simulator_profile(path: &std::path::Path) -> EvidenceResult<SimulatorProfile> {
    let input = crate::bounded_file::read_bounded_regular_file(path, MAX_PROFILE_BYTES)?;
    crate::json_preflight::preflight_json(&input, crate::json_preflight::QUALITY_REPORT_LIMITS)?;
    serde_json::from_str(&input)
        .map_err(|error| EvidenceError::new(format!("invalid simulator profile JSON: {error}")))
}

pub fn validate_profile_shape(profile: &SimulatorProfile) -> EvidenceResult<()> {
    if profile.artifacts.is_empty() || profile.artifacts.len() > MAX_ARTIFACTS {
        return Err(EvidenceError::new(
            "simulator profile artifact count is invalid",
        ));
    }
    validate_identifier("run_id", &profile.run_id)?;
    validate_identifier("workload.name", &profile.workload.name)?;
    validate_identifier(
        "workload.adapter_version",
        &profile.workload.adapter_version,
    )?;
    validate_identifier("workload.scenario_id", &profile.workload.scenario_id)?;
    validate_identifier("network.profile_id", &profile.network.profile_id)?;
    validate_identifier("disk.profile_id", &profile.disk.profile_id)?;
    validate_identifier(
        "fault_schedule.schedule_id",
        &profile.fault_schedule.schedule_id,
    )?;
    validate_sha256("fault_schedule.digest", &profile.fault_schedule.digest)?;
    let mut names = std::collections::BTreeSet::new();
    for artifact in &profile.artifacts {
        validate_identifier("artifact.name", &artifact.name)?;
        validate_sha256("artifact.digest", &artifact.digest)?;
        if !names.insert(&artifact.name) {
            return Err(EvidenceError::new(
                "simulator profile artifact names must be unique",
            ));
        }
    }
    if profile.scope != REQUIRED_SCOPE {
        return Err(EvidenceError::new("simulator profile scope is not exact"));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> EvidenceResult<()> {
    let valid = !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
    if !valid {
        return Err(EvidenceError::new(format!(
            "simulator profile {field} is not a bounded identifier"
        )));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> EvidenceResult<()> {
    let Some(hex) = value.strip_prefix(SHA256_PREFIX) else {
        return Err(EvidenceError::new(format!(
            "simulator profile {field} is not sha256-bound"
        )));
    };
    if hex.len() != SHA256_HEX_BYTES
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(EvidenceError::new(format!(
            "simulator profile {field} has an invalid sha256 digest"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_profile() -> SimulatorProfile {
        serde_json::from_str(include_str!(
            "../../../contracts/evidence/fixtures/valid/simulator-profile.valid.json"
        ))
        .expect("valid simulator fixture")
    }

    #[test]
    fn valid_profile_maps_to_runtime_config() {
        let config = valid_profile().try_into_config().expect("valid projection");
        assert_eq!(config.run_id, "register-simulator-42");
        assert_eq!(config.artifacts.len(), 1);
    }

    #[test]
    fn duplicate_artifact_and_unknown_field_fail_closed() {
        let mut duplicate = valid_profile();
        duplicate.artifacts.push(duplicate.artifacts[0].clone());
        assert!(duplicate.try_into_config().is_err());

        let input =
            include_str!("../../../contracts/evidence/fixtures/valid/simulator-profile.valid.json");
        let forged = input.replacen("\"seed\": 42", "\"seed\": 42, \"elapsed\": 1", 1);
        assert!(serde_json::from_str::<SimulatorProfile>(&forged).is_err());
    }

    #[test]
    fn bounded_loader_rejects_symlinks() {
        let directory = tempfile::tempdir().expect("tempdir");
        let regular = directory.path().join("simulator.json");
        std::fs::write(
            &regular,
            include_str!("../../../contracts/evidence/fixtures/valid/simulator-profile.valid.json"),
        )
        .expect("write profile");
        assert!(load_simulator_profile(&regular).is_ok());

        #[cfg(unix)]
        {
            let link = directory.path().join("simulator-link.json");
            std::os::unix::fs::symlink(&regular, &link).expect("symlink");
            assert!(load_simulator_profile(&link).is_err());
        }
    }
}
