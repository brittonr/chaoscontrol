//! Legacy-versus-Campaign frontier parity.
//!
//! - The `drive` submodule runs one backend through the round protocol from a
//!   script VM-free, so the lib-test corpus feeds identical observations and
//!   randomness to both policies and compares them after every round; it fails
//!   with a [`DriveError`].
//! - The `report` submodule builds a [`ParityReport`] that states
//!   decision-field equality and semantic-work equality as separate facts,
//!   with time and memory not measured; any decision drift blocks cutover
//!   readiness.
//! - The `replay` submodule checks a recorded trace pair
//!   ([`check_trace_pair`]): each run's recorded observations and draws are
//!   replayed through the other policy's backend and written to a
//!   [`FrontierParityReceipt`] (`check-frontier-parity`).

mod drive;
mod replay;
mod report;
#[cfg(test)]
mod tests;

pub use super::trace::TraceParseError;
pub use drive::{DriveError, DriveFailure};
pub use replay::{
    check_trace_bytes, check_trace_pair, write_receipt, FrontierParityReceipt, PairConfig,
    ParityCheckError, RunParity, TraceInput, TraceRole, PARITY_NON_CLAIMS, PARITY_RECEIPT_SCHEMA,
};
pub use report::{
    DecisionField, EvidenceQuestion, FieldDrift, FieldQuestion, ParityDrift, ParityReport,
    PolicySide, SideRole, WorkFact, WorkQuantity, WorkQuestion, MAX_REPORTED_DRIFTS,
    PARITY_REPORT_SCHEMA,
};
