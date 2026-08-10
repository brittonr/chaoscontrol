// r[impl chaoscontrol.property_coverage.virtio_evidence]
use std::collections::BTreeSet;

use chaoscontrol_evidence::kvm_release::{
    artifact_set_identity, classify, command_identity, matrix_identity, ArtifactIdentity,
    KvmReleaseMatrix, KvmReleaseReceipt, MatrixRow, ReleaseClass, RowReceipt, RowStatus,
    SourceFacts, WorkerFacts, RECEIPT_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use crate::framework::{run_generated, DeterministicRng, Failure, PropertyProfile, SuiteReport};

const SUITE: &str = "evidence";
const SOURCE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const STALE_SOURCE_REVISION: &str = "fedcba9876543210fedcba9876543210fedcba98";
const STARTED_UNIX_SECONDS: u64 = 1_000;
const FINISHED_UNIX_SECONDS: u64 = 1_100;
const NOW_UNIX_SECONDS: u64 = FINISHED_UNIX_SECONDS;
const ROW_TIME_OFFSET: u64 = 1;
const KVM_API_VERSION: i32 = 12;
const ARTIFACT_BYTES: u64 = 1;
const VALID_BLAKE3: &str =
    "blake3:0000000000000000000000000000000000000000000000000000000000000000";
const INVALID_BLAKE3: &str = "invalid";
const COMMAND_VARIANTS: usize = 6;
const COMMAND_ADD: usize = 0;
const COMMAND_REMOVE: usize = 1;
const COMMAND_DIRTY: usize = 2;
const COMMAND_STALE: usize = 3;
const COMMAND_TAMPER: usize = 4;
const COMMAND_RESET: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Add { row: usize },
    Remove { row: usize },
    Dirty { value: bool },
    StaleSource { value: bool },
    Tamper { row: usize },
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    row_counts: Vec<usize>,
    tampered_rows: BTreeSet<usize>,
    dirty: bool,
    stale_source: bool,
}

impl Model {
    fn new(row_count: usize) -> Self {
        Self {
            row_counts: vec![0; row_count],
            tampered_rows: BTreeSet::new(),
            dirty: false,
            stale_source: false,
        }
    }

    fn eligible(&self) -> bool {
        !self.dirty
            && !self.stale_source
            && self.tampered_rows.is_empty()
            && self.row_counts.iter().all(|count| *count == 1)
    }
}

pub fn run(selected: &PropertyProfile) -> Result<SuiteReport, crate::AnyCounterexample> {
    let matrix = matrix();
    run_generated(
        SUITE,
        selected,
        |rng| generate(rng, matrix.rows.len()),
        |commands| check(&matrix, commands),
    )
    .map_err(crate::AnyCounterexample::evidence)
}

fn matrix() -> KvmReleaseMatrix {
    serde_json::from_str(include_str!("../../../contracts/kvm-release/matrix.json"))
        .expect("the committed KVM release matrix must be valid JSON")
}

fn generate(rng: &mut DeterministicRng, row_count: usize) -> Command {
    let row = rng.index(row_count);
    match rng.index(COMMAND_VARIANTS) {
        COMMAND_ADD => Command::Add { row },
        COMMAND_REMOVE => Command::Remove { row },
        COMMAND_DIRTY => Command::Dirty { value: rng.coin() },
        COMMAND_STALE => Command::StaleSource { value: rng.coin() },
        COMMAND_TAMPER => Command::Tamper { row },
        COMMAND_RESET => Command::Reset,
        _ => unreachable!("bounded command selector must produce a known evidence command"),
    }
}

fn check(matrix: &KvmReleaseMatrix, commands: &[Command]) -> Result<usize, Failure> {
    let mut receipt = base_receipt(matrix);
    let mut model = Model::new(matrix.rows.len());
    let mut rejected = 0_usize;

    for (step, command) in commands.iter().enumerate() {
        match *command {
            Command::Add { row } => {
                receipt.rows.push(valid_row_receipt(&matrix.rows[row], row));
                model.row_counts[row] += 1;
            }
            Command::Remove { row } => {
                let row_id = &matrix.rows[row].id;
                receipt.rows.retain(|candidate| candidate.id != *row_id);
                model.row_counts[row] = 0;
                model.tampered_rows.remove(&row);
            }
            Command::Dirty { value } => {
                receipt.source.dirty = value;
                model.dirty = value;
            }
            Command::StaleSource { value } => {
                receipt.source.revision = if value {
                    STALE_SOURCE_REVISION.to_string()
                } else {
                    SOURCE_REVISION.to_string()
                };
                model.stale_source = value;
            }
            Command::Tamper { row } => {
                let row_id = &matrix.rows[row].id;
                let mut changed = false;
                for candidate in &mut receipt.rows {
                    if candidate.id == *row_id {
                        candidate.artifacts[0].blake3 = INVALID_BLAKE3.to_string();
                        changed = true;
                    }
                }
                if changed {
                    model.tampered_rows.insert(row);
                } else {
                    rejected += 1;
                }
            }
            Command::Reset => {
                receipt = base_receipt(matrix);
                model = Model::new(matrix.rows.len());
            }
        }

        let first = classify(matrix, SOURCE_REVISION, &receipt, NOW_UNIX_SECONDS);
        let second = classify(matrix, SOURCE_REVISION, &receipt, NOW_UNIX_SECONDS);
        if first != second {
            return Err(Failure::new(
                "evidence-classification-determinism",
                step,
                "identical evidence inputs produced different decisions",
            ));
        }
        let actual_eligible = first.terminal_class == ReleaseClass::ReleaseEligible;
        if actual_eligible != model.eligible() {
            return Err(Failure::new(
                "evidence-reference-agreement",
                step,
                format!("decision={first:?}, model={model:?}"),
            ));
        }
        if model.stale_source && actual_eligible {
            return Err(Failure::new(
                "evidence-source-binding",
                step,
                "evidence from another source revision was promoted",
            ));
        }
    }
    Ok(rejected)
}

fn base_receipt(matrix: &KvmReleaseMatrix) -> KvmReleaseReceipt {
    KvmReleaseReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        matrix_profile: matrix.profile_id.clone(),
        matrix_identity: matrix_identity(matrix),
        source: SourceFacts {
            revision: SOURCE_REVISION.to_string(),
            dirty: false,
        },
        runner_revision: SOURCE_REVISION.to_string(),
        worker: WorkerFacts {
            architecture: matrix.required_worker_arch.clone(),
            kernel_release: "property-suite".to_string(),
            kvm_api_version: Some(KVM_API_VERSION),
            capabilities: matrix.required_worker_capabilities.clone(),
        },
        started_unix_seconds: STARTED_UNIX_SECONDS,
        finished_unix_seconds: FINISHED_UNIX_SECONDS,
        rows: Vec::new(),
        bounded_claim: matrix.bounded_claim.clone(),
        non_claims: matrix.non_claims.clone(),
        terminal_class: ReleaseClass::Blocked,
    }
}

fn valid_row_receipt(row: &MatrixRow, row_index: usize) -> RowReceipt {
    let row_offset = u64::try_from(row_index).expect("matrix row count must fit in u64");
    let started = STARTED_UNIX_SECONDS + ROW_TIME_OFFSET + row_offset;
    let artifacts = vec![ArtifactIdentity {
        path: format!("property-suite/{}/receipt.json", row.id),
        bytes: ARTIFACT_BYTES,
        blake3: VALID_BLAKE3.to_string(),
    }];
    let mut executed_argv = vec![row.command.program.clone()];
    executed_argv.extend(row.command.args.clone());
    RowReceipt {
        id: row.id.clone(),
        kind: row.kind,
        required_capabilities: row.required_capabilities.clone(),
        command: row.command.clone(),
        executed_argv,
        command_identity: command_identity(&row.command),
        started_unix_seconds: started,
        finished_unix_seconds: started + ROW_TIME_OFFSET,
        status: RowStatus::Passed,
        exit_code: Some(0),
        artifacts: artifacts.clone(),
        artifact_set_identity: artifact_set_identity(&artifacts),
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_complete_and_tampered_receipt_regression() {
        let matrix = matrix();
        let commands: Vec<Command> = serde_json::from_str(include_str!(
            "../../../contracts/property-coverage/fixtures/regressions/evidence-complete-tampered.json"
        ))
        .expect("the evidence regression fixture must be valid JSON");
        let complete_row_count = matrix.rows.len();
        assert!(check(&matrix, &commands[..complete_row_count]).is_ok());
        assert!(check(&matrix, &commands).is_ok());
    }
}
