//! Typed bound decisions.
//!
//! Every exploration dimension is bounded by a named profile field. Reaching
//! a bound stops the operation and yields a typed skip; limits are never
//! silently increased.

use serde::{Deserialize, Serialize};

use crate::case::ExplorationCase;
use crate::profile::ExplorationProfile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundSkip {
    pub field: String,
    pub limit: u64,
    pub actual: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum BoundDecision {
    Admitted,
    Skipped { skip: BoundSkip },
}

/// Decides one bound fact: `actual` within `limit` admits, anything beyond
/// yields a typed skip naming the profile bound field.
pub fn decide_bound(field: &str, limit: u64, actual: u64) -> BoundDecision {
    if actual <= limit {
        BoundDecision::Admitted
    } else {
        BoundDecision::Skipped {
            skip: BoundSkip {
                field: field.to_string(),
                limit,
                actual,
            },
        }
    }
}

/// Artifact size bound decision for a case artifact.
pub fn check_artifact_bytes(
    profile: &ExplorationProfile,
    artifact_size_bytes: u64,
) -> BoundDecision {
    decide_bound(
        "maximum_artifact_bytes",
        profile.bounds.maximum_artifact_bytes,
        artifact_size_bytes,
    )
}

/// WIT size bound decision for a case WIT package.
pub fn check_wit_bytes(profile: &ExplorationProfile, wit_size_bytes: u64) -> BoundDecision {
    decide_bound(
        "maximum_wit_bytes",
        profile.bounds.maximum_wit_bytes,
        wit_size_bytes,
    )
}

/// Structural-count bound decision (imports, exports, functions, types,
/// memories, tables) keyed by the profile bound field name.
pub fn check_structural_count(
    profile: &ExplorationProfile,
    field: &str,
    count: u64,
) -> Result<BoundDecision, String> {
    let limit = match field {
        "maximum_imports" => profile.bounds.maximum_imports,
        "maximum_exports" => profile.bounds.maximum_exports,
        "maximum_functions" => profile.bounds.maximum_functions,
        "maximum_types" => profile.bounds.maximum_types,
        "maximum_memories" => profile.bounds.maximum_memories,
        "maximum_tables" => profile.bounds.maximum_tables,
        unknown => return Err(format!("unknown structural bound field {unknown:?}")),
    };
    Ok(decide_bound(field, limit, count))
}

/// Bound facts a validated case must satisfy; returns the skips that would
/// block the case from deeper exploration lanes.
pub fn case_bound_skips(profile: &ExplorationProfile, case: &ExplorationCase) -> Vec<BoundSkip> {
    let mut skips = Vec::new();
    for decision in [
        check_artifact_bytes(profile, case.artifact_size_bytes),
        match case.wit_size_bytes {
            Some(wit_size_bytes) => check_wit_bytes(profile, wit_size_bytes),
            None => BoundDecision::Admitted,
        },
    ] {
        if let BoundDecision::Skipped { skip } = decision {
            skips.push(skip);
        }
    }
    skips
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::CorpusClass;
    use crate::profile::example_profile;

    #[test]
    fn within_bound_admits_and_beyond_bound_skips_with_the_field_named() {
        assert_eq!(
            decide_bound("maximum_samples", 10, 10),
            BoundDecision::Admitted
        );
        assert_eq!(
            decide_bound("maximum_samples", 10, 11),
            BoundDecision::Skipped {
                skip: BoundSkip {
                    field: "maximum_samples".to_string(),
                    limit: 10,
                    actual: 11,
                },
            }
        );
    }

    #[test]
    fn artifact_wit_and_structural_checks_use_their_profile_bounds() {
        let profile = example_profile();
        assert_eq!(
            check_artifact_bytes(&profile, profile.bounds.maximum_artifact_bytes),
            BoundDecision::Admitted
        );
        assert!(matches!(
            check_artifact_bytes(&profile, profile.bounds.maximum_artifact_bytes + 1),
            BoundDecision::Skipped { .. }
        ));
        assert_eq!(
            check_wit_bytes(&profile, profile.bounds.maximum_wit_bytes),
            BoundDecision::Admitted
        );
        assert_eq!(
            check_structural_count(&profile, "maximum_memories", 64).expect("known field"),
            BoundDecision::Admitted
        );
        assert!(check_structural_count(&profile, "maximum_gadgets", 1).is_err());
    }

    #[test]
    fn validated_case_bounds_surface_skips_for_deeper_lanes() {
        let profile = example_profile();
        let mut case = crate::case::example_case();
        assert!(case_bound_skips(&profile, &case).is_empty());
        case.corpus_class = CorpusClass::GeneratedValid;
        case.wit_size_bytes = Some(profile.bounds.maximum_wit_bytes + 1);
        let skips = case_bound_skips(&profile, &case);
        assert_eq!(skips.len(), 1);
        assert_eq!(skips[0].field, "maximum_wit_bytes");
    }
}
