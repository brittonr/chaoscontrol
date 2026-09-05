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
    identity: "blake3:fc01a81570654cff3ac66450a649aa804d1fac5866c69f6b0eac5c01663a8d5f",
};
const RUN_CONTRACT: ArtifactSpec = ArtifactSpec {
    path: "contracts/evidence/run-config.ncl",
    identity: "blake3:1f9521e8688fb57a1ac1c9647ab733a41bebfcc0a729adb6bdbfd07264982a99",
};
const RUN_IMPORTS: [ArtifactSpec; 1] = [PRIMITIVES];
const CAMPAIGN_IMPORTS: [ArtifactSpec; 3] = [RUN_SOURCE, RUN_CONTRACT, PRIMITIVES];
const SINGLE_PRIMITIVES_IMPORT: [ArtifactSpec; 1] = [PRIMITIVES];

pub(crate) const SPECS: &[ProjectionSpec] = &[
    ProjectionSpec {
        profile_id: "vm-run",
        source: RUN_SOURCE,
        contract: RUN_CONTRACT,
        imports: &RUN_IMPORTS,
        projection: ArtifactSpec {
            path: "contracts/evidence/fixtures/valid/run-profile.valid.json",
            identity: "blake3:060e21247896866c54852f77beab9bff62a340634c39b98e604831120612cd23",
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
            identity: "blake3:48d0df8127888f9388cf2bfd46c42bb4ca99ebf7b52a0fc614ea3d230a9dbf14",
        },
        contract: ArtifactSpec {
            path: "contracts/evidence/campaign-profile.ncl",
            identity: "blake3:dd050710aefb3c39a81eb0b4f4b5cd15c35f202e511233adc12a5d2e1ade5efa",
        },
        imports: &CAMPAIGN_IMPORTS,
        projection: ArtifactSpec {
            path: "contracts/evidence/fixtures/valid/campaign-profile.valid.json",
            identity: "blake3:a570e82bcb0faa1cac4a8107b0f48c31cb1f514bd2c9c47eba2aa99ccaf2fe8f",
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

pub(crate) fn find_spec(profile_id: &str) -> crate::EvidenceResult<&'static ProjectionSpec> {
    SPECS
        .iter()
        .find(|spec| spec.profile_id == profile_id)
        .ok_or_else(|| {
            crate::EvidenceError::new(format!("unknown trusted profile ID: {profile_id}"))
        })
}

pub(crate) fn validate_receipt_against_spec(
    receipt: &crate::profile_projection::ProjectionReceipt,
    spec: &ProjectionSpec,
) -> crate::EvidenceResult<()> {
    ensure_artifact_matches(&receipt.source, spec.source, "source")?;
    ensure_artifact_matches(&receipt.contract, spec.contract, "contract")?;
    if receipt.imports.len() != spec.imports.len() {
        return Err(crate::EvidenceError::new(
            "profile projection import cardinality mismatch",
        ));
    }
    for (actual, expected) in receipt.imports.iter().zip(spec.imports) {
        ensure_artifact_matches(actual, *expected, "import")?;
    }
    ensure_artifact_matches(&receipt.projection, spec.projection, "projection")
}

fn ensure_artifact_matches(
    actual: &crate::profile_projection::BoundArtifact,
    expected: ArtifactSpec,
    kind: &str,
) -> crate::EvidenceResult<()> {
    if actual.path != expected.path || actual.identity != expected.identity {
        return Err(crate::EvidenceError::new(format!(
            "profile projection {kind} differs from the trusted specification: {}",
            expected.path
        )));
    }
    Ok(())
}
