//! Parity report: decision-field equality and semantic-work equality as
//! separate facts, with time and memory explicitly not measured
//! (`campaign.fd.evidence`). Any decision-field drift blocks cutover readiness
//! (`chaoscontrol.campaign_policy.frontier_parity.invalid`); semantic work is
//! measured and reported but never inferred from, or folded into, field
//! equality.

use super::drive::{RoundDecision, RunDecisions};
use crate::explorer::FrontierPolicy;

/// Schema of one parity report.
pub const PARITY_REPORT_SCHEMA: &str = "chaoscontrol.frontier-parity-report.v1";
/// Drifts a report lists in full; the count covers every drift.
pub const MAX_REPORTED_DRIFTS: usize = 64;

/// One structural decision the comparison checks.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionField {
    /// Retained entries after the root admission.
    RootRetained,
    /// Number of rounds that ran.
    RoundCount,
    /// Round sub-mode.
    Mode,
    /// Executed or skipped.
    Outcome,
    /// Selection draws consumed, and whether they matched the entropy source.
    Draws,
    /// Selected entry.
    SelectedEntry,
    /// Explore or exploit branch.
    ExploreFlag,
    /// Parent moment (branch result and depth) the selection restores.
    ParentMoment,
    /// Entries admitted, in push order.
    Admitted,
    /// Entries pruned.
    Pruned,
    /// Retained sequence numbers.
    RetainedSet,
    /// Per-entry selection counts.
    SelectionCounts,
    /// Per-entry score bits.
    RetainedScores,
    /// Selections and selection budget consumed.
    SelectionUse,
    /// RNG stream position after the round.
    RngPosition,
    /// Stop class.
    StopClass,
    /// Rounds completed.
    RoundsCompleted,
    /// A backend failure ended the run.
    BackendFailure,
}

/// Fields every comparison checks; [`DecisionField::RngPosition`] is added when
/// both sides expose a counted stream.
const BASE_FIELDS: [DecisionField; 17] = [
    DecisionField::RootRetained,
    DecisionField::RoundCount,
    DecisionField::Mode,
    DecisionField::Outcome,
    DecisionField::Draws,
    DecisionField::SelectedEntry,
    DecisionField::ExploreFlag,
    DecisionField::ParentMoment,
    DecisionField::Admitted,
    DecisionField::Pruned,
    DecisionField::RetainedSet,
    DecisionField::SelectionCounts,
    DecisionField::RetainedScores,
    DecisionField::SelectionUse,
    DecisionField::StopClass,
    DecisionField::RoundsCompleted,
    DecisionField::BackendFailure,
];

/// One field that differs.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FieldDrift {
    /// The differing field.
    pub field: DecisionField,
    /// Loop round, or `None` for run-level fields.
    pub round: Option<u64>,
    /// Left value.
    pub left: String,
    /// Right value.
    pub right: String,
}

/// Answer to one differential-evidence question.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EvidenceQuestion {
    /// Not measured; no equality may be inferred.
    NotMeasured,
    /// Measured by the comparison.
    Measured {
        /// Whether the measured values were equal.
        equal: bool,
    },
}

/// Which side of a comparison a run is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideRole {
    /// Driven live in this process from a script.
    Live,
    /// Read from a recorded trace.
    Recorded,
    /// A recorded run's observations replayed through a backend.
    Replayed,
}

/// One side of a comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PolicySide {
    /// Frontier policy that decided.
    pub policy: FrontierPolicy,
    /// How the decisions were obtained.
    pub role: SideRole,
}

/// Decision-field equality.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FieldQuestion {
    /// Every compared field is equal in every round.
    pub equal: bool,
    /// Total drifts found.
    pub drift_count: usize,
    /// The first [`MAX_REPORTED_DRIFTS`] drifts, in round then field order.
    pub drifts: Vec<FieldDrift>,
}

/// One semantic-work quantity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkQuantity {
    /// Candidate records the planner inspected.
    CandidatesExamined,
    /// Candidates eligible for selection.
    EligibleCandidates,
}

/// Totals of one work quantity over the rounds where both sides selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WorkFact {
    /// Quantity.
    pub quantity: WorkQuantity,
    /// Left total.
    pub left_total: u64,
    /// Right total.
    pub right_total: u64,
    /// Rounds where both sides reported work.
    pub rounds_measured: u64,
    /// Rounds whose values differ.
    pub differing_rounds: u64,
    /// First differing round.
    pub first_differing_round: Option<u64>,
    /// Whether every measured round is equal.
    pub equal: bool,
}

/// Semantic-work equality, measured separately from decision fields.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WorkQuestion {
    /// Measured answer.
    pub answer: EvidenceQuestion,
    /// Per-quantity facts.
    pub facts: Vec<WorkFact>,
}

/// Parity of one compared pair of runs.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ParityReport {
    /// [`PARITY_REPORT_SCHEMA`].
    pub schema: String,
    /// Corpus case or replay name.
    pub case: String,
    /// Left side.
    pub left: PolicySide,
    /// Right side.
    pub right: PolicySide,
    /// Rounds present on both sides.
    pub rounds_compared: u64,
    /// Fields compared.
    pub fields_compared: Vec<DecisionField>,
    /// Decision-field equality.
    pub decision_fields: FieldQuestion,
    /// Semantic-work equality (never inferred from field equality).
    pub semantic_work: WorkQuestion,
    /// Wall-clock time: not measured.
    pub time: EvidenceQuestion,
    /// Memory: not measured.
    pub memory: EvidenceQuestion,
}

/// Cutover is blocked: at least one decision field drifted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParityDrift {
    /// Case that drifted.
    pub case: String,
    /// Total drifts.
    pub drift_count: usize,
    /// Distinct drifted fields, ordered.
    pub fields: Vec<DecisionField>,
    /// First drift.
    pub first: Box<FieldDrift>,
}

impl std::fmt::Display for ParityDrift {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "frontier parity case {} drifted in {} field value(s) ({:?}); first: {:?} at round {:?}: {} != {}",
            self.case,
            self.drift_count,
            self.fields,
            self.first.field,
            self.first.round,
            self.first.left,
            self.first.right
        )
    }
}

impl std::error::Error for ParityDrift {}

impl ParityReport {
    /// Cutover readiness of this comparison: ready only when every decision
    /// field agrees. Semantic work is reported, not gated.
    ///
    /// # Errors
    /// Returns [`ParityDrift`] naming every drifted field.
    pub fn cutover_readiness(&self) -> Result<(), ParityDrift> {
        let Some(first) = self.decision_fields.drifts.first() else {
            return Ok(());
        };
        let mut fields: Vec<DecisionField> = self
            .decision_fields
            .drifts
            .iter()
            .map(|drift| drift.field)
            .collect();
        fields.sort_unstable();
        fields.dedup();
        Err(ParityDrift {
            case: self.case.clone(),
            drift_count: self.decision_fields.drift_count,
            fields,
            first: Box::new(first.clone()),
        })
    }
}

/// Collects drifts, keeping the first [`MAX_REPORTED_DRIFTS`].
struct Drifts {
    count: usize,
    listed: Vec<FieldDrift>,
}

impl Drifts {
    fn check<T: PartialEq + std::fmt::Debug>(
        &mut self,
        field: DecisionField,
        round: Option<u64>,
        left: &T,
        right: &T,
    ) {
        if left == right {
            return;
        }
        self.count = self.count.saturating_add(1);
        if self.listed.len() < MAX_REPORTED_DRIFTS {
            self.listed.push(FieldDrift {
                field,
                round,
                left: format!("{left:?}"),
                right: format!("{right:?}"),
            });
        }
    }
}

/// Accumulates one work quantity.
struct WorkTally {
    fact: WorkFact,
}

impl WorkTally {
    const fn new(quantity: WorkQuantity) -> Self {
        Self {
            fact: WorkFact {
                quantity,
                left_total: 0,
                right_total: 0,
                rounds_measured: 0,
                differing_rounds: 0,
                first_differing_round: None,
                equal: true,
            },
        }
    }

    fn add(&mut self, round: u64, left: u64, right: u64) {
        let fact = &mut self.fact;
        fact.left_total = fact.left_total.saturating_add(left);
        fact.right_total = fact.right_total.saturating_add(right);
        fact.rounds_measured = fact.rounds_measured.saturating_add(1);
        if left != right {
            fact.differing_rounds = fact.differing_rounds.saturating_add(1);
            fact.first_differing_round.get_or_insert(round);
            fact.equal = false;
        }
    }
}

fn retained_seqs(round: &RoundDecision) -> Vec<u64> {
    round.retained.iter().map(|entry| entry.seq).collect()
}

fn retained_counts(round: &RoundDecision) -> Vec<(u64, u32)> {
    round
        .retained
        .iter()
        .map(|entry| (entry.seq, entry.times_selected))
        .collect()
}

fn retained_scores(round: &RoundDecision) -> Vec<(u64, u64)> {
    round
        .retained
        .iter()
        .map(|entry| (entry.seq, entry.score_bits))
        .collect()
}

fn compare_round(drifts: &mut Drifts, left: &RoundDecision, right: &RoundDecision, rng: bool) {
    let round = Some(left.round);
    drifts.check(DecisionField::Mode, round, &left.mode, &right.mode);
    drifts.check(DecisionField::Outcome, round, &left.outcome, &right.outcome);
    drifts.check(
        DecisionField::Draws,
        round,
        &(left.draws, left.entropy_mismatch),
        &(right.draws, right.entropy_mismatch),
    );
    drifts.check(
        DecisionField::SelectedEntry,
        round,
        &left.selection.map(|selection| selection.seq),
        &right.selection.map(|selection| selection.seq),
    );
    drifts.check(
        DecisionField::ExploreFlag,
        round,
        &left.selection.map(|selection| selection.explore),
        &right.selection.map(|selection| selection.explore),
    );
    drifts.check(
        DecisionField::ParentMoment,
        round,
        &left.parent,
        &right.parent,
    );
    drifts.check(
        DecisionField::Admitted,
        round,
        &left.admitted,
        &right.admitted,
    );
    drifts.check(DecisionField::Pruned, round, &left.pruned, &right.pruned);
    drifts.check(
        DecisionField::RetainedSet,
        round,
        &retained_seqs(left),
        &retained_seqs(right),
    );
    drifts.check(
        DecisionField::SelectionCounts,
        round,
        &retained_counts(left),
        &retained_counts(right),
    );
    drifts.check(
        DecisionField::RetainedScores,
        round,
        &retained_scores(left),
        &retained_scores(right),
    );
    drifts.check(
        DecisionField::SelectionUse,
        round,
        &left.selection_use,
        &right.selection_use,
    );
    if rng {
        drifts.check(
            DecisionField::RngPosition,
            round,
            &left.rng_position,
            &right.rng_position,
        );
    }
}

/// Compares two runs field by field after every round, and their semantic
/// work separately.
// r[impl chaoscontrol.campaign_policy.frontier_parity]
pub(crate) fn compare_runs(
    case: &str,
    left_side: PolicySide,
    left: &RunDecisions,
    right_side: PolicySide,
    right: &RunDecisions,
) -> ParityReport {
    let rng = !left.rounds.is_empty()
        && left
            .rounds
            .iter()
            .chain(right.rounds.iter())
            .all(|round| round.rng_position.is_some());
    let mut drifts = Drifts {
        count: 0,
        listed: Vec::new(),
    };
    drifts.check(DecisionField::RootRetained, None, &left.root, &right.root);
    drifts.check(
        DecisionField::RoundCount,
        None,
        &left.rounds.len(),
        &right.rounds.len(),
    );
    let mut examined = WorkTally::new(WorkQuantity::CandidatesExamined);
    let mut eligible = WorkTally::new(WorkQuantity::EligibleCandidates);
    let mut rounds_compared = 0_u64;
    for (left_round, right_round) in left.rounds.iter().zip(right.rounds.iter()) {
        rounds_compared = rounds_compared.saturating_add(1);
        compare_round(&mut drifts, left_round, right_round, rng);
        if let (Some(left_work), Some(right_work)) = (left_round.work, right_round.work) {
            examined.add(
                left_round.round,
                left_work.candidates_examined,
                right_work.candidates_examined,
            );
            eligible.add(
                left_round.round,
                left_work.eligible_candidates,
                right_work.eligible_candidates,
            );
        }
    }
    drifts.check(DecisionField::StopClass, None, &left.stop, &right.stop);
    drifts.check(
        DecisionField::RoundsCompleted,
        None,
        &left.rounds_completed,
        &right.rounds_completed,
    );
    drifts.check(
        DecisionField::BackendFailure,
        None,
        &left.failure,
        &right.failure,
    );
    let mut fields_compared = BASE_FIELDS.to_vec();
    if rng {
        fields_compared.push(DecisionField::RngPosition);
        fields_compared.sort_unstable();
    }
    let facts = vec![examined.fact, eligible.fact];
    let work_equal = facts.iter().all(|fact| fact.equal);
    ParityReport {
        schema: PARITY_REPORT_SCHEMA.to_owned(),
        case: case.to_owned(),
        left: left_side,
        right: right_side,
        rounds_compared,
        fields_compared,
        decision_fields: FieldQuestion {
            equal: drifts.count == 0,
            drift_count: drifts.count,
            drifts: drifts.listed,
        },
        semantic_work: WorkQuestion {
            answer: EvidenceQuestion::Measured { equal: work_equal },
            facts,
        },
        time: EvidenceQuestion::NotMeasured,
        memory: EvidenceQuestion::NotMeasured,
    }
}
