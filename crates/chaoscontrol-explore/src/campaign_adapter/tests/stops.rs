use campaign_choregraph::{
    control_capacity_reason_id, plan_next_from_published_projection, PublishedSelectionRequest,
};
use campaign_core::{
    CampaignControlEvent, Id, IdKind, NextDecision, SelectionEvent, StopReason, WaitReason,
};

use super::{config, digest, Session, EXPLOIT};
use crate::campaign_adapter::{
    ineligible_retirement, is_capacity_refusal, product_stop, round_decision, round_fits_capacity,
    stop_reason, AdapterError, IneligibleReason, ProductStop, RoundDecision,
};
use crate::explorer::ExplorerConfig;

const CONSUMER_STOPS: [ProductStop; 4] = [
    ProductStop::CoveragePlateau,
    ProductStop::ShortRunFinding,
    ProductStop::Interrupted,
    ProductStop::CompletedWithoutSelection,
];

// r[verify chaoscontrol.campaign_policy.stop.valid]
// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn product_stops_record_only_structural_classes() {
    let session = Session::open(&config());
    assert_eq!(
        stop_reason(&session.identity, ProductStop::FrontierExhausted),
        StopReason::FrontierExhausted
    );
    assert_eq!(
        stop_reason(&session.identity, ProductStop::Completed),
        StopReason::SelectionLimitReached
    );
    for stop in CONSUMER_STOPS {
        let reason = stop_reason(&session.identity, stop);
        assert!(matches!(reason, StopReason::ConsumerRequested { .. }));
        assert_eq!(product_stop(&reason), Ok(stop));
        let CampaignControlEvent::StopAccepted(event) =
            campaign_core::admit_stop(&session.state, reason).unwrap()
        else {
            panic!("a stop admission produces a stop event");
        };
        assert!(event.stop.incomplete_search_acknowledged);
    }
    let capacity = stop_reason(&session.identity, ProductStop::ControlCapacityExhausted);
    assert_eq!(
        capacity,
        StopReason::ConsumerRequested {
            reason_id: control_capacity_reason_id()
        }
    );
    assert_eq!(
        product_stop(&capacity),
        Ok(ProductStop::ControlCapacityExhausted)
    );
    let CampaignControlEvent::StopAccepted(event) =
        campaign_core::admit_stop(&session.state, capacity).unwrap()
    else {
        panic!("a stop admission produces a stop event");
    };
    assert!(event.stop.incomplete_search_acknowledged);
}

/// Plans through a cold projection of `session` under the host bound `maximum_events`.
fn plan_within(session: &Session, maximum_events: u32) -> campaign_core::DecisionPlan {
    plan_next_from_published_projection(
        &session.projection_within(maximum_events),
        PublishedSelectionRequest {
            request: session.request(EXPLOIT),
        },
    )
    .unwrap()
}

/// Admission, one candidate batch, and its guidance record.
const ONE_ENTRY_LINEAGE: u32 = 3;
/// A selection, its settlement, and the stop.
const SELECTION_RESERVE: u32 = 3;

// r[verify chaoscontrol.campaign_policy.stop.valid]
// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn the_host_capacity_stop_maps_to_its_own_product_class() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    assert_eq!(session.payloads.len(), ONE_ENTRY_LINEAGE as usize);
    let fits = ONE_ENTRY_LINEAGE + SELECTION_RESERVE;
    let tight = fits - 1;

    // One event short of a selection, its settlement, and the stop: Campaign
    // plans the adapter-owned consumer stop instead of a selection.
    let plan = plan_within(&session, tight);
    let NextDecision::Stop(stop) = &plan.decision else {
        panic!("expected the capacity stop, got {:?}", plan.decision);
    };
    assert!(stop.incomplete_search_acknowledged);
    assert_eq!(
        round_decision(&session.identity, plan.clone(), session.campaign_id()),
        Ok(RoundDecision::Stop(ProductStop::ControlCapacityExhausted))
    );
    // Publishing the selection core would plan is refused with the typed error.
    let request = session.request(EXPLOIT);
    let RoundDecision::Execute(selection) = round_decision(
        &session.identity,
        session.plan(EXPLOIT),
        session.campaign_id(),
    )
    .unwrap() else {
        panic!("core alone plans a selection");
    };
    let event = SelectionEvent::new(*selection, request.profile, request.entropy).unwrap();
    let refused = session
        .publication_within(
            CampaignControlEvent::SelectionPublished(event.clone()),
            tight,
        )
        .unwrap_err();
    assert_eq!(
        refused,
        campaign_choregraph::AdapterError::ProjectionCapacityExhausted
    );
    assert!(is_capacity_refusal(&AdapterError::Choregraph(refused)));
    // The capacity stop itself publishes within the bound and reloads cold.
    let stop = stop_reason(&session.identity, ProductStop::ControlCapacityExhausted);
    let mut stopped = Session::open(&config());
    stopped.admit_entries(stopped.root.moment_id, &[1.0]);
    let admitted = campaign_core::admit_stop(&stopped.state, stop).unwrap();
    assert!(stopped.publication_within(admitted.clone(), tight).is_ok());
    stopped.publish(admitted);
    assert!(stopped.projection_within(tight).state().stopped.is_some());

    // At the bound, the selection plans and publishes.
    assert!(matches!(
        round_decision(
            &session.identity,
            plan_within(&session, fits),
            session.campaign_id()
        ),
        Ok(RoundDecision::Execute(_))
    ));
    assert!(session
        .publication_within(CampaignControlEvent::SelectionPublished(event), fits)
        .is_ok());
}

// r[verify chaoscontrol.campaign_policy.stop.wait]
#[test]
fn an_outstanding_selection_without_capacity_waits_for_its_result() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[2.0, 1.0]);
    session.executed(EXPLOIT);
    let lineage = u32::try_from(session.payloads.len()).unwrap();
    // Another selection needs itself, two settlements, and the stop.
    let fits = lineage + SELECTION_RESERVE + 1;
    assert_eq!(
        round_decision(
            &session.identity,
            plan_within(&session, fits - 1),
            session.campaign_id()
        ),
        Ok(RoundDecision::Wait(WaitReason::AwaitingResults))
    );
    assert_eq!(
        round_decision(
            &session.identity,
            plan_within(&session, fits),
            session.campaign_id()
        ),
        Ok(RoundDecision::Wait(WaitReason::AwaitingCapacity))
    );
}

#[test]
fn a_round_fits_only_with_room_for_its_settlement_and_the_stop() {
    const LINEAGE: usize = 3;
    const ROUND_EVENTS: u64 = 10;
    const EXACT: u32 = 14;
    assert_eq!(
        round_fits_capacity(LINEAGE, ROUND_EVENTS, EXACT, 1),
        Ok(true)
    );
    assert_eq!(
        round_fits_capacity(LINEAGE, ROUND_EVENTS, EXACT - 1, 1),
        Ok(false)
    );
    let last_generation = u64::MAX - u64::from(EXACT);
    assert_eq!(
        round_fits_capacity(LINEAGE, ROUND_EVENTS, EXACT, last_generation),
        Ok(true)
    );
    assert_eq!(
        round_fits_capacity(LINEAGE, ROUND_EVENTS, EXACT, last_generation + 1),
        Ok(false)
    );
    assert_eq!(
        round_fits_capacity(LINEAGE, u64::MAX, u32::MAX, 1),
        Err(AdapterError::BoundOverflow)
    );
    assert!(!is_capacity_refusal(&AdapterError::Choregraph(
        campaign_choregraph::AdapterError::ProjectionBindingMismatch
    )));
    assert!(is_capacity_refusal(&AdapterError::Choregraph(
        campaign_choregraph::AdapterError::GenerationCapacityExhausted
    )));
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn planner_stops_map_to_completion_and_exhaustion() {
    let mut once = Session::open(&ExplorerConfig {
        max_rounds: 1,
        ..config()
    });
    let root = once.root.moment_id;
    once.admit_entries(root, &[1.0]);
    let selection = once.executed(EXPLOIT);
    once.settle(&selection, &super::no_children());
    assert_eq!(
        once.select(EXPLOIT),
        RoundDecision::Stop(ProductStop::Completed)
    );

    let mut exhausted = Session::open(&config());
    let root = exhausted.root.moment_id;
    let candidates = exhausted.admit_entries(root, &[1.0]);
    let retirement = ineligible_retirement(
        &exhausted.identity,
        &exhausted.state,
        candidates[0],
        IneligibleReason::NotRetained,
    );
    let event = campaign_core::retire_candidate(&exhausted.state, retirement).unwrap();
    exhausted.publish(event);
    assert_eq!(
        round_decision(
            &exhausted.identity,
            exhausted.plan(EXPLOIT),
            exhausted.campaign_id()
        ),
        Ok(RoundDecision::Stop(ProductStop::FrontierExhausted))
    );
}

// r[verify chaoscontrol.campaign_policy.stop.wait]
#[test]
fn outstanding_work_waits_instead_of_reporting_exhaustion() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    session.executed(EXPLOIT);
    let decision = round_decision(
        &session.identity,
        session.plan(super::explore(0)),
        session.campaign_id(),
    );
    assert_eq!(
        decision,
        Ok(RoundDecision::Wait(WaitReason::AwaitingCapacity))
    );
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn stops_without_product_meaning_are_typed_errors() {
    let session = Session::open(&config());
    let foreign = Id {
        kind: IdKind::Reason,
        digest: digest(60),
    };
    for reason in [
        StopReason::BudgetExhausted {
            dimensions: vec![session.identity.resource_dimension_id],
        },
        StopReason::PolicyRequested { reason_id: foreign },
        StopReason::ControlEventLimitReached,
        StopReason::ConsumerRequested { reason_id: foreign },
    ] {
        assert_eq!(product_stop(&reason), Err(AdapterError::UnmappedStop));
    }
    let mut session = session;
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    let plan = session.plan(EXPLOIT);
    let other = Id {
        kind: IdKind::Campaign,
        digest: digest(61),
    };
    assert_eq!(
        round_decision(&session.identity, plan, other),
        Err(AdapterError::CrossedIdentity)
    );
}
