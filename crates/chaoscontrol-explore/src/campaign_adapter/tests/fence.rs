use campaign_choregraph::{ControlPublication, DurablePublicationEvidence};
use campaign_core::{CampaignControlEvent, SelectionEvent};

use super::{config, Session, EXPLOIT};
use crate::campaign_adapter::{execution_authority, round_decision, RoundDecision};

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
