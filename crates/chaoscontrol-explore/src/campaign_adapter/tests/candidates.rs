use campaign_core::{CampaignError, Id, IdKind, UNIT_COST};

use super::{config, digest, Session, EXPLOIT};
use crate::campaign_adapter::{
    candidate_record, generation_profile_id, marker_digest, operation_profile_id,
    validate_candidate, AdapterError, FrontierEntryFacts,
};
use crate::explorer::{ExplorationMode, ExplorerConfig};
use crate::marker_branching::MarkerFrontierMetadata;

fn marker() -> MarkerFrontierMetadata {
    MarkerFrontierMetadata {
        marker_identity: "leader-elected".to_owned(),
        owner: "raft".to_owned(),
        state_ref: None,
        logical_position_ref: None,
        run_id: 1,
        observed_tick: 100,
    }
}

// r[verify chaoscontrol.campaign_policy.adapter.valid]
#[test]
fn one_frontier_entry_binds_every_candidate_identity() {
    let session = Session::open(&config());
    let facts = FrontierEntryFacts {
        marker_digest: Some(marker_digest(&marker())),
        ..session.entry_facts(session.root.moment_id, 0)
    };
    let record = candidate_record(
        &session.identity,
        &session.state,
        &facts,
        session.generation_profile_id,
    )
    .unwrap();
    assert_eq!(record.campaign_id, session.campaign_id());
    assert_eq!(record.parent_moment_id, session.root.moment_id);
    assert_eq!(record.depth, 1);
    assert_eq!(
        record.operation_schema_id,
        session.identity.operation_schema_id
    );
    assert_eq!(record.source_adapter_id, session.identity.adapter_id);
    assert_eq!(record.cost_profile_id, session.identity.cost_profile_id);
    assert_eq!(record.declared_costs.len(), 1);
    assert_eq!(record.declared_costs[0].amount, UNIT_COST);
    assert_eq!(
        record.operation_profile_id,
        operation_profile_id(facts.mode, 0, facts.marker_digest)
    );
    assert_eq!(
        record.operation_generation_profile_id,
        session.generation_profile_id
    );
    assert_eq!(
        validate_candidate(
            &session.identity,
            &session.state,
            &facts,
            session.generation_profile_id,
            &record
        ),
        Ok(())
    );
}

// r[verify chaoscontrol.campaign_policy.adapter.valid]
#[test]
fn reused_candidates_get_distinct_selection_identities() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    let candidates = session.admit_entries(root, &[5.0]);
    let first = session.executed(EXPLOIT);
    session.settle(&first, &super::no_children());
    let second = session.executed(EXPLOIT);
    assert_eq!(first.candidate_id, candidates[0]);
    assert_eq!(second.candidate_id, candidates[0]);
    assert_ne!(first.selection_id, second.selection_id);
    assert_ne!(first.operation_id, second.operation_id);
}

// r[verify chaoscontrol.campaign_policy.adapter.invalid]
#[test]
fn duplicate_marker_metadata_stays_distinct_but_duplicate_sequences_reject() {
    let session = Session::open(&config());
    let digest = Some(marker_digest(&marker()));
    let record = |seq| {
        candidate_record(
            &session.identity,
            &session.state,
            &FrontierEntryFacts {
                marker_digest: digest,
                ..session.entry_facts(session.root.moment_id, seq)
            },
            session.generation_profile_id,
        )
        .unwrap()
    };
    let (first, second) = (record(0), record(1));
    assert_ne!(first.candidate_id, second.candidate_id);
    let duplicate = campaign_core::admit_candidates(
        &session.state,
        vec![first.clone(), record(0)],
        session.profile.admission_rule,
        session.identity.coalescing_policy_id,
    );
    assert_eq!(duplicate, Err(CampaignError::DuplicateIdentity));
    assert!(campaign_core::admit_candidates(
        &session.state,
        vec![first, second],
        session.profile.admission_rule,
        session.identity.coalescing_policy_id,
    )
    .is_ok());
}

// r[verify chaoscontrol.campaign_policy.adapter.invalid]
#[test]
fn crossed_moment_mode_profile_or_campaign_fails_without_mutation() {
    let session = Session::open(&config());
    let before = session.state.clone();
    let facts = session.entry_facts(session.root.moment_id, 0);
    let unknown = FrontierEntryFacts {
        parent_moment_id: Id {
            kind: IdKind::ChildMoment,
            digest: digest(31),
        },
        ..facts
    };
    assert_eq!(
        candidate_record(
            &session.identity,
            &session.state,
            &unknown,
            session.generation_profile_id
        ),
        Err(AdapterError::UnknownParentMoment)
    );
    let record = candidate_record(
        &session.identity,
        &session.state,
        &facts,
        session.generation_profile_id,
    )
    .unwrap();
    let crossed_entries = [
        FrontierEntryFacts {
            mode: ExplorationMode::InputTree,
            ..facts
        },
        FrontierEntryFacts {
            entry_seq: 1,
            ..facts
        },
        FrontierEntryFacts {
            marker_digest: Some(marker_digest(&marker())),
            ..facts
        },
    ];
    for crossed in crossed_entries {
        assert_eq!(
            validate_candidate(
                &session.identity,
                &session.state,
                &crossed,
                session.generation_profile_id,
                &record
            ),
            Err(AdapterError::CandidateMismatch)
        );
    }
    let other_profile = generation_profile_id(&ExplorerConfig {
        havoc_after_stale: 3,
        ..config()
    })
    .unwrap();
    assert_eq!(
        validate_candidate(
            &session.identity,
            &session.state,
            &facts,
            other_profile,
            &record
        ),
        Err(AdapterError::CandidateMismatch)
    );
    let mut foreign = record;
    foreign.campaign_id = Id {
        kind: IdKind::Campaign,
        digest: digest(32),
    };
    assert_eq!(
        validate_candidate(
            &session.identity,
            &session.state,
            &facts,
            session.generation_profile_id,
            &foreign
        ),
        Err(AdapterError::CrossedCampaign)
    );
    assert_eq!(session.state, before);
}

#[test]
fn generation_profile_and_marker_digest_bind_every_declared_field() {
    let base = generation_profile_id(&config()).unwrap();
    let mut mutation = config().mutation;
    mutation.add_prob = 0.5;
    let variants = [
        ExplorerConfig {
            mutation,
            ..config()
        },
        ExplorerConfig {
            havoc_mutations: [1, 2],
            ..config()
        },
        ExplorerConfig {
            schedule_diversity: true,
            ..config()
        },
        ExplorerConfig {
            branch_factor: 9,
            ..config()
        },
        ExplorerConfig {
            ticks_per_branch: 7,
            ..config()
        },
    ];
    for variant in variants {
        assert_ne!(generation_profile_id(&variant).unwrap(), base);
    }
    assert_eq!(generation_profile_id(&config()).unwrap(), base);
    let with_empty_ref = MarkerFrontierMetadata {
        state_ref: Some(String::new()),
        ..marker()
    };
    assert_ne!(marker_digest(&with_empty_ref), marker_digest(&marker()));
    let moved = MarkerFrontierMetadata {
        state_ref: None,
        logical_position_ref: Some(String::new()),
        ..marker()
    };
    assert_ne!(marker_digest(&moved), marker_digest(&with_empty_ref));
}
