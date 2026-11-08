use crate::profile_projection::{BoundArtifact, ProjectionReceipt};
use crate::{EvidenceError, EvidenceResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArtifactSpec {
    pub(crate) path: &'static str,
    pub(crate) identity: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectionSpec {
    pub(crate) profile_id: &'static str,
    pub(crate) source: ArtifactSpec,
    pub(crate) contract: ArtifactSpec,
    pub(crate) imports: &'static [ArtifactSpec],
    pub(crate) projection: ArtifactSpec,
    pub(crate) receipt: &'static str,
}

const PRIMITIVES: ArtifactSpec = ArtifactSpec {
    path: "contracts/evidence/profile-primitives.ncl",
    identity: "blake3:829939b6d573df8a78b59016c6b5eb0ed745333ace8522da076a223eca3eea08",
};
const RUN_SOURCE: ArtifactSpec = ArtifactSpec {
    path: "contracts/evidence/examples/raft-run-config.ncl",
    identity: "blake3:68c9c7ce183a47585ebd4637150802c68c5d97d946e89c5a72029c639783511f",
};
const RUN_CONTRACT: ArtifactSpec = ArtifactSpec {
    path: "contracts/evidence/run-config.ncl",
    identity: "blake3:5fc8271940ddacf8b01845470159ca8a01035a39f940fc587acb11431e5e2e75",
};
const SCHEDULER_PROGRESS: ArtifactSpec = ArtifactSpec {
    path: "contracts/evidence/scheduler-progress.ncl",
    identity: "blake3:cb1256af0e1752112ceb3b276d839f433acc4dd25db9156a60f901be1be4001a",
};
const RUN_IMPORTS: [ArtifactSpec; 2] = [PRIMITIVES, SCHEDULER_PROGRESS];
const CAMPAIGN_IMPORTS: [ArtifactSpec; 4] =
    [RUN_SOURCE, RUN_CONTRACT, PRIMITIVES, SCHEDULER_PROGRESS];
const SINGLE_PRIMITIVES_IMPORT: [ArtifactSpec; 1] = [PRIMITIVES];

pub(crate) const SPECS: &[ProjectionSpec] = &[
    ProjectionSpec {
        profile_id: "vm-run",
        source: RUN_SOURCE,
        contract: RUN_CONTRACT,
        imports: &RUN_IMPORTS,
        projection: ArtifactSpec {
            path: "contracts/evidence/fixtures/valid/run-profile.valid.json",
            identity: "blake3:48b80699b5823d6e8e5dbd98b9a68ded8ea6fd3f88c05736d2d8a0c131030da3",
        },
        receipt: "contracts/evidence/fixtures/valid/run-profile.projection-receipt.json",
    },
    ProjectionSpec {
        profile_id: "in-process-simulator",
        source: ArtifactSpec {
            path: "contracts/evidence/examples/register-simulator-profile.ncl",
            identity: "blake3:a217670982762ec3556efdd6e970471da35a7e7279528c9eb6951dee61ec8c42",
        },
        contract: ArtifactSpec {
            path: "contracts/evidence/simulator-profile.ncl",
            identity: "blake3:9fd2db62d183f0c571f41a88787e8451760527759eb87027132878e8d923fb06",
        },
        imports: &SINGLE_PRIMITIVES_IMPORT,
        projection: ArtifactSpec {
            path: "contracts/evidence/fixtures/valid/simulator-profile.valid.json",
            identity: "blake3:d9f703ce8b650aabacebc8dd190946922771a88c1e7696cd3bb952c9080a8045",
        },
        receipt: "contracts/evidence/fixtures/valid/simulator-profile.projection-receipt.json",
    },
    ProjectionSpec {
        profile_id: "campaign",
        source: ArtifactSpec {
            path: "contracts/evidence/examples/raft-campaign-profile.ncl",
            identity: "blake3:699d0453493687c23a0c5f9928486547f344e9f58b421545c4757ea49c2e1d07",
        },
        contract: ArtifactSpec {
            path: "contracts/evidence/campaign-profile.ncl",
            identity: "blake3:be83fff426cf1d5f1824e15bb55c955e6cba72e87f5233dda51c652df7e6c7ea",
        },
        imports: &CAMPAIGN_IMPORTS,
        projection: ArtifactSpec {
            path: "contracts/evidence/fixtures/valid/campaign-profile.valid.json",
            identity: "blake3:e4cacbc2ac1f882d4d543dbd20c7a9aa86ccc1b31223374b84aa2a3b2b7bde6f",
        },
        receipt: "contracts/evidence/fixtures/valid/campaign-profile.projection-receipt.json",
    },
    ProjectionSpec {
        profile_id: "finite-fault-schedule",
        source: ArtifactSpec {
            path: "contracts/evidence/examples/raft-fault-schedule-profile.ncl",
            identity: "blake3:a879d54b3cbafa84a23a83f4d77cc94d89055aefaf45279816dae60fe54943dd",
        },
        contract: ArtifactSpec {
            path: "contracts/evidence/fault-schedule-profile.ncl",
            identity: "blake3:de501efbcb8b33f2d70c0b2b6dafb54ae27da7629cc47c3d040ca4d2bb16d22f",
        },
        imports: &SINGLE_PRIMITIVES_IMPORT,
        projection: ArtifactSpec {
            path: "contracts/evidence/fixtures/valid/fault-schedule-profile.valid.json",
            identity: "blake3:2819933e3581e0f6b69b177346b6400ad9248d2de92299d163ec28393334c7e6",
        },
        receipt: "contracts/evidence/fixtures/valid/fault-schedule-profile.projection-receipt.json",
    },
];

pub(crate) fn find_spec(profile_id: &str) -> EvidenceResult<&'static ProjectionSpec> {
    SPECS
        .iter()
        .find(|spec| spec.profile_id == profile_id)
        .ok_or_else(|| EvidenceError::new(format!("unknown trusted profile ID: {profile_id}")))
}

pub(crate) fn validate_receipt_against_spec(
    receipt: &ProjectionReceipt,
    spec: &ProjectionSpec,
) -> EvidenceResult<()> {
    ensure_artifact_matches(&receipt.source, spec.source, "source")?;
    ensure_artifact_matches(&receipt.contract, spec.contract, "contract")?;
    if receipt.imports.len() != spec.imports.len() {
        return Err(EvidenceError::new(
            "profile projection import cardinality mismatch",
        ));
    }
    for (actual, expected) in receipt.imports.iter().zip(spec.imports) {
        ensure_artifact_matches(actual, *expected, "import")?;
    }
    ensure_artifact_matches(&receipt.projection, spec.projection, "projection")
}

fn ensure_artifact_matches(
    actual: &BoundArtifact,
    expected: ArtifactSpec,
    kind: &str,
) -> EvidenceResult<()> {
    if actual.path != expected.path || actual.identity != expected.identity {
        return Err(EvidenceError::new(format!(
            "profile projection {kind} differs from the trusted specification: {}",
            expected.path
        )));
    }
    Ok(())
}
