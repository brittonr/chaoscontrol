use super::*;
use crate::session::digest_bytes;

fn history() -> RetainedHistory {
    RetainedHistory {
        source_ids: vec![digest_bytes(b"recording")],
        start_tick: 0,
        end_tick: 100,
        checkpoints: vec![
            MomentCheckpoint {
                id: 7,
                tick: 40,
                identity: None,
            },
            MomentCheckpoint {
                id: 8,
                tick: 60,
                identity: None,
            },
        ],
        branches: vec![
            SessionBranch {
                branch_id: digest_bytes(b"root"),
                parent: None,
                checkpoint_id: 7,
                tick: 40,
            },
            SessionBranch {
                branch_id: digest_bytes(b"child"),
                parent: Some(digest_bytes(b"root")),
                checkpoint_id: 8,
                tick: 60,
            },
        ],
    }
}
fn past_request() -> MomentRequest {
    MomentRequest {
        selector: MomentSelector::Tick(60),
        past_ticks: 20,
        future_ticks: 0,
        requires_restored_state: false,
    }
}

// r[verify chaoscontrol.debug_query.moment]
// r[verify chaoscontrol.debug_query.fixtures]
#[test]
fn historical_tick_checkpoint_and_branch_windows_are_offline_and_deterministic() {
    let history = history();
    let request = past_request();
    let first = admit_moment(&request, &history, None).unwrap();
    assert_eq!(first, admit_moment(&request, &history, None).unwrap());
    assert_eq!((first.start_tick, first.tick, first.end_tick), (40, 60, 60));
    assert_eq!(first.source_ids, history.source_ids);
    assert_eq!(first.session_id, None);
    assert_eq!(first.checkpoint_id, None);
    let checkpoint = MomentRequest {
        selector: MomentSelector::Checkpoint(8),
        ..request.clone()
    };
    assert_eq!(admit_moment(&checkpoint, &history, None).unwrap(), first);
    let branch_id = history.branches[1].branch_id.clone();
    let branch = MomentRequest {
        selector: MomentSelector::Branch(branch_id.clone()),
        ..request
    };
    let selected = admit_moment(&branch, &history, None).unwrap();
    assert_eq!(selected.branch_id, Some(branch_id));
    assert_eq!(selected.tick, first.tick);
}
#[test]
fn future_and_zero_width_restored_operations_require_an_exact_session() {
    let history = history();
    for request in [
        MomentRequest {
            future_ticks: 1,
            ..past_request()
        },
        MomentRequest {
            requires_restored_state: true,
            ..past_request()
        },
    ] {
        assert_eq!(
            admit_moment(&request, &history, None),
            Err(MomentError::MissingSession)
        );
    }
}
#[test]
fn declared_horizons_never_saturate_truncate_or_infer_missing_ticks() {
    let history = history();
    for request in [
        MomentRequest {
            past_ticks: 61,
            ..past_request()
        },
        MomentRequest {
            future_ticks: 41,
            ..past_request()
        },
        MomentRequest {
            selector: MomentSelector::Tick(101),
            ..past_request()
        },
        MomentRequest {
            selector: MomentSelector::Tick(u64::MAX),
            future_ticks: 1,
            ..past_request()
        },
    ] {
        assert_eq!(
            admit_moment(&request, &history, None),
            Err(MomentError::Horizon)
        );
    }
    let request = MomentRequest {
        past_ticks: MAX_MOMENT_TICKS + 1,
        ..past_request()
    };
    assert_eq!(
        admit_moment(&request, &history, None),
        Err(MomentError::Bound)
    );
}
#[test]
fn missing_checkpoint_branch_and_ambiguous_parentage_fail() {
    let original = history();
    let checkpoint = MomentRequest {
        selector: MomentSelector::Checkpoint(99),
        ..past_request()
    };
    assert_eq!(
        admit_moment(&checkpoint, &original, None),
        Err(MomentError::MissingCheckpoint)
    );
    let branch = MomentRequest {
        selector: MomentSelector::Branch(digest_bytes(b"unknown")),
        ..past_request()
    };
    assert_eq!(
        admit_moment(&branch, &original, None),
        Err(MomentError::UnknownBranch)
    );
    for parent in [
        digest_bytes(b"missing"),
        original.branches[1].branch_id.clone(),
    ] {
        let mut history = original.clone();
        history.branches[1].parent = Some(parent);
        assert_eq!(history.validate(), Err(MomentError::InvalidHistory));
    }
    let mut reversed = original.clone();
    reversed.branches[0].parent = Some(reversed.branches[1].branch_id.clone());
    assert_eq!(reversed.validate(), Err(MomentError::InvalidHistory));
    let mut wrong = original;
    wrong.branches[1].tick -= 1;
    assert_eq!(wrong.validate(), Err(MomentError::InvalidHistory));
}
#[test]
fn source_selection_and_horizon_drift_invalidate_bound_context() {
    let original = history();
    let request = past_request();
    let context = admit_moment(&request, &original, None).unwrap();
    let mut drift = original.clone();
    drift.source_ids = vec![digest_bytes(b"changed artifact")];
    assert_eq!(
        validate_moment(&context, &request, &drift, None),
        Err(MomentError::SessionMismatch)
    );
    let mut drift = original;
    drift.end_tick += 1;
    assert_eq!(
        validate_moment(&context, &request, &drift, None),
        Err(MomentError::SessionMismatch)
    );
    let changed = MomentRequest {
        past_ticks: request.past_ticks - 1,
        ..request
    };
    assert_eq!(
        validate_moment(&context, &changed, &history(), None),
        Err(MomentError::SessionMismatch)
    );
}
#[test]
fn receipt_context_rejects_malformed_missing_and_unpaired_identities() {
    let context = admit_moment(&past_request(), &history(), None).unwrap();
    let mut malformed = context.clone();
    malformed.source_ids[0] = "b3:not-a-digest".into();
    assert_eq!(malformed.validate(), Err(MomentError::InvalidIdentity));
    let mut duplicate = context.clone();
    duplicate.source_ids.push(duplicate.source_ids[0].clone());
    assert_eq!(duplicate.validate(), Err(MomentError::InvalidIdentity));
    let mut missing = context.clone();
    missing.end_tick += 1;
    assert_eq!(missing.validate(), Err(MomentError::MissingSession));
    let mut unpaired = context;
    unpaired.session_id = Some(digest_bytes(b"session"));
    assert_eq!(unpaired.validate(), Err(MomentError::MissingSession));
}
#[test]
fn retained_empty_events_still_have_an_explicit_source_and_horizon() {
    let retained = RetainedHistory {
        source_ids: vec![digest_bytes(b"empty recording")],
        start_tick: 0,
        end_tick: 0,
        checkpoints: Vec::new(),
        branches: Vec::new(),
    };
    let request = MomentRequest {
        selector: MomentSelector::Tick(0),
        past_ticks: 0,
        future_ticks: 0,
        requires_restored_state: false,
    };
    let context = admit_moment(&request, &retained, None).unwrap();
    assert_eq!((context.start_tick, context.end_tick), (0, 0));
    assert_eq!(context.source_ids, retained.source_ids);
    let mut missing = retained;
    missing.source_ids.clear();
    assert_eq!(
        admit_moment(&request, &missing, None),
        Err(MomentError::Bound)
    );
}
#[test]
fn typed_moment_request_roundtrips_without_ambient_defaults() {
    let encoded = serde_json::to_vec(&past_request()).unwrap();
    assert_eq!(
        serde_json::from_slice::<MomentRequest>(&encoded).unwrap(),
        past_request()
    );
    assert!(serde_json::from_str::<MomentRequest>(
        r#"{"selector":{"kind":"tick","value":60},"past_ticks":20}"#
    )
    .is_err());
}
