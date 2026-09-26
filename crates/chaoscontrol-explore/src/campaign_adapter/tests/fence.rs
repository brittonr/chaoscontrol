use campaign_choregraph::{
    AdapterError, ControlProjectionRequest, ControlPublication, DurablePublicationEvidence,
    HistoryGraph, ParentChoices,
};
use campaign_core::{CampaignControlEvent, SelectionEvent};

use super::{config, root_facts, Session, EXPLOIT, HISTORY_MAX_EVENTS};
use crate::campaign_adapter::{
    admit_root_moment, advance_projection, execution_authority, load_projection,
    projection_binding, round_decision, RootMomentFacts, RoundDecision,
};

fn evidence(
    publication: &ControlPublication,
    durable_event: bool,
    durable_branch_update: bool,
) -> DurablePublicationEvidence {
    DurablePublicationEvidence {
        history_event_id: publication.history_event_id(),
        branch_before: publication.branch_plan().before.clone(),
        branch_after: publication.branch_plan().after.clone(),
        durable_event,
        durable_branch_update,
    }
}

// r[verify chaoscontrol.campaign_policy.publication_fence.valid]
// r[verify chaoscontrol.campaign_policy.publication_fence.invalid]
#[test]
fn only_the_durably_published_selection_may_execute() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[2.0, 1.0]);
    let request = session.request(EXPLOIT);
    let plan =
        campaign_core::plan_next(&session.state, request.clone(), &session.binding()).unwrap();
    let RoundDecision::Execute(selection) =
        round_decision(&session.identity, plan, session.campaign_id()).unwrap()
    else {
        panic!("the frontier has a selectable candidate");
    };
    assert!(!execution_authority(&selection, None, None));
    let event = CampaignControlEvent::SelectionPublished(
        SelectionEvent::new(*selection.clone(), request.profile, request.entropy).unwrap(),
    );
    let publication = session.plan_publication(event);
    assert!(!execution_authority(&selection, Some(&publication), None));
    for (durable_event, durable_branch_update) in [(false, false), (true, false), (false, true)] {
        let partial = evidence(&publication, durable_event, durable_branch_update);
        assert!(!execution_authority(
            &selection,
            Some(&publication),
            Some(&partial)
        ));
    }
    let committed = evidence(&publication, true, true);
    assert!(execution_authority(
        &selection,
        Some(&publication),
        Some(&committed)
    ));
    let mut other = *selection;
    other.ordinal += 1;
    assert!(!execution_authority(
        &other,
        Some(&publication),
        Some(&committed)
    ));
    let mut forged = committed;
    forged.branch_before = forged.branch_after.clone();
    let published = session.publish(publication.control_event().clone());
    assert!(!execution_authority(
        &selection_of(&published),
        Some(&published),
        Some(&forged)
    ));
}

fn selection_of(publication: &ControlPublication) -> campaign_core::SelectionDecision {
    let CampaignControlEvent::SelectionPublished(event) = publication.control_event() else {
        panic!("a selection publication carries a selection event");
    };
    event.decision.clone()
}

/// Projection request over `graph` with the session's branch and payloads.
fn request_over<'a>(
    session: &'a Session,
    graph: &'a HistoryGraph,
    choices: &'a ParentChoices,
) -> ControlProjectionRequest<'a> {
    ControlProjectionRequest {
        graph,
        branch: &session.branch,
        parent_choices: choices,
        maximum_events: HISTORY_MAX_EVENTS,
        expected_binding: projection_binding(&session.identity, graph, &session.branch).unwrap(),
        payloads: &session.payloads,
    }
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn advancing_onto_a_graph_older_than_the_held_projection_is_stale() {
    let session = Session::open(&config());
    let choices = ParentChoices::new();
    let older = session.graph.clone();
    let held = load_projection(&request_over(&session, &older, &choices)).unwrap();
    // Another writer's event grows the graph without moving the control branch.
    let foreign = admit_root_moment(
        &older,
        &session.identity,
        &RootMomentFacts {
            epoch: 1,
            ..root_facts()
        },
    )
    .unwrap();
    let current =
        advance_projection(&held, &request_over(&session, &foreign.graph, &choices)).unwrap();
    assert_eq!(current.graph_events(), held.graph_events() + 1);
    assert_eq!(
        advance_projection(&current, &request_over(&session, &older, &choices)).err(),
        Some(AdapterError::StaleGraph)
    );
    // A cold load of the older graph still validates; only a held projection
    // refuses to go back in graph time.
    assert!(load_projection(&request_over(&session, &older, &choices)).is_ok());
}
