//! Pure fault schedule selection authority used by the simulation kernel.
//!
//! `chaoscontrol-fault` owns the fault state machine. This module keeps the
//! simulation core as the dependency boundary and prevents the KVM shell from
//! defining a second selector.

pub use chaoscontrol_fault::engine::{EngineConfig, EngineSnapshot, FaultEngine};
pub use chaoscontrol_fault::schedule::{FaultSchedule, FaultScheduleSnapshot};
