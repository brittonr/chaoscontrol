//! Frontier policy selection and product stop classes.
//!
//! After the Campaign cutover the Campaign-backed policy is the default for
//! configs, the CLI, profiles, and checkpoints that predate the policy field.
//! The legacy in-process frontier survives only as the explicitly selected
//! `legacy-diagnostic` rollback: it prints a diagnostic banner, writes a
//! `frontier-policy.json` receipt, and marks `checkpoint.json` and `report.txt`
//! release-ineligible so release, dogfood, and KVM-matrix gates refuse its
//! outputs. Both policies keep every product stop meaning (plateau, finding,
//! signal, maximum rounds) in ChaosControl, and neither claims that
//! exploration searched exhaustively.

use std::borrow::Cow;
use std::fmt;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use chaoscontrol_evidence::frontier_policy_gate::{
    CAMPAIGN_FRONTIER_POLICY, FRONTIER_POLICY_RECEIPT_FILE, FRONTIER_POLICY_RECEIPT_SCHEMA,
    LEGACY_DIAGNOSTIC_FRONTIER_POLICY,
};

use super::ExplorerConfig;

/// Stable name of the default Campaign-backed frontier policy.
pub const CAMPAIGN_POLICY_NAME: &str = CAMPAIGN_FRONTIER_POLICY;
/// Stable name of the explicit legacy diagnostic rollback policy.
pub const LEGACY_DIAGNOSTIC_POLICY_NAME: &str = LEGACY_DIAGNOSTIC_FRONTIER_POLICY;
/// Pre-cutover name of the legacy policy. Rejected so the legacy frontier can
/// never be selected implicitly by an old command line or profile.
pub const RETIRED_LEGACY_POLICY_NAME: &str = "legacy";
/// Pre-cutover name of the Campaign-backed policy.
pub const RETIRED_CAMPAIGN_CANDIDATE_POLICY_NAME: &str = "campaign-candidate";
/// Report non-claim: every exploration stop is bounded sampling, never proof
/// that all behavior was explored.
pub const EXHAUSTIVE_SEARCH_NON_CLAIM: &str =
    "exploration is bounded adaptive sampling; no stop claims that all behavior was explored";
/// Banner every legacy-diagnostic run prints before any VM work.
pub const LEGACY_DIAGNOSTIC_BANNER: &str = "\
══════════════════ LEGACY-DIAGNOSTIC FRONTIER POLICY ══════════════════
  This run uses the retired legacy in-process frontier as explicit
  diagnostic rollback. Its outputs are NOT release eligible:
  frontier-policy.json, checkpoint.json, and report.txt are marked
  release_eligible = false, and the dogfood, release, and KVM-matrix
  gates refuse them. Omit --frontier-policy (default: campaign) for
  supported exploration.
═══════════════════════════════════════════════════════════════════════";
/// Notice recorded in every legacy-diagnostic `frontier-policy.json` receipt.
pub const LEGACY_DIAGNOSTIC_RECEIPT_NOTICE: &str =
    "legacy-diagnostic is explicit rollback only; these outputs cannot satisfy release, dogfood, or KVM-matrix gates";

/// Which frontier policy selects the next expansion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrontierPolicy {
    /// Campaign-backed frontier with a durable selection journal (default).
    #[default]
    Campaign,
    /// Legacy in-process epsilon-greedy frontier, retained only as explicit
    /// diagnostic rollback. Its outputs are never release eligible.
    LegacyDiagnostic,
}

impl FrontierPolicy {
    /// Every policy in stable order.
    pub const ALL: [Self; 2] = [Self::Campaign, Self::LegacyDiagnostic];

    /// Stable policy name used by the CLI, profiles, checkpoints, and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Campaign => CAMPAIGN_POLICY_NAME,
            Self::LegacyDiagnostic => LEGACY_DIAGNOSTIC_POLICY_NAME,
        }
    }

    /// True for the default Campaign policy (checkpoints and progress omit
    /// the field for it, so pre-policy files load as Campaign).
    #[must_use]
    pub const fn is_campaign(&self) -> bool {
        matches!(self, Self::Campaign)
    }

    /// True for the explicit legacy diagnostic rollback policy.
    #[must_use]
    pub const fn is_legacy_diagnostic(self) -> bool {
        matches!(self, Self::LegacyDiagnostic)
    }

    /// Whether outputs of this policy may satisfy release, dogfood, and
    /// KVM-matrix gates.
    #[must_use]
    pub const fn release_eligible(self) -> bool {
        matches!(self, Self::Campaign)
    }
}

impl fmt::Display for FrontierPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A frontier policy name that does not select a policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrontierPolicyParseError {
    /// The pre-cutover `legacy` spelling: the legacy frontier is diagnostic
    /// rollback only and must be named `legacy-diagnostic` explicitly.
    RetiredLegacy,
    /// The pre-cutover `campaign-candidate` spelling of `campaign`.
    RetiredCampaignCandidate,
    /// Any other name.
    Unknown {
        /// The rejected name.
        name: String,
    },
}

impl fmt::Display for FrontierPolicyParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RetiredLegacy => write!(
                formatter,
                "frontier policy {RETIRED_LEGACY_POLICY_NAME:?} was retired at the Campaign cutover; the legacy frontier is diagnostic rollback only: select {LEGACY_DIAGNOSTIC_POLICY_NAME:?} explicitly (its outputs are not release eligible) or omit the policy for the default {CAMPAIGN_POLICY_NAME:?}"
            ),
            Self::RetiredCampaignCandidate => write!(
                formatter,
                "frontier policy {RETIRED_CAMPAIGN_CANDIDATE_POLICY_NAME:?} was renamed at the Campaign cutover; use {CAMPAIGN_POLICY_NAME:?} (the default)"
            ),
            Self::Unknown { name } => write!(
                formatter,
                "unknown frontier policy {name:?} (expected {CAMPAIGN_POLICY_NAME:?} or {LEGACY_DIAGNOSTIC_POLICY_NAME:?})"
            ),
        }
    }
}

impl std::error::Error for FrontierPolicyParseError {}

impl FromStr for FrontierPolicy {
    type Err = FrontierPolicyParseError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if let Some(policy) = Self::ALL.into_iter().find(|policy| policy.as_str() == name) {
            return Ok(policy);
        }
        Err(match name {
            RETIRED_LEGACY_POLICY_NAME => FrontierPolicyParseError::RetiredLegacy,
            RETIRED_CAMPAIGN_CANDIDATE_POLICY_NAME => {
                FrontierPolicyParseError::RetiredCampaignCandidate
            }
            _ => FrontierPolicyParseError::Unknown {
                name: name.to_owned(),
            },
        })
    }
}

/// Checkpoints, profiles, progress files, and traces parse through
/// [`FrontierPolicy::from_str`], so retired names fail with the same typed
/// message as the CLI.
impl<'de> serde::Deserialize<'de> for FrontierPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = Cow::<'de, str>::deserialize(deserializer)?;
        name.parse().map_err(serde::de::Error::custom)
    }
}

/// `frontier-policy.json`: written under `output_dir` by every
/// legacy-diagnostic run before any VM work.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontierPolicyReceipt {
    /// [`FRONTIER_POLICY_RECEIPT_SCHEMA`].
    pub schema: String,
    /// Policy of the run.
    pub frontier_policy: FrontierPolicy,
    /// Always `false` for a diagnostic run.
    pub release_eligible: bool,
    /// Master seed of the run.
    pub seed: u64,
    /// Why the outputs cannot satisfy release gates.
    pub notice: String,
}

impl FrontierPolicyReceipt {
    /// Receipt for a run configured with `config`.
    #[must_use]
    pub fn for_config(config: &ExplorerConfig) -> Self {
        Self {
            schema: FRONTIER_POLICY_RECEIPT_SCHEMA.to_owned(),
            frontier_policy: config.frontier_policy,
            release_eligible: config.frontier_policy.release_eligible(),
            seed: config.seed,
            notice: LEGACY_DIAGNOSTIC_RECEIPT_NOTICE.to_owned(),
        }
    }

    /// Durably replace `output_dir/frontier-policy.json` with this receipt.
    pub fn write(&self, output_dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let mut temporary = tempfile::NamedTempFile::new_in(output_dir)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(output_dir.join(FRONTIER_POLICY_RECEIPT_FILE))
            .map_err(|error| error.error)?;
        std::fs::File::open(output_dir)?.sync_all()
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

    const UNKNOWN_NAMES: [&str; 6] = [
        "",
        "Legacy",
        "Campaign",
        "campaign_candidate",
        "legacy ",
        "exhaustive",
    ];

    // r[verify chaoscontrol.campaign_policy.cutover.valid]
    #[test]
    fn campaign_is_the_default_and_names_round_trip() {
        assert_eq!(FrontierPolicy::default(), FrontierPolicy::Campaign);
        assert!(FrontierPolicy::Campaign.release_eligible());
        assert!(!FrontierPolicy::LegacyDiagnostic.release_eligible());
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

    // r[verify chaoscontrol.campaign_policy.rollback.invalid]
    #[test]
    fn retired_legacy_spelling_is_rejected_and_names_the_diagnostic_value() {
        let error = RETIRED_LEGACY_POLICY_NAME
            .parse::<FrontierPolicy>()
            .unwrap_err();
        assert_eq!(error, FrontierPolicyParseError::RetiredLegacy);
        assert!(
            error.to_string().contains("\"legacy-diagnostic\""),
            "{error}"
        );
        let serde_error = serde_json::from_str::<FrontierPolicy>("\"legacy\"").unwrap_err();
        assert!(
            serde_error.to_string().contains("\"legacy-diagnostic\""),
            "{serde_error}"
        );

        let error = RETIRED_CAMPAIGN_CANDIDATE_POLICY_NAME
            .parse::<FrontierPolicy>()
            .unwrap_err();
        assert_eq!(error, FrontierPolicyParseError::RetiredCampaignCandidate);
        assert!(error.to_string().contains("\"campaign\""), "{error}");
    }

    #[test]
    fn unknown_policy_names_are_rejected() {
        for name in UNKNOWN_NAMES {
            let error = name.parse::<FrontierPolicy>().unwrap_err();
            assert_eq!(
                error,
                FrontierPolicyParseError::Unknown {
                    name: name.to_owned()
                }
            );
            assert!(serde_json::from_str::<FrontierPolicy>(&format!("\"{name}\"")).is_err());
        }
    }

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn diagnostic_receipt_is_release_ineligible_and_round_trips() {
        let config = ExplorerConfig {
            frontier_policy: FrontierPolicy::LegacyDiagnostic,
            seed: 7,
            ..ExplorerConfig::default()
        };
        let receipt = FrontierPolicyReceipt::for_config(&config);
        assert!(!receipt.release_eligible);
        assert_eq!(receipt.schema, FRONTIER_POLICY_RECEIPT_SCHEMA);
        let directory = tempfile::tempdir().unwrap();
        receipt.write(directory.path()).unwrap();
        let text =
            std::fs::read_to_string(directory.path().join(FRONTIER_POLICY_RECEIPT_FILE)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["frontier_policy"], LEGACY_DIAGNOSTIC_POLICY_NAME);
        assert_eq!(value["release_eligible"], false);
        assert_eq!(
            serde_json::from_str::<FrontierPolicyReceipt>(&text).unwrap(),
            receipt
        );
    }
}
