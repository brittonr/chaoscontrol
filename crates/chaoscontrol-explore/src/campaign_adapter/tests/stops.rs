use campaign_choregraph::{
    control_capacity_reason_id, plan_next_from_published_projection, PublishedSelectionRequest,
};
use campaign_core::{
    CampaignControlEvent, Id, IdKind, NextDecision, SelectionEvent, StopReason, WaitReason,
};

use super::{config, digest, Session, EXPLOIT, HISTORY_MAX_EVENTS};
use crate::campaign_adapter::{
    ineligible_retirement, is_capacity_refusal, product_stop, round_decision, round_fits_capacity,
    stop_reason, AdapterError, HostCapacity, IneligibleReason, ProductStop, RoundDecision,
    RoundDemand,
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
    assert!(stopped.projection_within(tight).state().stopped().is_some());

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

/// The root moment plus the one-entry lineage.
const ONE_ENTRY_GRAPH: u32 = 1 + ONE_ENTRY_LINEAGE;

/// A session over a graph of `graph_max_events` events holding one candidate.
fn one_entry_within_graph(graph_max_events: u32) -> Session {
    let mut session = Session::open_within_graph(&config(), graph_max_events);
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0]);
    assert_eq!(session.graph.len(), ONE_ENTRY_GRAPH);
    session
}

/// Admits one foreign root moment per epoch of `epochs` into the session's
/// graph (another writer of the shared history).
fn admit_foreign_moments(session: &mut Session, epochs: std::ops::RangeInclusive<u64>) {
    for epoch in epochs {
        let facts = crate::campaign_adapter::RootMomentFacts {
            epoch,
            ..super::root_facts()
        };
        session.graph =
            crate::campaign_adapter::admit_root_moment(&session.graph, &session.identity, &facts)
                .unwrap()
                .graph;
    }
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn the_graph_event_limit_plans_and_publishes_the_capacity_stop() {
    let fits = ONE_ENTRY_GRAPH + SELECTION_RESERVE;
    let tight = fits - 1;
    // The projection bound stays wide: only the graph event limit binds.
    let session = one_entry_within_graph(tight);
    assert_eq!(
        round_decision(
            &session.identity,
            plan_within(&session, HISTORY_MAX_EVENTS),
            session.campaign_id()
        ),
        Ok(RoundDecision::Stop(ProductStop::ControlCapacityExhausted))
    );
    // Publishing the selection core alone would plan is refused with the typed
    // graph error, which is a host capacity refusal.
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
            HISTORY_MAX_EVENTS,
        )
        .unwrap_err();
    assert_eq!(
        refused,
        campaign_choregraph::AdapterError::GraphCapacityExhausted
    );
    assert!(is_capacity_refusal(&AdapterError::Choregraph(refused)));
    // The capacity stop publishes at the graph bound and reloads cold.
    let mut stopped = one_entry_within_graph(tight);
    let stop = campaign_core::admit_stop(
        &stopped.state,
        stop_reason(&stopped.identity, ProductStop::ControlCapacityExhausted),
    )
    .unwrap();
    stopped.publish(stop);
    assert_eq!(stopped.graph.len(), ONE_ENTRY_GRAPH + 1);
    let reloaded = stopped.projection();
    assert_eq!(
        product_stop(&reloaded.state().stopped().unwrap().reason),
        Ok(ProductStop::ControlCapacityExhausted)
    );

    // With room for the selection, its settlement, and the stop, it plans and
    // publishes.
    let roomy = one_entry_within_graph(fits);
    assert!(matches!(
        round_decision(
            &roomy.identity,
            plan_within(&roomy, HISTORY_MAX_EVENTS),
            roomy.campaign_id()
        ),
        Ok(RoundDecision::Execute(_))
    ));
    assert!(roomy
        .publication_within(
            CampaignControlEvent::SelectionPublished(event),
            HISTORY_MAX_EVENTS
        )
        .is_ok());
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn a_graph_without_room_for_the_stop_refuses_planning_with_the_typed_error() {
    let tight = ONE_ENTRY_GRAPH + SELECTION_RESERVE - 1;
    // Another writer takes one event: the stop still fits and is planned.
    let mut session = one_entry_within_graph(tight);
    admit_foreign_moments(&mut session, 1..=1);
    assert_eq!(
        round_decision(
            &session.identity,
            plan_within(&session, HISTORY_MAX_EVENTS),
            session.campaign_id()
        ),
        Ok(RoundDecision::Stop(ProductStop::ControlCapacityExhausted))
    );
    // It takes the stop's room too (the graph is full): the planner returns the
    // typed capacity error instead of a stop publication would refuse, and the
    // stop itself is refused.
    admit_foreign_moments(&mut session, 2..=2);
    assert_eq!(session.graph.len(), tight);
    let refused = plan_next_from_published_projection(
        &session.projection(),
        PublishedSelectionRequest {
            request: session.request(EXPLOIT),
        },
    )
    .unwrap_err();
    assert_eq!(
        refused,
        campaign_choregraph::AdapterError::GraphCapacityExhausted
    );
    assert!(is_capacity_refusal(&AdapterError::Choregraph(refused)));
    let stop = campaign_core::admit_stop(
        &session.state,
        stop_reason(&session.identity, ProductStop::ControlCapacityExhausted),
    )
    .unwrap();
    assert_eq!(
        session.publication_within(stop, HISTORY_MAX_EVENTS),
        Err(campaign_choregraph::AdapterError::GraphCapacityExhausted)
    );
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
fn a_round_fits_only_beside_the_exposed_reserve_and_within_the_graph_room() {
    const LINEAGE: u64 = 3;
    const RESERVED: u32 = 2;
    const ROUND: RoundDemand = RoundDemand {
        control_events: 10,
        history_events: 12,
    };
    // Lineage, the reserve (settlement and stop), and the round.
    const EXACT: u32 = 15;
    const EXACT_ROOM: u32 = 12;
    let capacity = |maximum_events, graph_room, admitted_generation| HostCapacity {
        lineage: LINEAGE,
        reserved_control_events: RESERVED,
        maximum_events,
        graph_room,
        admitted_generation,
    };
    assert_eq!(
        round_fits_capacity(&capacity(EXACT, EXACT_ROOM, 1), ROUND),
        Ok(true)
    );
    assert_eq!(
        round_fits_capacity(&capacity(EXACT - 1, EXACT_ROOM, 1), ROUND),
        Ok(false)
    );
    // Every graph append of the round, child moments included, must fit the room.
    assert_eq!(
        round_fits_capacity(&capacity(EXACT, EXACT_ROOM - 1, 1), ROUND),
        Ok(false)
    );
    let last_generation = u64::MAX - u64::from(EXACT);
    assert_eq!(
        round_fits_capacity(&capacity(EXACT, EXACT_ROOM, last_generation), ROUND),
        Ok(true)
    );
    assert_eq!(
        round_fits_capacity(&capacity(EXACT, EXACT_ROOM, last_generation + 1), ROUND),
        Ok(false)
    );
    let unbounded = RoundDemand {
        control_events: u64::MAX,
        history_events: u64::MAX,
    };
    assert_eq!(
        round_fits_capacity(&capacity(u32::MAX, u32::MAX, 1), unbounded),
        Err(AdapterError::BoundOverflow)
    );
    // A shared graph or a stale graph is a halting history error, never the
    // capacity stop: the Explorer cannot reach either (its genesis graph is
    // fresh per epoch and every seal presents its current graph).
    let root_event = Session::open(&config()).payloads[0].history_event_id;
    for halting in [
        campaign_choregraph::AdapterError::ProjectionBindingMismatch,
        campaign_choregraph::AdapterError::StaleGraph,
        campaign_choregraph::AdapterError::SharedControlGraph {
            event_id: root_event,
        },
    ] {
        assert!(!is_capacity_refusal(&AdapterError::Choregraph(halting)));
    }
    for refusal in [
        campaign_choregraph::AdapterError::ProjectionCapacityExhausted,
        campaign_choregraph::AdapterError::GenerationCapacityExhausted,
        campaign_choregraph::AdapterError::GraphCapacityExhausted,
    ] {
        assert!(is_capacity_refusal(&AdapterError::Choregraph(refusal)));
    }
}

/// Graph event limit of [`host_capacity_takes_campaigns_exposed_reserve`].
const RESERVE_GRAPH_MAX: u32 = 48;

/// Asserts `projection`'s host capacity is Campaign's exposed reserve and room,
/// and that a round fits exactly at that room and lineage and not one event past.
fn assert_capacity_at_the_bound(
    projection: &campaign_choregraph::ValidatedControlProjection,
    reserved_control_events: u32,
    reserved_graph_events: u32,
) -> HostCapacity {
    let capacity = HostCapacity::of(projection).unwrap();
    assert_eq!(
        projection.reserved_control_events(),
        reserved_control_events
    );
    assert_eq!(projection.reserved_graph_events(), reserved_graph_events);
    assert_eq!(capacity.reserved_control_events, reserved_control_events);
    assert_eq!(
        capacity.graph_room,
        RESERVE_GRAPH_MAX - projection.graph_events() - reserved_graph_events
    );
    let control_room =
        u64::from(capacity.maximum_events) - capacity.lineage - u64::from(reserved_control_events);
    let fits = RoundDemand {
        control_events: control_room,
        history_events: u64::from(capacity.graph_room),
    };
    assert_eq!(round_fits_capacity(&capacity, fits), Ok(true));
    let one_graph_event_more = RoundDemand {
        history_events: fits.history_events + 1,
        ..fits
    };
    assert_eq!(
        round_fits_capacity(&capacity, one_graph_event_more),
        Ok(false)
    );
    let one_control_event_more = RoundDemand {
        control_events: fits.control_events + 1,
        ..fits
    };
    assert_eq!(
        round_fits_capacity(&capacity, one_control_event_more),
        Ok(false)
    );
    capacity
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn host_capacity_takes_campaigns_exposed_reserve() {
    let mut session = Session::open_within_graph(&config(), RESERVE_GRAPH_MAX);
    let root = session.root.moment_id;
    session.admit_entries(root, &[2.0, 1.0]);
    // Open with nothing outstanding: the reserve is the stop.
    let idle = session.projection();
    let idle_capacity = assert_capacity_at_the_bound(&idle, 1, 1);
    assert_eq!(
        idle_capacity.lineage,
        u64::try_from(session.payloads.len()).unwrap()
    );

    // A selection admitted to the graph whose branch update has not committed is
    // a pending draft, counted as a selection: its own event, its settlement, and
    // the stop. The graph already holds the draft.
    let request = session.request(EXPLOIT);
    let plan =
        campaign_core::plan_next(&session.state, request.clone(), &session.binding()).unwrap();
    let RoundDecision::Execute(selection) =
        round_decision(&session.identity, plan, session.campaign_id()).unwrap()
    else {
        panic!("expected a selection");
    };
    let event = CampaignControlEvent::SelectionPublished(
        SelectionEvent::new(*selection, request.profile, request.entropy).unwrap(),
    );
    let committed_graph = session.graph.clone();
    session.graph = session
        .plan_publication(event.clone())
        .graph_after_admission()
        .clone();
    let pending = session.projection();
    assert_eq!(pending.graph_events(), idle.graph_events() + 1);
    let pending_capacity = assert_capacity_at_the_bound(&pending, 3, 2);
    assert_eq!(pending_capacity.lineage, idle_capacity.lineage);
    assert_eq!(pending_capacity.graph_room, idle_capacity.graph_room - 2);

    // Committed, the selection's settlement and the stop stay reserved: the room
    // is the one the pending draft already exposed.
    session.graph = committed_graph;
    session.publish(event);
    let outstanding = session.projection();
    let outstanding_capacity = assert_capacity_at_the_bound(&outstanding, 2, 2);
    assert_eq!(outstanding_capacity.lineage, idle_capacity.lineage + 1);
    assert_eq!(outstanding_capacity.graph_room, pending_capacity.graph_room);
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
