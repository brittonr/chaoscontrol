use campaign_core::{CandidateStatus, Id, IdKind};

use super::{config, digest, Session, EXPLOIT};
use crate::campaign_adapter::{
    ineligible_retirement, unobserved_closure, AdapterError, AdapterReason, Eligibility,
    IneligibleReason, SnapshotContext, SnapshotEntry, SnapshotTable,
};

const HANDLE: u32 = 7;
const EPOCH: u64 = 2;

fn context() -> SnapshotContext {
    SnapshotContext {
        base_set_id: digest(4),
        epoch: EPOCH,
    }
}

fn entry(base_set: u8, epoch: u64) -> SnapshotEntry<u32> {
    SnapshotEntry {
        handle: HANDLE,
        base_set_id: digest(base_set),
        epoch,
    }
}

fn moment(byte: u8) -> Id {
    Id {
        kind: IdKind::ChildMoment,
        digest: digest(byte),
    }
}

// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.valid]
// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.invalid]
#[test]
fn only_current_retained_snapshots_are_executable() {
    let mut table = SnapshotTable::new();
    table.retain(moment(1), entry(4, EPOCH)).unwrap();
    table.retain(moment(2), entry(4, EPOCH - 1)).unwrap();
    table.retain(moment(3), entry(5, EPOCH)).unwrap();
    assert_eq!(table.len(), 3);
    assert_eq!(
        table.eligibility(moment(1), context()),
        Eligibility::Eligible
    );
    assert_eq!(
        table.eligibility(moment(2), context()),
        Eligibility::Ineligible(IneligibleReason::StaleEpoch)
    );
    assert_eq!(
        table.eligibility(moment(3), context()),
        Eligibility::Ineligible(IneligibleReason::StaleBaseSet)
    );
    assert_eq!(
        table.eligibility(moment(4), context()),
        Eligibility::Ineligible(IneligibleReason::NotRetained)
    );
    assert_eq!(
        table.release(moment(1)).map(|entry| entry.handle),
        Some(HANDLE)
    );
    assert_eq!(
        table.eligibility(moment(1), context()),
        Eligibility::Ineligible(IneligibleReason::NotRetained)
    );
}

#[test]
fn snapshot_keys_must_be_unique_moments() {
    let mut table = SnapshotTable::new();
    table.retain(moment(1), entry(4, EPOCH)).unwrap();
    assert_eq!(
        table.retain(moment(1), entry(4, EPOCH)),
        Err(AdapterError::InvalidSnapshotKey)
    );
    let candidate = Id {
        kind: IdKind::Candidate,
        digest: digest(1),
    };
    assert_eq!(
        table.retain(candidate, entry(4, EPOCH)),
        Err(AdapterError::InvalidSnapshotKey)
    );
    assert_eq!(table.len(), 1);
}

// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.valid]
// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.invalid]
#[test]
fn ineligible_candidates_are_retired_and_never_selected() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    let candidates = session.admit_entries(root, &[9.0, 1.0]);
    let record = session
        .state
        .candidate(candidates[0])
        .unwrap()
        .candidate
        .clone();
    let mut table = SnapshotTable::new();
    table
        .retain(session.root.moment_id, entry(4, EPOCH - 1))
        .unwrap();
    let Err(AdapterError::Ineligible(reason)) = table.require_executable(&record, context()) else {
        panic!("a stale-epoch parent must be ineligible");
    };
    assert_eq!(reason, IneligibleReason::StaleEpoch);
    let request = ineligible_retirement(&session.identity, &session.state, candidates[0], reason);
    let event = campaign_core::retire_candidate(&session.state, request).unwrap();
    session.publish(event);
    assert_eq!(
        session.state.candidate(candidates[0]).unwrap().status,
        CandidateStatus::Retired {
            reason_id: session
                .identity
                .reason_id(AdapterReason::SnapshotStaleEpoch)
        }
    );
    assert_eq!(session.executed(EXPLOIT).candidate_id, candidates[1]);
    let mut current = SnapshotTable::new();
    current
        .retain(session.root.moment_id, entry(4, EPOCH))
        .unwrap();
    assert_eq!(
        current
            .require_executable(&record, context())
            .map(|entry| entry.handle),
        Ok(HANDLE)
    );
}

// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.invalid]
#[test]
fn a_published_selection_without_a_snapshot_closes_unobserved() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[9.0]);
    let selection = session.executed(EXPLOIT);
    let closure = unobserved_closure(
        &session.identity,
        &selection,
        AdapterReason::SnapshotNotRetained,
    )
    .unwrap();
    assert!(closure.uncertainty_acknowledged);
    let event = campaign_core::close_unobserved(&session.state, closure).unwrap();
    session.publish(event);
    assert!(session.state.outstanding.is_empty());
    assert!(session.state.accepted_results.is_empty());
}
