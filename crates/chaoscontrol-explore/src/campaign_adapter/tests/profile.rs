use campaign_core::{CampaignError, SelectionMode};

use super::{config, Session, EXPLOIT, SEED};
use crate::campaign_adapter::ranks::ORIGINAL_SCORE_DIMENSION;
use crate::campaign_adapter::{
    admit_config, retention_plan_request, stop_reason, AdapterError, AdapterIdentity,
    BootstrapFacts, DegenerateConfig, ProductStop, SelectionLimit, EXPLORATION_ORDER,
    RANK_DIMENSIONS, RETENTION_RANK_ORDER,
};
use crate::explorer::ExplorerConfig;

const RESTORABLE: BootstrapFacts = BootstrapFacts {
    restorable_root: true,
};

// r[verify chaoscontrol.campaign_policy.adapter.invalid]
#[test]
fn degenerate_legacy_configurations_reject_before_exploration() {
    let identity = AdapterIdentity::for_seed(SEED);
    let zero_frontier = ExplorerConfig {
        max_frontier: 0,
        ..config()
    };
    assert_eq!(
        admit_config(&identity, &zero_frontier, RESTORABLE),
        Err(AdapterError::DegenerateConfig(
            DegenerateConfig::ZeroMaxFrontier
        ))
    );
    for weight in [f64::NAN, f64::INFINITY, -1.0] {
        let bad_weight = ExplorerConfig {
            rare_edge_weight: weight,
            ..config()
        };
        assert_eq!(
            admit_config(&identity, &bad_weight, RESTORABLE),
            Err(AdapterError::DegenerateConfig(
                DegenerateConfig::InvalidRareEdgeWeight
            ))
        );
    }
    assert_eq!(
        admit_config(
            &identity,
            &config(),
            BootstrapFacts {
                restorable_root: false
            }
        ),
        Err(AdapterError::DegenerateConfig(
            DegenerateConfig::MissingRestorableRoot
        ))
    );
    let unbounded = ExplorerConfig {
        max_rounds: u64::MAX,
        ..config()
    };
    assert_eq!(
        admit_config(&identity, &unbounded, RESTORABLE),
        Err(AdapterError::BoundOverflow)
    );
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn zero_rounds_complete_without_selection() {
    let zero_rounds = ExplorerConfig {
        max_rounds: 0,
        ..config()
    };
    let session = Session::open(&zero_rounds);
    assert_eq!(session.profile.selection_limit, SelectionLimit::NoSelection);
    assert_eq!(session.profile.bounds.max_selections, 1);
    let reason = stop_reason(&session.identity, ProductStop::CompletedWithoutSelection);
    assert!(campaign_core::admit_stop(&session.state, reason).is_ok());
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn retention_drops_the_newest_of_equal_lowest_scores_like_legacy_truncation() {
    let small = ExplorerConfig {
        max_frontier: 2,
        ..config()
    };
    let mut session = Session::open(&small);
    let root = session.root.moment_id;
    let request = retention_plan_request(&session.identity, &session.state);
    assert_eq!(request.rank_order, RETENTION_RANK_ORDER.to_vec());
    assert_eq!(
        request.retention_profile_id,
        session.identity.retention_profile_id
    );
    let candidates = session.admit_entries(root, &[5.0, 3.0, 3.0, 1.0]);
    let mut pruned = session.prune();
    pruned.sort_by_key(|candidate| session.seq_of(*candidate));
    assert_eq!(pruned, vec![candidates[2], candidates[3]]);
    assert_eq!(session.retained(), 2);
    assert!(session.prune().is_empty());
}

// r[verify chaoscontrol.campaign_policy.ranks.invalid]
#[test]
fn rank_orders_beyond_the_fixed_rank_shape_are_config_errors_for_every_draw() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[2.0, 1.0]);
    let session = session;
    let rank_dimensions = u32::try_from(RANK_DIMENSIONS).unwrap();
    // ChaosControl's declared orders stay inside the rank shape its guidance fixes.
    assert!(EXPLORATION_ORDER
        .iter()
        .chain(RETENTION_RANK_ORDER.iter())
        .all(|dimension| *dimension < rank_dimensions));
    // An order one past the shape is refused as configuration whatever the
    // draw, never as a draw-dependent guidance-shape error; the declared order
    // selects on the same draw.
    for draws in [EXPLOIT, super::explore(1)] {
        assert!(matches!(
            crate::campaign_adapter::round_decision(
                &session.identity,
                session.plan(draws),
                session.campaign_id()
            ),
            Ok(crate::campaign_adapter::RoundDecision::Execute(_))
        ));
        let mut request = session.request(draws);
        let SelectionMode::Exploratory {
            exploration_order, ..
        } = &mut request.profile.mode
        else {
            panic!("ChaosControl plans exploratory selections");
        };
        *exploration_order = vec![rank_dimensions];
        request.profile = request.profile.derive().unwrap();
        assert_eq!(
            campaign_core::plan_next(&session.state, request, &session.binding()),
            Err(CampaignError::InvalidConfig)
        );
    }
    let mut retention = retention_plan_request(&session.identity, &session.state);
    retention.rank_order = vec![ORIGINAL_SCORE_DIMENSION, rank_dimensions];
    assert_eq!(
        campaign_core::plan_pruning(&session.state, retention),
        Err(CampaignError::InvalidConfig)
    );
}
