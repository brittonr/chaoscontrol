//! Pure first-bug-per-subtree survival analysis.
//!
//! Shells supply typed observations and model policy. This core performs no
//! file, clock, process, network, or environment access.

mod model;
mod statistics;

pub use model::{
    assemble_observations, observation_set_identity, AssembledObservation, BugInstance,
    FindabilityError, SubtreeObservation, MAX_BUG_INSTANCES_PER_SUBTREE, MAX_FINDABILITY_SUBTREES,
};
pub use statistics::{
    fit_findability, validate_report, ExponentialFit, FindabilityPolicy, FindabilityReport,
    FindabilityStatus, IndependenceAssessment, LomaxProjection, FINDABILITY_REPORT_SCHEMA_VERSION,
    REQUIRED_ASSUMPTIONS,
};
