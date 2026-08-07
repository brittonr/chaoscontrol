//! Pure replay classification kernel.
//!
//! The explorer shell supplies already-observed facts (identity validity,
//! parent depth, snapshot validation status, target assertion outcome). This
//! kernel maps those facts to a replay class with no side effects.

use crate::dto::{ReplayClass, SnapshotValidationStatus};

/// Classify a reproduce run from observed facts.
///
/// Mirrors the classification the explorer applies when it emits a verdict:
/// identity failures dominate, then snapshot validation failures, then the
/// snapshot-backed outcomes, with schedule-only runs classified as gaps.
pub fn classify_replay(
    identity_valid: bool,
    replay_parent_depth: u32,
    snapshot_status: SnapshotValidationStatus,
    target_failed: bool,
) -> ReplayClass {
    if !identity_valid {
        return ReplayClass::ReplayError;
    }
    match snapshot_status {
        SnapshotValidationStatus::MissingRef => ReplayClass::MissingSnapshotRef,
        SnapshotValidationStatus::MissingArtifact => ReplayClass::MissingSnapshotArtifact,
        SnapshotValidationStatus::InvalidDigest | SnapshotValidationStatus::InvalidRef => {
            ReplayClass::InvalidSnapshotDigest
        }
        SnapshotValidationStatus::Valid if replay_parent_depth == 0 => {
            ReplayClass::ScheduleOnlyReplayGap
        }
        SnapshotValidationStatus::Valid if target_failed => ReplayClass::SnapshotBackedReproduced,
        SnapshotValidationStatus::Valid => ReplayClass::SnapshotBackedNotReproduced,
        SnapshotValidationStatus::NotRequired => ReplayClass::ScheduleOnlyReplayGap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_failure_dominates_all_other_facts() {
        assert_eq!(
            classify_replay(false, 2, SnapshotValidationStatus::Valid, true),
            ReplayClass::ReplayError
        );
    }

    #[test]
    fn snapshot_failures_map_to_distinct_negative_classes() {
        assert_eq!(
            classify_replay(true, 2, SnapshotValidationStatus::MissingRef, true),
            ReplayClass::MissingSnapshotRef
        );
        assert_eq!(
            classify_replay(true, 2, SnapshotValidationStatus::MissingArtifact, true),
            ReplayClass::MissingSnapshotArtifact
        );
        assert_eq!(
            classify_replay(true, 2, SnapshotValidationStatus::InvalidDigest, true),
            ReplayClass::InvalidSnapshotDigest
        );
        assert_eq!(
            classify_replay(true, 2, SnapshotValidationStatus::InvalidRef, true),
            ReplayClass::InvalidSnapshotDigest
        );
    }

    #[test]
    fn valid_snapshot_classifies_by_depth_and_outcome() {
        assert_eq!(
            classify_replay(true, 0, SnapshotValidationStatus::Valid, true),
            ReplayClass::ScheduleOnlyReplayGap
        );
        assert_eq!(
            classify_replay(true, 1, SnapshotValidationStatus::Valid, true),
            ReplayClass::SnapshotBackedReproduced
        );
        assert_eq!(
            classify_replay(true, 1, SnapshotValidationStatus::Valid, false),
            ReplayClass::SnapshotBackedNotReproduced
        );
    }

    #[test]
    fn not_required_snapshot_is_always_a_schedule_only_gap() {
        assert_eq!(
            classify_replay(true, 0, SnapshotValidationStatus::NotRequired, true),
            ReplayClass::ScheduleOnlyReplayGap
        );
    }
}
