//! Pure assertion-hit checkpoint selection.
//!
//! The VMM supplies ordered, catalog-bound assertion observations. This module
//! decides which observations are worth retaining as exploration checkpoints.
//! It performs no I/O, allocation beyond bounded collections, or clock access.

use chaoscontrol_protocol::identity::{AssertionFingerprint, AssertionKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Hard bound on observations considered for one branch.
pub const MAX_ASSERTION_HITS_PER_BRANCH: usize = 256;
/// Default per-branch capture allowance.
pub const DEFAULT_ASSERTION_CAPTURES_PER_BRANCH: u32 = 8;
/// Default per-campaign capture allowance.
pub const DEFAULT_ASSERTION_CAPTURES_PER_CAMPAIGN: u32 = 64;
/// Bound on distinct assertion identities tracked for rarity scoring.
pub const MAX_TRACKED_ASSERTION_IDENTITIES: usize = 4_096;

const NOVELTY_BONUS: f64 = 32.0;
const RARITY_NUMERATOR: f64 = 16.0;
const SOMETIMES_WEIGHT: f64 = 2.0;
const REACHABLE_WEIGHT: f64 = 1.0;
const SEED_JITTER_SCALE: f64 = 0.001;

/// Named finite limits for assertion-hit checkpoint selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssertionCheckpointLimits {
    pub max_hits_per_branch: usize,
    pub max_captures_per_branch: u32,
    pub max_captures_per_campaign: u32,
}

impl Default for AssertionCheckpointLimits {
    fn default() -> Self {
        Self {
            max_hits_per_branch: MAX_ASSERTION_HITS_PER_BRANCH,
            max_captures_per_branch: DEFAULT_ASSERTION_CAPTURES_PER_BRANCH,
            max_captures_per_campaign: DEFAULT_ASSERTION_CAPTURES_PER_CAMPAIGN,
        }
    }
}

impl AssertionCheckpointLimits {
    /// Validate that every limit is internally meaningful.
    pub fn validate(&self) -> Result<(), AssertionCheckpointError> {
        if self.max_hits_per_branch == 0 {
            return Err(AssertionCheckpointError::ZeroHitLimit);
        }
        if self.max_hits_per_branch > MAX_ASSERTION_HITS_PER_BRANCH {
            return Err(AssertionCheckpointError::HitLimitTooLarge);
        }
        if self.max_captures_per_branch == 0 {
            return Err(AssertionCheckpointError::ZeroBranchCaptureLimit);
        }
        if self.max_captures_per_campaign == 0 {
            return Err(AssertionCheckpointError::ZeroCampaignCaptureLimit);
        }
        Ok(())
    }
}

/// One catalog-bound assertion observation at a deterministic execution boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionHitObservation {
    pub run_id: [u8; 32],
    pub vm_index: usize,
    pub vcpu: usize,
    pub exit_count: u64,
    pub observed_tick: u64,
    pub fingerprint: AssertionFingerprint,
    pub kind: AssertionKind,
    pub condition: bool,
    pub compatibility_id: Option<u32>,
    pub guest: String,
    pub process_identity: Option<String>,
}

impl AssertionHitObservation {
    /// Exact duplicate key for one observed assertion hit.
    fn hit_key(&self) -> HitKey {
        HitKey {
            run_id: self.run_id,
            vm_index: self.vm_index,
            vcpu: self.vcpu,
            exit_count: self.exit_count,
            observed_tick: self.observed_tick,
            fingerprint: self.fingerprint,
        }
    }

    fn sort_key(&self) -> (u64, usize, usize, u64, [u8; 32]) {
        (
            self.observed_tick,
            self.vm_index,
            self.vcpu,
            self.exit_count,
            self.run_id,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct HitKey {
    run_id: [u8; 32],
    vm_index: usize,
    vcpu: usize,
    exit_count: u64,
    observed_tick: u64,
    fingerprint: AssertionFingerprint,
}

/// Why a hit was or was not selected for capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionCheckpointDecisionKind {
    Selected,
    Duplicate,
    IneligibleKind,
    Unsatisfied,
    BranchBudgetExhausted,
    CampaignBudgetExhausted,
}

/// One deterministic decision for one observed hit.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionCheckpointDecision {
    pub observation: AssertionHitObservation,
    pub prior_hits: u32,
    pub score: f64,
    pub reason: AssertionCheckpointDecisionKind,
}

/// Result of selecting checkpoints for one branch.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionCheckpointSelection {
    pub decisions: Vec<AssertionCheckpointDecision>,
    pub admitted_hits: usize,
    pub truncated: bool,
}

/// Deterministic assertion-hit checkpoint selector.
///
/// The selector persists campaign capture budget, prior hit counts, and exact
/// duplicate keys across branches. Per-branch capture budget resets on each
/// call to [`Self::select`].
#[derive(Debug, Clone)]
pub struct AssertionCheckpointSelector {
    limits: AssertionCheckpointLimits,
    seed: u64,
    campaign_captures: u32,
    hit_counts: BTreeMap<AssertionFingerprint, u32>,
    seen_hits: BTreeSet<HitKey>,
}

impl AssertionCheckpointSelector {
    pub fn new(
        limits: AssertionCheckpointLimits,
        seed: u64,
    ) -> Result<Self, AssertionCheckpointError> {
        limits.validate()?;
        Ok(Self {
            limits,
            seed,
            campaign_captures: 0,
            hit_counts: BTreeMap::new(),
            seen_hits: BTreeSet::new(),
        })
    }

    pub fn limits(&self) -> AssertionCheckpointLimits {
        self.limits
    }

    pub fn campaign_captures(&self) -> u32 {
        self.campaign_captures
    }

    pub fn prior_hits(&self, fingerprint: AssertionFingerprint) -> u32 {
        self.hit_counts.get(&fingerprint).copied().unwrap_or(0)
    }

    /// Export campaign-persistent selector state.
    pub fn state(&self) -> AssertionCheckpointSelectorState {
        AssertionCheckpointSelectorState {
            campaign_captures: self.campaign_captures,
            hit_counts: self
                .hit_counts
                .iter()
                .map(|(fingerprint, count)| (fingerprint.to_hex(), *count))
                .collect(),
        }
    }

    /// Restore campaign-persistent selector state.
    ///
    /// Exact duplicate keys are not retained across resume; prior hit counts
    /// and the campaign capture budget are.
    pub fn restore_state(
        &mut self,
        state: &AssertionCheckpointSelectorState,
    ) -> Result<(), AssertionCheckpointError> {
        state.validate()?;
        let mut hit_counts = BTreeMap::new();
        for (fingerprint, count) in &state.hit_counts {
            let fingerprint = AssertionFingerprint::from_hex(fingerprint)
                .map_err(|_| AssertionCheckpointError::InvalidFingerprint)?;
            hit_counts.insert(fingerprint, *count);
        }
        self.campaign_captures = state.campaign_captures;
        self.hit_counts = hit_counts;
        self.seen_hits.clear();
        Ok(())
    }

    /// Select capture candidates for one branch.
    pub fn select(
        &mut self,
        observations: &[AssertionHitObservation],
    ) -> AssertionCheckpointSelection {
        let mut ordered = observations.to_vec();
        ordered.sort_by_key(AssertionHitObservation::sort_key);

        let admitted = ordered.len().min(self.limits.max_hits_per_branch);
        let truncated = ordered.len() > admitted;
        let mut branch_captures = 0_u32;
        let mut decisions = Vec::with_capacity(admitted);

        for observation in ordered.into_iter().take(admitted) {
            let prior_hits = self.advance_hit_count(observation.fingerprint);
            let score = self.score(observation.fingerprint, prior_hits, observation.kind);

            if !eligible(&observation) {
                let reason = match observation.kind {
                    AssertionKind::Sometimes => AssertionCheckpointDecisionKind::Unsatisfied,
                    AssertionKind::Always | AssertionKind::Unreachable => {
                        AssertionCheckpointDecisionKind::IneligibleKind
                    }
                    AssertionKind::Reachable => {
                        unreachable!("reachable assertions are eligible")
                    }
                };
                decisions.push(AssertionCheckpointDecision {
                    observation,
                    prior_hits,
                    score,
                    reason,
                });
                continue;
            }

            if !self.seen_hits.insert(observation.hit_key()) {
                decisions.push(AssertionCheckpointDecision {
                    observation,
                    prior_hits,
                    score,
                    reason: AssertionCheckpointDecisionKind::Duplicate,
                });
                continue;
            }

            let reason = if branch_captures >= self.limits.max_captures_per_branch {
                AssertionCheckpointDecisionKind::BranchBudgetExhausted
            } else if self.campaign_captures >= self.limits.max_captures_per_campaign {
                AssertionCheckpointDecisionKind::CampaignBudgetExhausted
            } else {
                branch_captures = branch_captures.saturating_add(1);
                self.campaign_captures = self.campaign_captures.saturating_add(1);
                AssertionCheckpointDecisionKind::Selected
            };
            decisions.push(AssertionCheckpointDecision {
                observation,
                prior_hits,
                score,
                reason,
            });
        }

        AssertionCheckpointSelection {
            decisions,
            admitted_hits: admitted,
            truncated,
        }
    }

    fn advance_hit_count(&mut self, fingerprint: AssertionFingerprint) -> u32 {
        let prior = self.prior_hits(fingerprint);
        self.hit_counts.insert(fingerprint, prior.saturating_add(1));
        prior
    }

    fn score(
        &self,
        fingerprint: AssertionFingerprint,
        prior_hits: u32,
        kind: AssertionKind,
    ) -> f64 {
        let novelty = if prior_hits == 0 { NOVELTY_BONUS } else { 0.0 };
        let rarity = RARITY_NUMERATOR / (f64::from(prior_hits) + 1.0);
        let kind_weight = match kind {
            AssertionKind::Sometimes => SOMETIMES_WEIGHT,
            AssertionKind::Reachable => REACHABLE_WEIGHT,
            AssertionKind::Always | AssertionKind::Unreachable => 0.0,
        };
        novelty + rarity + kind_weight + self.seed_jitter(fingerprint)
    }

    fn seed_jitter(&self, fingerprint: AssertionFingerprint) -> f64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.seed.to_le_bytes());
        hasher.update(&fingerprint.0);
        let bytes = hasher.finalize();
        let mut head = [0_u8; 8];
        head.copy_from_slice(&bytes.as_bytes()[..8]);
        (u64::from_le_bytes(head) as f64) / (u64::MAX as f64) * SEED_JITTER_SCALE
    }
}

fn eligible(observation: &AssertionHitObservation) -> bool {
    match observation.kind {
        AssertionKind::Sometimes => observation.condition,
        AssertionKind::Reachable => true,
        AssertionKind::Always | AssertionKind::Unreachable => false,
    }
}

/// Exact assertion-hit provenance retained with a checkpoint frontier entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionCheckpointMetadata {
    /// Full admitted assertion fingerprint.
    pub fingerprint: AssertionFingerprint,
    /// Assertion kind at the hit.
    pub kind: AssertionKind,
    /// Condition result at the hit.
    pub condition: bool,
    /// VM slot that observed the hit.
    pub vm_index: u32,
    /// Active vCPU at the hit.
    pub vcpu: u32,
    /// VM exit count at the hit boundary.
    pub exit_count: u64,
    /// VM virtual TSC at the hit boundary.
    pub virtual_tsc: u64,
    /// Fault-engine run identity.
    pub run_id: [u8; 32],
    /// Fault-engine run sequence.
    pub run_sequence: u64,
    /// Global simulation tick at capture.
    pub observed_tick: u64,
}

/// Serializable selector state retained across checkpoint resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionCheckpointSelectorState {
    /// Captures already spent by the campaign.
    pub campaign_captures: u32,
    /// Prior hit count per assertion fingerprint hex.
    pub hit_counts: BTreeMap<String, u32>,
}

impl AssertionCheckpointSelectorState {
    /// Validate bounded, parseable state before restoring it.
    pub fn validate(&self) -> Result<(), AssertionCheckpointError> {
        if self.hit_counts.len() > MAX_TRACKED_ASSERTION_IDENTITIES {
            return Err(AssertionCheckpointError::TooManyTrackedIdentities);
        }
        for (fingerprint, count) in &self.hit_counts {
            AssertionFingerprint::from_hex(fingerprint)
                .map_err(|_| AssertionCheckpointError::InvalidFingerprint)?;
            if *count == 0 {
                return Err(AssertionCheckpointError::ZeroHitCount);
            }
        }
        Ok(())
    }
}

/// Validation failures for checkpoint limits and retained selector state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionCheckpointError {
    ZeroHitLimit,
    HitLimitTooLarge,
    ZeroBranchCaptureLimit,
    ZeroCampaignCaptureLimit,
    TooManyTrackedIdentities,
    InvalidFingerprint,
    ZeroHitCount,
}

impl core::fmt::Display for AssertionCheckpointError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::ZeroHitLimit => "assertion hit limit must be nonzero",
            Self::HitLimitTooLarge => "assertion hit limit exceeds the admitted maximum",
            Self::ZeroBranchCaptureLimit => "assertion branch capture limit must be nonzero",
            Self::ZeroCampaignCaptureLimit => "assertion campaign capture limit must be nonzero",
            Self::TooManyTrackedIdentities => "assertion hit identity count exceeds the bound",
            Self::InvalidFingerprint => "assertion hit identity is not a valid fingerprint",
            Self::ZeroHitCount => "assertion hit count must be nonzero",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AssertionCheckpointError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(byte: u8) -> AssertionFingerprint {
        AssertionFingerprint([byte; 32])
    }

    fn observation(
        tick: u64,
        vm: usize,
        fingerprint_byte: u8,
        kind: AssertionKind,
        condition: bool,
    ) -> AssertionHitObservation {
        AssertionHitObservation {
            run_id: [7; 32],
            vm_index: vm,
            vcpu: 0,
            exit_count: tick * 1_000 + vm as u64,
            observed_tick: tick,
            fingerprint: fingerprint(fingerprint_byte),
            kind,
            condition,
            compatibility_id: Some(u32::from(fingerprint_byte)),
            guest: "guest".to_string(),
            process_identity: None,
        }
    }

    fn limits(branch: u32, campaign: u32) -> AssertionCheckpointLimits {
        AssertionCheckpointLimits {
            max_hits_per_branch: 16,
            max_captures_per_branch: branch,
            max_captures_per_campaign: campaign,
        }
    }

    #[test]
    fn selects_rare_sometimes_and_reachable_hits_only() {
        let mut selector =
            AssertionCheckpointSelector::new(limits(8, 8), 42).expect("valid limits");
        let observations = vec![
            observation(10, 0, 1, AssertionKind::Sometimes, true),
            observation(11, 0, 2, AssertionKind::Reachable, true),
            observation(12, 0, 3, AssertionKind::Always, true),
            observation(13, 0, 4, AssertionKind::Sometimes, false),
        ];
        let selection = selector.select(&observations);
        let reasons = selection
            .decisions
            .iter()
            .map(|decision| decision.reason)
            .collect::<Vec<_>>();
        assert_eq!(
            reasons,
            vec![
                AssertionCheckpointDecisionKind::Selected,
                AssertionCheckpointDecisionKind::Selected,
                AssertionCheckpointDecisionKind::IneligibleKind,
                AssertionCheckpointDecisionKind::Unsatisfied,
            ]
        );
        assert_eq!(selector.campaign_captures(), 2);
    }

    #[test]
    fn duplicate_hits_are_suppressed_by_exact_key() {
        let mut selector =
            AssertionCheckpointSelector::new(limits(8, 8), 42).expect("valid limits");
        let hit = observation(10, 0, 1, AssertionKind::Sometimes, true);
        let selection = selector.select(&[hit.clone(), hit]);
        assert_eq!(
            selection
                .decisions
                .iter()
                .map(|decision| decision.reason)
                .collect::<Vec<_>>(),
            vec![
                AssertionCheckpointDecisionKind::Selected,
                AssertionCheckpointDecisionKind::Duplicate,
            ]
        );
        assert_eq!(selector.campaign_captures(), 1);
    }

    #[test]
    fn branch_budget_exhausts_before_campaign_budget() {
        let mut selector =
            AssertionCheckpointSelector::new(limits(1, 8), 42).expect("valid limits");
        let selection = selector.select(&[
            observation(10, 0, 1, AssertionKind::Sometimes, true),
            observation(11, 0, 2, AssertionKind::Sometimes, true),
        ]);
        assert_eq!(
            selection
                .decisions
                .iter()
                .map(|decision| decision.reason)
                .collect::<Vec<_>>(),
            vec![
                AssertionCheckpointDecisionKind::Selected,
                AssertionCheckpointDecisionKind::BranchBudgetExhausted,
            ]
        );
        assert_eq!(selector.campaign_captures(), 1);
    }

    #[test]
    fn campaign_budget_persists_across_branches() {
        let mut selector =
            AssertionCheckpointSelector::new(limits(8, 1), 42).expect("valid limits");
        let first = selector.select(&[observation(10, 0, 1, AssertionKind::Sometimes, true)]);
        assert_eq!(
            first.decisions[0].reason,
            AssertionCheckpointDecisionKind::Selected
        );
        let second = selector.select(&[observation(20, 0, 2, AssertionKind::Sometimes, true)]);
        assert_eq!(
            second.decisions[0].reason,
            AssertionCheckpointDecisionKind::CampaignBudgetExhausted
        );
        assert_eq!(selector.campaign_captures(), 1);
    }

    #[test]
    fn selection_is_deterministic_under_input_reordering() {
        let observations = vec![
            observation(12, 1, 3, AssertionKind::Sometimes, true),
            observation(10, 0, 1, AssertionKind::Sometimes, true),
            observation(11, 0, 2, AssertionKind::Reachable, true),
        ];
        let mut first = AssertionCheckpointSelector::new(limits(8, 8), 42).expect("valid limits");
        let mut second = AssertionCheckpointSelector::new(limits(8, 8), 42).expect("valid limits");
        let mut reordered = observations.clone();
        reordered.reverse();
        let left = first.select(&observations);
        let right = second.select(&reordered);
        assert_eq!(left, right);
    }

    #[test]
    fn novelty_and_rarity_rank_prior_hits() {
        let mut selector =
            AssertionCheckpointSelector::new(limits(8, 8), 42).expect("valid limits");
        let selection = selector.select(&[
            observation(10, 0, 1, AssertionKind::Sometimes, true),
            observation(11, 0, 1, AssertionKind::Sometimes, true),
            observation(12, 0, 2, AssertionKind::Sometimes, true),
        ]);
        assert!(selection.decisions[0].score > selection.decisions[1].score);
        assert!(selection.decisions[2].score > selection.decisions[1].score);
        assert_eq!(selector.prior_hits(fingerprint(1)), 2);
    }

    #[test]
    fn hit_limit_truncates_without_silent_selection() {
        let mut selector = AssertionCheckpointSelector::new(
            AssertionCheckpointLimits {
                max_hits_per_branch: 2,
                ..limits(8, 8)
            },
            42,
        )
        .expect("valid limits");
        let selection = selector.select(&[
            observation(10, 0, 1, AssertionKind::Sometimes, true),
            observation(11, 0, 2, AssertionKind::Sometimes, true),
            observation(12, 0, 3, AssertionKind::Sometimes, true),
        ]);
        assert_eq!(selection.admitted_hits, 2);
        assert!(selection.truncated);
        assert_eq!(selection.decisions.len(), 2);
        assert_eq!(selector.campaign_captures(), 2);
    }

    #[test]
    fn selector_state_round_trips_campaign_budget_and_hit_counts() {
        let mut selector =
            AssertionCheckpointSelector::new(limits(8, 1), 42).expect("valid limits");
        selector.select(&[observation(10, 0, 1, AssertionKind::Sometimes, true)]);
        let state = selector.state();
        assert_eq!(state.campaign_captures, 1);
        assert_eq!(state.hit_counts.get(&fingerprint(1).to_hex()), Some(&1));

        let mut resumed = AssertionCheckpointSelector::new(limits(8, 1), 42).expect("valid limits");
        resumed.restore_state(&state).expect("valid state");
        assert_eq!(resumed.campaign_captures(), 1);
        assert_eq!(resumed.prior_hits(fingerprint(1)), 1);
        let selection = resumed.select(&[observation(20, 0, 2, AssertionKind::Sometimes, true)]);
        assert_eq!(
            selection.decisions[0].reason,
            AssertionCheckpointDecisionKind::CampaignBudgetExhausted
        );
    }

    #[test]
    fn selector_state_rejects_invalid_identities_and_counts() {
        let invalid = AssertionCheckpointSelectorState {
            campaign_captures: 0,
            hit_counts: BTreeMap::from([("not-a-fingerprint".to_string(), 1)]),
        };
        assert_eq!(
            invalid.validate().unwrap_err(),
            AssertionCheckpointError::InvalidFingerprint
        );
        let zero = AssertionCheckpointSelectorState {
            campaign_captures: 0,
            hit_counts: BTreeMap::from([(fingerprint(1).to_hex(), 0)]),
        };
        assert_eq!(
            zero.validate().unwrap_err(),
            AssertionCheckpointError::ZeroHitCount
        );
    }

    #[test]
    fn zero_limits_are_rejected() {
        assert_eq!(
            AssertionCheckpointSelector::new(
                AssertionCheckpointLimits {
                    max_hits_per_branch: 0,
                    ..limits(8, 8)
                },
                42
            )
            .unwrap_err(),
            AssertionCheckpointError::ZeroHitLimit
        );
        assert_eq!(
            AssertionCheckpointSelector::new(limits(0, 8), 42).unwrap_err(),
            AssertionCheckpointError::ZeroBranchCaptureLimit
        );
        assert_eq!(
            AssertionCheckpointSelector::new(limits(8, 0), 42).unwrap_err(),
            AssertionCheckpointError::ZeroCampaignCaptureLimit
        );
    }
}
