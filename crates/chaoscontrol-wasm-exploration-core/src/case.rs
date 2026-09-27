//! Exploration case DTOs, admission validation, and seed/identity
//! construction.
//!
//! A case binds the generator cohort, seed, configuration, parent and
//! componentization inputs, expected validity class, and the exact output
//! BLAKE3, mirroring the corpus requirements of the rail specification.

use serde::{Deserialize, Serialize};

use crate::{blake3_hex, canonical_json, is_blake3_hex, is_identifier, ExplorationProfile};

pub const MAXIMUM_SEED_BYTES: usize = 64;
const SEED_DOMAIN: &[u8] = b"chaoscontrol.wasm-exploration.seed.v1";

/// Which pinned generator produced the case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorKind {
    WasmSmith,
    WitSmith,
    WasmToolsComponent,
}

impl GeneratorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GeneratorKind::WasmSmith => "wasm-smith",
            GeneratorKind::WitSmith => "wit-smith",
            GeneratorKind::WasmToolsComponent => "wasm-tools-component",
        }
    }
}

/// Which corpus lane the case belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CorpusClass {
    BaselineComponent,
    BaselineWitPackage,
    GeneratedMalformed,
    GeneratedValid,
    MutatedInvalid,
    MutatedValid,
}

impl CorpusClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            CorpusClass::BaselineComponent => "baseline-component",
            CorpusClass::BaselineWitPackage => "baseline-wit-package",
            CorpusClass::GeneratedMalformed => "generated-malformed",
            CorpusClass::GeneratedValid => "generated-valid",
            CorpusClass::MutatedInvalid => "mutated-invalid",
            CorpusClass::MutatedValid => "mutated-valid",
        }
    }
}

/// The declared validity class the case is expected to exhibit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExpectedValidity {
    Malformed,
    Valid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationCase {
    pub case_id: String,
    pub corpus_class: CorpusClass,
    pub generator: GeneratorKind,
    pub generator_version: String,
    pub seed_hex: String,
    pub configuration_blake3: String,
    pub parent_case_id: Option<String>,
    pub componentization_blake3: Option<String>,
    pub expected_validity: ExpectedValidity,
    pub artifact_blake3: String,
    pub artifact_size_bytes: u64,
    pub wit_blake3: Option<String>,
    pub wit_size_bytes: Option<u64>,
}

fn decode_seed(seed_hex: &str) -> Result<Vec<u8>, String> {
    if seed_hex.is_empty()
        || !seed_hex.len().is_multiple_of(2)
        || seed_hex.len() > MAXIMUM_SEED_BYTES * 2
    {
        return Err(format!(
            "case seed_hex must be 1..={MAXIMUM_SEED_BYTES} even-length hex bytes"
        ));
    }
    let mut seed = Vec::with_capacity(seed_hex.len() / 2);
    let bytes = seed_hex.as_bytes();
    for pair in bytes.chunks(2) {
        let nibbles = pair
            .iter()
            .map(|byte| {
                (*byte as char)
                    .to_digit(16)
                    .ok_or_else(|| "case seed_hex is not hex".to_string())
            })
            .collect::<Result<Vec<u32>, String>>()?;
        seed.push(((nibbles[0] << 4) | nibbles[1]) as u8);
    }
    Ok(seed)
}

fn optional_blake3(field: &str, value: &Option<String>) -> Result<(), String> {
    match value {
        Some(value) if is_blake3_hex(value) => Ok(()),
        Some(value) => Err(format!("case {field} {value:?} is not a BLAKE3 hex digest")),
        None => Ok(()),
    }
}

/// Validates one case against format rules and the admitted profile cohort.
pub fn validate_case(case: &ExplorationCase, profile: &ExplorationProfile) -> Result<(), String> {
    if !is_identifier(&case.case_id) {
        return Err(format!(
            "case_id {:?} is not a bounded identifier",
            case.case_id
        ));
    }
    if case.generator_version.is_empty() || case.generator_version.len() > 64 {
        return Err("case generator_version must be a non-empty bounded string".to_string());
    }
    decode_seed(&case.seed_hex)?;
    if !is_blake3_hex(&case.configuration_blake3) {
        return Err("case configuration_blake3 is not a BLAKE3 hex digest".to_string());
    }
    if let Some(parent) = &case.parent_case_id {
        if !is_identifier(parent) {
            return Err(format!(
                "case parent_case_id {parent:?} is not a bounded identifier"
            ));
        }
    }
    optional_blake3("componentization_blake3", &case.componentization_blake3)?;
    if !is_blake3_hex(&case.artifact_blake3) {
        return Err("case artifact_blake3 is not a BLAKE3 hex digest".to_string());
    }
    if case.artifact_size_bytes == 0
        || case.artifact_size_bytes > profile.bounds.maximum_artifact_bytes
    {
        return Err(format!(
            "case artifact_size_bytes {} exceeds the admitted artifact bound",
            case.artifact_size_bytes
        ));
    }
    match (&case.wit_blake3, &case.wit_size_bytes) {
        (None, None) => {}
        (Some(_), Some(wit_size)) => {
            if *wit_size == 0 || wit_size > &profile.bounds.maximum_wit_bytes {
                return Err(format!(
                    "case wit_size_bytes {wit_size} exceeds the admitted WIT bound"
                ));
            }
        }
        (None, Some(_)) | (Some(_), None) => {
            return Err("case WIT identity and size must be set together".to_string())
        }
    }
    if !profile
        .corpus_classes
        .iter()
        .any(|value| value == case.corpus_class.as_str())
    {
        return Err(format!(
            "case corpus class {:?} is not admitted by the profile",
            case.corpus_class.as_str()
        ));
    }
    if !profile
        .tool_cohort
        .generators
        .iter()
        .any(|value| value == case.generator.as_str())
    {
        return Err(format!(
            "case generator {:?} is not admitted by the profile",
            case.generator.as_str()
        ));
    }
    Ok(())
}

/// Deterministic BLAKE3 identity over the complete case facts.
pub fn case_identity(case: &ExplorationCase) -> Result<String, String> {
    Ok(blake3_hex(&canonical_json(case)?))
}

/// The verdict of replaying one corpus case from its declared manifest
/// facts against a regeneration of the same declared inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum CorpusReplayVerdict {
    Reproducible {
        case_identity_blake3: String,
    },
    ReplayMismatch {
        declared_identity_blake3: String,
        regenerated_identity_blake3: String,
    },
}

/// Verifies one corpus replay. Regeneration of the declared inputs must
/// reproduce the exact artifact bytes; anything else is the typed
/// `replay-mismatch` class and never counts as reproducible evidence.
/// Comparing cases with different declared inputs is a harness error, not a
/// mismatch, because it verifies nothing about reproducibility.
pub fn verify_corpus_replay(
    declared: &ExplorationCase,
    regenerated: &ExplorationCase,
) -> Result<CorpusReplayVerdict, String> {
    let manifest_inputs_equal = declared.case_id == regenerated.case_id
        && declared.corpus_class == regenerated.corpus_class
        && declared.generator == regenerated.generator
        && declared.generator_version == regenerated.generator_version
        && declared.seed_hex == regenerated.seed_hex
        && declared.configuration_blake3 == regenerated.configuration_blake3
        && declared.parent_case_id == regenerated.parent_case_id
        && declared.componentization_blake3 == regenerated.componentization_blake3
        && declared.expected_validity == regenerated.expected_validity
        && declared.wit_blake3 == regenerated.wit_blake3
        && declared.wit_size_bytes == regenerated.wit_size_bytes;
    if !manifest_inputs_equal {
        return Err(
            "corpus replay must compare one case against a regeneration of the same declared inputs"
                .to_string(),
        );
    }
    let declared_identity_blake3 = case_identity(declared)?;
    let regenerated_identity_blake3 = case_identity(regenerated)?;
    if declared_identity_blake3 == regenerated_identity_blake3 {
        Ok(CorpusReplayVerdict::Reproducible {
            case_identity_blake3: declared_identity_blake3,
        })
    } else {
        Ok(CorpusReplayVerdict::ReplayMismatch {
            declared_identity_blake3,
            regenerated_identity_blake3,
        })
    }
}

/// Seed material binds the seed bytes to the configuration, parent, and
/// componentization identity inputs through one domain-separated digest, so
/// two cases sharing a seed cannot share derived generator material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedMaterial {
    pub seed_hex: String,
    pub derivation_blake3: String,
}

pub fn derive_seed_material(case: &ExplorationCase) -> Result<SeedMaterial, String> {
    let seed = decode_seed(&case.seed_hex)?;
    let mut frame =
        Vec::with_capacity(SEED_DOMAIN.len() + seed.len() + 2 * crate::BLAKE3_HEX_LENGTH);
    frame.extend_from_slice(SEED_DOMAIN);
    frame.extend_from_slice(&seed);
    frame.extend_from_slice(case.configuration_blake3.as_bytes());
    if let Some(componentization) = &case.componentization_blake3 {
        frame.extend_from_slice(componentization.as_bytes());
    }
    if let Some(parent) = &case.parent_case_id {
        frame.extend_from_slice(parent.as_bytes());
    }
    Ok(SeedMaterial {
        seed_hex: case.seed_hex.clone(),
        derivation_blake3: blake3_hex(&frame),
    })
}

#[cfg(test)]
pub(crate) fn example_case() -> ExplorationCase {
    ExplorationCase {
        case_id: "component-case-0001".to_string(),
        corpus_class: CorpusClass::GeneratedValid,
        generator: GeneratorKind::WasmSmith,
        generator_version: "wasm-smith 1.245.1".to_string(),
        seed_hex: "00112233445566778899aabbccddeeff".to_string(),
        configuration_blake3: crate::blake3_hex(b"generation-configuration"),
        parent_case_id: None,
        componentization_blake3: Some(crate::blake3_hex(b"wit-componentization-inputs")),
        expected_validity: ExpectedValidity::Valid,
        artifact_blake3: crate::blake3_hex(b"generated-component-bytes"),
        artifact_size_bytes: 512,
        wit_blake3: Some(crate::blake3_hex(b"wit-package-bytes")),
        wit_size_bytes: Some(256),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::example_profile;

    #[test]
    fn valid_case_admits_with_stable_identity_and_seed_material() {
        let profile = example_profile();
        let case = example_case();
        validate_case(&case, &profile).expect("example case must validate");
        let first = case_identity(&case).expect("identity");
        let second = case_identity(&case).expect("identity");
        assert_eq!(first, second);

        let material = derive_seed_material(&case).expect("seed material");
        let again = derive_seed_material(&case).expect("seed material");
        assert_eq!(material, again);
        assert_eq!(material.seed_hex, case.seed_hex);
    }

    #[test]
    fn seed_material_separates_cases_that_share_a_seed() {
        let profile = example_profile();
        let shared_seed = example_case();
        let mut other_config = example_case();
        other_config.case_id = "component-case-0002".to_string();
        other_config.configuration_blake3 = crate::blake3_hex(b"other-configuration");
        validate_case(&shared_seed, &profile).expect("first case");
        validate_case(&other_config, &profile).expect("second case");
        let first = derive_seed_material(&shared_seed).expect("seed material");
        let second = derive_seed_material(&other_config).expect("seed material");
        assert_ne!(first.derivation_blake3, second.derivation_blake3);
    }

    #[test]
    fn corpus_replay_of_identical_regeneration_is_reproducible() {
        let declared = example_case();
        let regenerated = example_case();
        match verify_corpus_replay(&declared, &regenerated).expect("verdict") {
            CorpusReplayVerdict::Reproducible {
                case_identity_blake3,
            } => {
                assert_eq!(
                    case_identity_blake3,
                    case_identity(&declared).expect("identity")
                );
            }
            CorpusReplayVerdict::ReplayMismatch { .. } => {
                panic!("identical regeneration must not be a replay mismatch");
            }
        }
    }

    #[test]
    fn corpus_replay_with_divergent_bytes_is_the_replay_mismatch_class() {
        let declared = example_case();
        let mut regenerated = example_case();
        regenerated.artifact_blake3 = crate::blake3_hex(b"regenerated-different-bytes");
        match verify_corpus_replay(&declared, &regenerated).expect("verdict") {
            CorpusReplayVerdict::Reproducible { .. } => {
                panic!("divergent regeneration must not be reproducible");
            }
            CorpusReplayVerdict::ReplayMismatch {
                declared_identity_blake3,
                regenerated_identity_blake3,
            } => {
                assert_eq!(
                    declared_identity_blake3,
                    case_identity(&declared).expect("identity")
                );
                assert_eq!(
                    regenerated_identity_blake3,
                    case_identity(&regenerated).expect("identity")
                );
                assert_ne!(declared_identity_blake3, regenerated_identity_blake3);
            }
        }
    }

    #[test]
    fn corpus_replay_across_different_declared_inputs_is_a_harness_error() {
        let declared = example_case();
        let mut regenerated = example_case();
        regenerated.seed_hex = "ff112233445566778899aabbccddeeff".to_string();
        let error = verify_corpus_replay(&declared, &regenerated)
            .expect_err("different declared inputs must not verify");
        assert!(error.contains("same declared inputs"));
    }

    #[test]
    fn unadmitted_cohort_facts_are_rejected() {
        let profile = example_profile();

        let mut case = example_case();
        case.generator = GeneratorKind::WitSmith;
        case.generator_version = "wit-smith 1.245.1".to_string();
        validate_case(&case, &profile).expect("wit-smith is admitted");
        case.generator_version = String::new();
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.corpus_class = CorpusClass::BaselineComponent;
        validate_case(&case, &profile).expect("baseline class is admitted");

        let mut profile = example_profile();
        profile.tool_cohort.generators = vec!["wasm-smith".to_string()];
        let mut case = example_case();
        case.generator = GeneratorKind::WitSmith;
        assert!(validate_case(&case, &profile).is_err());
    }

    #[test]
    fn oversized_artifacts_and_wit_are_rejected() {
        let profile = example_profile();

        let mut case = example_case();
        case.artifact_size_bytes = profile.bounds.maximum_artifact_bytes + 1;
        assert!(validate_case(&case, &profile).is_err());
        case.artifact_size_bytes = profile.bounds.maximum_artifact_bytes;
        validate_case(&case, &profile).expect("exact bound admits");

        let mut case = example_case();
        case.wit_size_bytes = Some(profile.bounds.maximum_wit_bytes + 1);
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.wit_size_bytes = Some(256);
        case.wit_blake3 = None;
        assert!(validate_case(&case, &profile).is_err());
    }

    #[test]
    fn malformed_identities_and_seeds_are_rejected() {
        let profile = example_profile();

        let mut case = example_case();
        case.artifact_blake3 = "deadbeef".to_string();
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.seed_hex = "nothex".to_string();
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.seed_hex = "abc".to_string();
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.seed_hex = String::new();
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.componentization_blake3 = Some("ZZZ".to_string());
        assert!(validate_case(&case, &profile).is_err());

        let mut case = example_case();
        case.parent_case_id = Some("Bad Parent".to_string());
        assert!(validate_case(&case, &profile).is_err());
    }
}
