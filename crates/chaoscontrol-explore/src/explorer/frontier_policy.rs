//! Frontier policy selection and product stop classes.
//!
//! Before cutover the legacy frontier stays the default; the Campaign-backed
//! candidate policy is selected explicitly. Both policies keep every product
//! stop meaning (plateau, finding, signal, maximum rounds) in ChaosControl, and
//! neither claims that exploration searched exhaustively.

use std::fmt;
use std::str::FromStr;

/// Stable name of the legacy in-process frontier policy.
pub const LEGACY_POLICY_NAME: &str = "legacy";
/// Stable name of the Campaign-backed candidate policy.
pub const CAMPAIGN_CANDIDATE_POLICY_NAME: &str = "campaign-candidate";
/// Report non-claim: every exploration stop is bounded sampling, never proof
/// that all behavior was explored.
pub const EXHAUSTIVE_SEARCH_NON_CLAIM: &str =
    "exploration is bounded adaptive sampling; no stop claims that all behavior was explored";

/// Which frontier policy selects the next expansion.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum FrontierPolicy {
    /// Legacy in-process epsilon-greedy frontier (pre-cutover default).
    #[default]
    Legacy,
    /// Campaign-backed candidate frontier with a durable selection journal.
    CampaignCandidate,
}

impl FrontierPolicy {
    /// Every policy in stable order.
    pub const ALL: [Self; 2] = [Self::Legacy, Self::CampaignCandidate];

    /// Stable policy name used by the CLI, profiles, checkpoints, and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => LEGACY_POLICY_NAME,
            Self::CampaignCandidate => CAMPAIGN_CANDIDATE_POLICY_NAME,
        }
    }

    /// True for the legacy policy (checkpoints omit the field for it).
    #[must_use]
    pub const fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy)
    }
}

impl fmt::Display for FrontierPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An unknown frontier policy name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownFrontierPolicy {
    /// The rejected name.
    pub name: String,
}

impl fmt::Display for UnknownFrontierPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown frontier policy {:?} (expected {LEGACY_POLICY_NAME} or {CAMPAIGN_CANDIDATE_POLICY_NAME})",
            self.name
        )
    }
}

impl std::error::Error for UnknownFrontierPolicy {}

impl FromStr for FrontierPolicy {
    type Err = UnknownFrontierPolicy;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|policy| policy.as_str() == name)
            .ok_or_else(|| UnknownFrontierPolicy {
                name: name.to_owned(),
            })
    }
}

/// Why an exploration run stopped. Product-owned meaning for both policies.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorationStop {
    /// No frontier entry remained (legacy: and corpus recycling found nothing).
    FrontierExhausted,
    /// `stale_round_limit` consecutive rounds found no new edge or bug.
    CoveragePlateau,
    /// A short run (`max_rounds < 10`) found a bug.
    BugFound,
    /// The operator requested shutdown.
    Interrupted,
    /// Every configured round ran.
    Completed,
}

impl ExplorationStop {
    /// Legacy dashboard and log name of the stop.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FrontierExhausted => "frontier_exhausted",
            Self::CoveragePlateau => "coverage_plateau",
            Self::BugFound => "bug_found",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for ExplorationStop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_names_round_trip_and_default_is_legacy() {
        assert_eq!(FrontierPolicy::default(), FrontierPolicy::Legacy);
        for policy in FrontierPolicy::ALL {
            assert_eq!(policy.as_str().parse::<FrontierPolicy>(), Ok(policy));
            let json = serde_json::to_string(&policy).unwrap();
            assert_eq!(json, format!("\"{}\"", policy.as_str()));
            assert_eq!(
                serde_json::from_str::<FrontierPolicy>(&json).unwrap(),
                policy
            );
        }
    }

    #[test]
    fn unknown_policy_names_are_rejected() {
        for name in ["", "Legacy", "campaign", "campaign_candidate", "legacy "] {
            let error = name.parse::<FrontierPolicy>().unwrap_err();
            assert_eq!(error.name, name);
            assert!(serde_json::from_str::<FrontierPolicy>(&format!("\"{name}\"")).is_err());
        }
    }
}
