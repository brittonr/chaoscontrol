use campaign_core::{CampaignControlEvent, Id, IdKind, StopReason, WaitReason};

use super::{config, digest, Session, EXPLOIT};
use crate::campaign_adapter::{
    ineligible_retirement, product_stop, round_decision, stop_reason, AdapterError,
    IneligibleReason, ProductStop, RoundDecision,
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
