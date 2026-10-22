use campaign_choregraph::{
    plan_control_event_publication, ControlProjectionRequest, ControlPublicationRequest,
    EventClass, EventId, HistoryError, ParentChoices,
};
use campaign_core::{CampaignError, Id, IdKind};

use super::{config, digest, empty_graph, root_facts, Session, EXPLOIT, SEED};
use crate::campaign_adapter::moments::{
    moment_schema, EXPLORATION_PARENT_ROLE, MOMENT_KIND_METADATA_KEY,
};
use crate::campaign_adapter::{
    admit_child_moment, admit_root_moment, child_moment_draft, child_payload_id, readmit_moment,
    root_moment_draft, root_payload_id, AdapterError, AdapterIdentity, BranchSlot, RootMomentFacts,
};

// r[verify chaoscontrol.campaign_policy.history.valid]
#[test]
fn root_moment_is_one_parentless_application_event() {
    let identity = AdapterIdentity::for_seed(SEED);
    let draft = root_moment_draft(&identity, &root_facts());
    assert_eq!(draft.class, EventClass::Application);
    assert!(draft.parents.is_empty());
    assert_eq!(draft.schema_identity, moment_schema());
    assert!(draft.metadata.is_sorted());
    assert!(draft
        .metadata
        .iter()
        .any(|entry| entry.key == MOMENT_KIND_METADATA_KEY));
    let admitted = admit_root_moment(&empty_graph(), &identity, &root_facts()).unwrap();
    assert_eq!(admitted.moment_id.kind, IdKind::RootMoment);
    assert_eq!(admitted.moment_id.digest, admitted.event.digest().bytes());
    assert_eq!(admitted.event_payload_id, root_payload_id(&root_facts()));
    assert_eq!(admitted.graph.len(), 1);
    let other_base = RootMomentFacts {
        base_set_id: digest(99),
        ..root_facts()
    };
    assert_ne!(root_payload_id(&other_base), root_payload_id(&root_facts()));
}

// r[verify chaoscontrol.campaign_policy.history.valid]
#[test]
fn child_moment_links_to_its_parent_and_binds_the_selection() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    let selection = session.executed(EXPLOIT);
    let before = session.graph.len();
    let child = session.admit_child(&selection, 0);
    assert_eq!(child.graph.len(), before + 1);
    assert_eq!(child.moment_id.kind, IdKind::ChildMoment);
    let event = child.graph.event(child.event).unwrap().content().clone();
    assert_eq!(event.parents.len(), 1);
    assert_eq!(event.parents[0].role, EXPLORATION_PARENT_ROLE);
    assert_eq!(event.parents[0].event, session.root.event);
    let facts = session.child_facts(&selection, 0);
    assert_eq!(child.event_payload_id, child_payload_id(&facts).unwrap());
    let other_slot = crate::campaign_adapter::ChildMomentFacts {
        branch_slot: BranchSlot::Alternative(0),
        ..facts
    };
    assert_ne!(
        child_payload_id(&other_slot).unwrap(),
        child_payload_id(&facts).unwrap()
    );
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn stale_crossed_or_conflicting_moments_fail_without_mutation() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    let selection = session.executed(EXPLOIT);
    let facts = session.child_facts(&selection, 0);
    let graph = session.graph.clone();
    let identity = session.identity.clone();
    let campaign = session.campaign_id();
    let missing = EventId::new(campaign_choregraph::Digest::from_bytes(digest(77)));
    assert_eq!(
        admit_child_moment(&graph, &identity, campaign, &facts, missing),
        Err(AdapterError::History(HistoryError::MissingParent {
            event: missing
        }))
    );
    assert_eq!(
        admit_child_moment(&graph, &identity, campaign, &facts, session.branch.head),
        Err(AdapterError::NotMoment)
    );
    let other_campaign = Id {
        kind: IdKind::Campaign,
        digest: digest(55),
    };
    assert_eq!(
        admit_child_moment(
            &graph,
            &identity,
            other_campaign,
            &facts,
            session.root.event
        ),
        Err(AdapterError::CrossedCampaign)
    );
    let crossed = crate::campaign_adapter::ChildMomentFacts {
        selection_id: selection.candidate_id,
        ..facts
    };
    assert_eq!(
        admit_child_moment(&graph, &identity, campaign, &crossed, session.root.event),
        Err(AdapterError::Campaign(CampaignError::CrossedIdentity))
    );
    let child =
        admit_child_moment(&graph, &identity, campaign, &facts, session.root.event).unwrap();
    assert_eq!(
        admit_child_moment(
            &child.graph,
            &identity,
            campaign,
            &facts,
            session.root.event
        ),
        Err(AdapterError::History(HistoryError::NoChange))
    );
    let draft = child_moment_draft(&identity, &facts, session.root.event).unwrap();
    let conflicting = readmit_moment(&graph, session.branch.head, draft.clone());
    assert_eq!(
        conflicting,
        Err(AdapterError::History(HistoryError::ConflictingDuplicate {
            event: session.branch.head
        }))
    );
    let renamed = readmit_moment(&graph, missing, draft);
    assert!(matches!(
        renamed,
        Err(AdapterError::History(HistoryError::IdentityMismatch { .. }))
    ));
    assert_eq!(graph, session.graph);
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn a_binding_from_before_a_moment_admission_is_stale() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    let stale_binding = session.binding();
    let selection = session.executed(EXPLOIT);
    let stale_after_selection = session.binding();
    assert_ne!(stale_binding, stale_after_selection);
    let held = session.projection();
    session.admit_child(&selection, 0);
    let choices = ParentChoices::new();
    let observation = crate::campaign_adapter::result_observation(
        &session.identity,
        &session.state,
        &selection,
        &super::no_children(),
    )
    .unwrap();
    let event = campaign_core::admit_result(&session.state, observation).unwrap();
    let planned = plan_control_event_publication(ControlPublicationRequest {
        current: ControlProjectionRequest {
            graph: &session.graph,
            branch: &session.branch,
            parent_choices: &choices,
            maximum_events: session.graph.len() + 1,
            expected_binding: stale_after_selection,
            payloads: &session.payloads,
        },
        previous: &held,
        control_event: event.clone(),
    });
    assert!(planned.is_err());
    session.publish(event);
    assert_eq!(session.state.outstanding().len(), 0);
}
