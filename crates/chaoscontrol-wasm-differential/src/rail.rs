//! Pure MVP streaming, reproducibility, resume, shrink and evidence contracts.
use crate::*;
use serde::{Deserialize, Serialize};

fn identity<T: Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(domain, value)).map_err(|error| error.to_string())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chunk {
    pub offset: usize,
    pub length: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkSchedule {
    pub module_blake3: String,
    pub seed: u64,
    pub generator: String,
    pub expected_class: CaseKind,
    pub return_policy: String,
    pub chunks: Vec<Chunk>,
}
impl ChunkSchedule {
    pub fn validate(&self, bytes: &[u8], maximum_chunks: usize) -> Result<String, String> {
        if self.module_blake3 != blake3::hash(bytes).to_hex().as_str()
            || self.generator.is_empty()
            || self.return_policy != "return-each-consumed-buffer-once"
            || self.chunks.is_empty()
            || self.chunks.len() > maximum_chunks
        {
            return Err("stream-schedule-identity-or-bound".into());
        }
        let mut position = 0usize;
        for chunk in &self.chunks {
            if chunk.offset != position || chunk.length == 0 {
                return Err("stream-partition".into());
            }
            position = position
                .checked_add(chunk.length)
                .ok_or("stream-partition-overflow")?;
            if position > bytes.len() {
                return Err("stream-partition-extension".into());
            }
        }
        if position != bytes.len() {
            return Err("stream-partition-omission".into());
        }
        identity("chaoscontrol.mvp.chunk-schedule.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseManifest {
    pub profile_identity: String,
    pub generator: String,
    pub seed: u64,
    pub index: usize,
    pub maximum_instruction_pairs: usize,
    pub parent_blake3: String,
    pub expected_class: CaseKind,
    pub output_blake3: String,
    pub schedules: Vec<ChunkSchedule>,
}
impl CaseManifest {
    pub fn generate(
        profile: &DifferentialProfile,
        index: usize,
        kind: CaseKind,
    ) -> Result<(Self, Vec<u8>), String> {
        let limit = match kind {
            CaseKind::Execute => profile.generated_valid_cases,
            CaseKind::Reject => profile.generated_malformed_cases,
        };
        if index >= limit {
            return Err("generation-bound".into());
        }
        let parent = generate_mvp_module(
            profile.seed,
            index,
            profile.bounds.maximum_generated_instruction_pairs,
        );
        let parent_blake3 = blake3::hash(&parent).to_hex().to_string();
        let bytes = match kind {
            CaseKind::Execute => parent,
            CaseKind::Reject => malformed_magic(&parent),
        };
        if bytes.len() as u64 > profile.bounds.maximum_input_bytes {
            return Err("generation-input-bound".into());
        }
        let output_blake3 = blake3::hash(&bytes).to_hex().to_string();
        let complete = ChunkSchedule {
            module_blake3: output_blake3.clone(),
            seed: profile.seed,
            generator: profile.generator_id.clone(),
            expected_class: kind,
            return_policy: "return-each-consumed-buffer-once".into(),
            chunks: vec![Chunk {
                offset: 0,
                length: bytes.len(),
            }],
        };
        let mut one_byte = complete.clone();
        if bytes.len() > profile.bounds.maximum_chunks {
            return Err("generation-chunk-bound".into());
        }
        one_byte.chunks = (0..bytes.len())
            .map(|offset| Chunk { offset, length: 1 })
            .collect();
        let manifest = Self {
            profile_identity: profile_identity(profile)?,
            generator: profile.generator_id.clone(),
            seed: profile.seed,
            index,
            maximum_instruction_pairs: profile.bounds.maximum_generated_instruction_pairs,
            parent_blake3,
            expected_class: kind,
            output_blake3,
            schedules: vec![complete, one_byte],
        };
        Ok((manifest, bytes))
    }
    pub fn verify(&self, profile: &DifferentialProfile, bytes: &[u8]) -> Result<(), String> {
        let (expected, regenerated) = Self::generate(profile, self.index, self.expected_class)?;
        if self != &expected || bytes != regenerated {
            return Err("corpus-replay-mismatch".into());
        }
        for schedule in &self.schedules {
            schedule.validate(bytes, profile.bounds.maximum_chunks)?;
        }
        match self.expected_class {
            CaseKind::Execute => {
                crate::admission::admit_module(profile, bytes)
                    .map_err(|error| format!("corpus-admission: {error:?}"))?;
            }
            CaseKind::Reject => {
                if crate::admission::admit_module(profile, bytes).is_ok() {
                    return Err("corpus-expected-rejection".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceClass {
    RuntimeMatch,
    RuntimeDivergence,
    ResumeMatch,
    ResumeMismatch,
    ReplayMatch,
    ReplayMismatch,
    ShrinkAccepted,
    BoundSkip,
    HarnessError,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticEvidence {
    pub class: EvidenceClass,
    pub profile_identity: String,
    pub case_blake3: String,
    pub raw_observation_refs: Vec<String>,
    pub role: String,
    pub non_claims: Vec<String>,
}
impl DiagnosticEvidence {
    pub fn validate(&self) -> Result<(), String> {
        if self.role != "host-side-diagnostic" {
            return Err("evidence-overclaim".into());
        }
        validate_hex(
            "profile identity",
            &self.profile_identity,
            BLAKE3_HEX_LENGTH,
        )?;
        validate_hex("case identity", &self.case_blake3, BLAKE3_HEX_LENGTH)?;
        if self.raw_observation_refs.is_empty() {
            return Err("evidence-missing-observations".into());
        }
        for reference in &self.raw_observation_refs {
            validate_hex("raw observation identity", reference, BLAKE3_HEX_LENGTH)?;
        }
        validate_exact_set(
            "evidence non_claims",
            &self.non_claims,
            &REQUIRED_NON_CLAIMS,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailurePredicate {
    pub profile_identity: String,
    pub spacewasm_runtime: String,
    pub wasmtime_runtime: String,
    pub boundary: CaseKind,
    pub class: EvidenceClass,
    pub first_difference: FirstDifference,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkStep {
    pub parent_blake3: String,
    pub candidate_blake3: String,
    pub predicate: FailurePredicate,
}

pub fn admit_shrink(
    profile: &DifferentialProfile,
    parent: &[u8],
    candidate: &[u8],
    expected: &FailurePredicate,
    first: &FailurePredicate,
    replay: &FailurePredicate,
) -> Result<ShrinkStep, String> {
    if candidate.len() >= parent.len()
        || expected != first
        || first != replay
        || expected.profile_identity != profile_identity(profile)?
        || expected.spacewasm_runtime != profile.spacewasm_runner_blake3
        || expected.wasmtime_runtime != profile.wasmtime_version
        || !matches!(
            expected.class,
            EvidenceClass::RuntimeDivergence | EvidenceClass::ResumeMismatch
        )
    {
        return Err("shrink-predicate-drift".into());
    }
    for bytes in [parent, candidate] {
        match (
            expected.boundary,
            crate::admission::admit_module(profile, bytes),
        ) {
            (CaseKind::Execute, Ok(_)) => {}
            (CaseKind::Reject, Err(denial))
                if denial.class == crate::admission::AdmissionClass::Malformed => {}
            _ => return Err("shrink-boundary-class-drift".into()),
        }
    }
    Ok(ShrinkStep {
        parent_blake3: blake3::hash(parent).to_hex().to_string(),
        candidate_blake3: blake3::hash(candidate).to_hex().to_string(),
        predicate: expected.clone(),
    })
}

pub fn compare_resume(
    uninterrupted: &NormalizedObservation,
    segmented: &NormalizedObservation,
) -> Result<EvidenceClass, String> {
    if uninterrupted.engine != "spacewasm"
        || segmented.engine != "spacewasm"
        || matches!(
            uninterrupted.outcome,
            OutcomeClass::Rejected
                | OutcomeClass::TimedOut
                | OutcomeClass::EngineError
                | OutcomeClass::ResourceExhausted
        )
        || matches!(
            segmented.outcome,
            OutcomeClass::Rejected
                | OutcomeClass::TimedOut
                | OutcomeClass::EngineError
                | OutcomeClass::ResourceExhausted
        )
    {
        return Err("resume-terminal-observation-required".into());
    }
    Ok(if first_difference(uninterrupted, segmented).is_none() {
        EvidenceClass::ResumeMatch
    } else {
        EvidenceClass::ResumeMismatch
    })
}

pub fn admit_retention(
    profile: &DifferentialProfile,
    retained: usize,
    additional: usize,
) -> Result<(), EvidenceClass> {
    if retained
        .checked_add(additional)
        .is_none_or(|total| total > profile.bounds.maximum_retained_artifacts)
    {
        Err(EvidenceClass::BoundSkip)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedules_reject_omission_overlap_reorder_extension_and_wrong_bytes() {
        let bytes = b"module";
        let mut schedule = ChunkSchedule {
            module_blake3: blake3::hash(bytes).to_hex().to_string(),
            seed: 1,
            generator: "fixture".into(),
            expected_class: CaseKind::Execute,
            return_policy: "return-each-consumed-buffer-once".into(),
            chunks: vec![
                Chunk {
                    offset: 0,
                    length: 2,
                },
                Chunk {
                    offset: 2,
                    length: 4,
                },
            ],
        };
        assert!(schedule.validate(bytes, 2).is_ok());
        for chunks in [
            vec![Chunk {
                offset: 0,
                length: 5,
            }],
            vec![
                Chunk {
                    offset: 0,
                    length: 2,
                },
                Chunk {
                    offset: 1,
                    length: 4,
                },
            ],
            vec![
                Chunk {
                    offset: 2,
                    length: 4,
                },
                Chunk {
                    offset: 0,
                    length: 2,
                },
            ],
            vec![Chunk {
                offset: 0,
                length: 7,
            }],
        ] {
            schedule.chunks = chunks;
            assert!(schedule.validate(bytes, 2).is_err());
        }
    }
    #[test]
    fn corpus_replay_rejects_bytes_class_and_schedule_drift() {
        let profile = crate::tests::profile();
        let (manifest, bytes) = CaseManifest::generate(&profile, 0, CaseKind::Execute).unwrap();
        manifest.verify(&profile, &bytes).unwrap();
        let mut wrong = bytes.clone();
        wrong[0] ^= 1;
        assert_eq!(
            manifest.verify(&profile, &wrong).unwrap_err(),
            "corpus-replay-mismatch"
        );
        let mut drift = manifest.clone();
        drift.expected_class = CaseKind::Reject;
        assert_eq!(
            drift.verify(&profile, &bytes).unwrap_err(),
            "corpus-replay-mismatch"
        );
        let mut drift = manifest;
        drift.schedules[0].chunks[0].length -= 1;
        assert_eq!(
            drift.verify(&profile, &bytes).unwrap_err(),
            "corpus-replay-mismatch"
        );
    }
    #[test]
    fn resume_compares_terminal_facts_and_evidence_rejects_promoted_role() {
        let left = completed("spacewasm");
        let right = completed("spacewasm");
        assert_eq!(
            compare_resume(&left, &right).unwrap(),
            EvidenceClass::ResumeMatch
        );
        let mut changed = right;
        changed.return_values.push("i32:1".into());
        assert_eq!(
            compare_resume(&left, &changed).unwrap(),
            EvidenceClass::ResumeMismatch
        );
        let mut record = DiagnosticEvidence {
            class: EvidenceClass::RuntimeMatch,
            profile_identity: "a".repeat(64),
            case_blake3: "b".repeat(64),
            raw_observation_refs: vec!["c".repeat(64)],
            role: "host-side-diagnostic".into(),
            non_claims: REQUIRED_NON_CLAIMS.into_iter().map(String::from).collect(),
        };
        record.validate().unwrap();
        record.role = "production-readiness".into();
        assert_eq!(record.validate().unwrap_err(), "evidence-overclaim");
    }

    #[test]
    fn semantic_surfaces_diverge_but_raw_counters_do_not() {
        let baseline = completed("spacewasm");
        let mut raw_only = baseline.clone();
        raw_only.raw_counters.insert("fuel".into(), 99);
        assert_eq!(
            compare_resume(&baseline, &raw_only).unwrap(),
            EvidenceClass::ResumeMatch
        );
        let mut variants = vec![baseline.clone(); 5];
        variants[0].surfaces.validation_class = "rejected".into();
        variants[1].surfaces.link_class = "link-rejected".into();
        variants[2]
            .surfaces
            .preserves_output
            .push("canonical-output".into());
        variants[3]
            .surfaces
            .hostcall_transcript
            .push("ordered-host-effect".into());
        variants[4].surfaces.final_memory_blake3 = Some("a".repeat(64));
        for changed in variants {
            assert_eq!(
                compare_resume(&baseline, &changed).unwrap(),
                EvidenceClass::ResumeMismatch
            );
        }
    }

    #[test]
    fn shrink_replays_same_failure_and_rejects_earlier_boundary() {
        let profile = crate::tests::profile();
        let mut parent = generate_mvp_module(profile.seed, 0, 2);
        // A valid custom section grows the parent without changing execution.
        parent.extend_from_slice(&[0, 2, 1, b'x']);
        let candidate = generate_mvp_module(profile.seed, 0, 1);
        let predicate = FailurePredicate {
            profile_identity: profile_identity(&profile).unwrap(),
            spacewasm_runtime: profile.spacewasm_runner_blake3.clone(),
            wasmtime_runtime: profile.wasmtime_version.clone(),
            boundary: CaseKind::Execute,
            class: EvidenceClass::RuntimeDivergence,
            first_difference: FirstDifference {
                field: "return_values".into(),
                spacewasm: "[]".into(),
                wasmtime: "[1]".into(),
            },
        };
        let step = admit_shrink(
            &profile, &parent, &candidate, &predicate, &predicate, &predicate,
        )
        .unwrap();
        assert_eq!(
            step.candidate_blake3,
            blake3::hash(&candidate).to_hex().to_string()
        );
        let malformed = malformed_magic(&candidate);
        assert_eq!(
            admit_shrink(&profile, &parent, &malformed, &predicate, &predicate, &predicate)
                .unwrap_err(),
            "shrink-boundary-class-drift"
        );
        let mut drift = predicate.clone();
        drift.class = EvidenceClass::ResumeMismatch;
        assert_eq!(
            admit_shrink(&profile, &parent, &candidate, &predicate, &drift, &drift).unwrap_err(),
            "shrink-predicate-drift"
        );
        assert_eq!(
            admit_retention(&profile, profile.bounds.maximum_retained_artifacts, 1),
            Err(EvidenceClass::BoundSkip)
        );
    }
}
