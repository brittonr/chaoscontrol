//! Pure typed operation histories and bounded phenomenon classification.
//!
//! The core receives complete typed facts. It performs no file, clock,
//! process, network, or environment access.

mod checker;
mod model;

pub use checker::{
    check_history, validate_report_for_history, CheckOutcome, OperationBinding, PhenomenaReport,
    Phenomenon, Violation, CHECKER_ID, REPORT_SCHEMA_VERSION, REQUIRED_NON_CLAIMS,
};
pub use model::{
    bind_history, history_identity, operation_identity, validate_history, Dependency,
    DependencyKind, HistoryOperation, ObservationGap, OperationKind, OperationStatus,
    PhenomenaError, PhenomenaHistory, ReadObservation, HISTORY_SCHEMA_VERSION,
    MAX_HISTORY_DEPENDENCIES, MAX_HISTORY_GAPS, MAX_HISTORY_OPERATIONS,
};
