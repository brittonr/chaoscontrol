//! Pure deterministic replay/evidence core for ChaosControl.
//!
//! This crate is the single Rust-owned authority for replay verdict DTOs,
//! artifact hash DTOs, snapshot reference DTOs, replay class values, snapshot
//! validation status values, and the fail-closed validation decisions shared
//! by `chaoscontrol-explore` (emitter) and `chaoscontrol-evidence` (gates).
//!
//! Boundaries (see `.cairn/archive/2026-08-04-extract-replay-evidence-core/`):
//!
//! - Pure logic over in-memory DTOs only. No filesystem, VM, clock, process,
//!   KVM, environment, or receipt-writing effects live here.
//! - Public JSON field names are stable. Shell crates re-export these DTOs or
//!   adapt their stricter views onto them.
//! - Validation proves DTO syntax, artifact-reference consistency, replay
//!   class admissibility, and bounded anti-claim wording. It does not prove
//!   global deterministic hypervisor behavior or release readiness.

pub mod claims;
pub mod classify;
pub mod dto;
pub mod non_null_option;
pub mod validate;

pub use dto::{
    ArtifactHash, ReplayClass, ReplayCommandContext, ReplayParentSnapshotRef,
    ReplaySnapshotValidation, ReplayVerdict, SnapshotValidationStatus,
    LEGACY_REPLAY_VERDICT_SCHEMA_VERSION, NOT_REPRODUCED_EXIT_STATUS,
    REPLAY_VERDICT_SCHEMA_VERSION, REPRODUCED_EXIT_STATUS,
};
pub use validate::{
    ValidationError, CURRENT_SNAPSHOT_CODEC, CURRENT_SNAPSHOT_SCHEMA_VERSION, FILE_STORE_KIND,
    REPLAY_CLASSES, REQUIRED_REPLAY_CLASS, SNAPSHOT_STATUSES, SUPPORTED_SNAPSHOT_CODECS,
    SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS,
};
