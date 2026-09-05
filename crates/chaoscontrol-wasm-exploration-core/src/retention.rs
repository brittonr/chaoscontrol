//! Deterministic retention planning for exploration artifacts.
//!
//! The caller supplies artifact identifiers in explicit priority order; the
//! plan keeps the first `maximum_retained_artifacts` and names the rest as
//! skipped. No recency, clock, or filesystem facts participate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPlan {
    pub retained: Vec<String>,
    pub skipped: Vec<String>,
    pub maximum_retained_artifacts: usize,
}

/// Plans retention for one exploration lane. Duplicate identifiers and a zero
/// retention bound are rejected; otherwise the prefix of the given order is
/// retained and the remainder skipped.
pub fn plan_retention(
    artifact_ids: &[String],
    maximum_retained_artifacts: usize,
) -> Result<RetentionPlan, String> {
    if maximum_retained_artifacts == 0 {
        return Err("retention bound maximum_retained_artifacts must be positive".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    for artifact_id in artifact_ids {
        if !seen.insert(artifact_id) {
            return Err(format!("duplicate retention artifact {artifact_id:?}"));
        }
    }
    let retained = artifact_ids
        .iter()
        .take(maximum_retained_artifacts)
        .cloned()
        .collect();
    let skipped = artifact_ids
        .iter()
        .skip(maximum_retained_artifacts)
        .cloned()
        .collect();
    Ok(RetentionPlan {
        retained,
        skipped,
        maximum_retained_artifacts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn retention_keeps_the_priority_prefix_and_names_the_rest() {
        let plan = plan_retention(&ids(&["a", "b", "c"]), 2).expect("plan");
        assert_eq!(plan.retained, ids(&["a", "b"]));
        assert_eq!(plan.skipped, ids(&["c"]));
        assert_eq!(plan.maximum_retained_artifacts, 2);
    }

    #[test]
    fn retention_is_deterministic_for_identical_inputs() {
        let inputs = ids(&["case-1", "case-2", "case-3", "case-4"]);
        let first = plan_retention(&inputs, 3).expect("plan");
        let second = plan_retention(&inputs, 3).expect("plan");
        assert_eq!(first, second);
    }

    #[test]
    fn zero_bounds_and_duplicate_identifiers_are_rejected() {
        assert!(plan_retention(&ids(&["a"]), 0).is_err());
        assert!(plan_retention(&ids(&["a", "a"]), 2).is_err());
        let plan = plan_retention(&ids(&["a"]), 4).expect("plan under the bound");
        assert!(plan.skipped.is_empty());
        assert_eq!(plan.retained, ids(&["a"]));
    }
}
