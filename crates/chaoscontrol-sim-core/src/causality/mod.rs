//! Pure bounded interleaving minimization and probable-cause ranking.

mod attribution;
mod minimization;
mod model;

pub use attribution::{
    rank_candidates, AttributionObservation, AttributionRanking, AttributionReport,
};
pub use minimization::{DdminState, MinimizationCandidate, MinimizationResult};
pub use model::{
    candidate_set_identity, step_set_identity, validate_budget, AnalysisBudget, CausalityError,
    CauseCandidate, CauseClass, InterleavingStep, MAX_ATTRIBUTION_CANDIDATES,
    MAX_INTERLEAVING_STEPS,
};
