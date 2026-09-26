use campaign_core::{CampaignError, SelectionCountDecay, SelectionMode};

use super::{config, Session, EXPLOIT, MAX_FRONTIER, ROUNDS, SEED};
use crate::campaign_adapter::profile::ENTRIES_PER_BRANCH;
use crate::campaign_adapter::ranks::ORIGINAL_SCORE_DIMENSION;
use crate::campaign_adapter::{
    admit_config, retention_plan_request, stop_reason, AdapterError, AdapterIdentity,
    BootstrapFacts, DegenerateConfig, ProductStop, SelectionLimit, EXPLORATION_ORDER,
    EXPLORATION_THRESHOLD_PARTS_PER_MILLION, MAX_RANK_MAGNITUDE, RANK_DIMENSIONS,
    RETENTION_RANK_ORDER,
};
use crate::explorer::ExplorerConfig;

const RESTORABLE: BootstrapFacts = BootstrapFacts {
    restorable_root: true,
};

// r[verify chaoscontrol.campaign_policy.adapter.valid]
#[test]
fn explorer_config_maps_to_sequential_campaign_bounds() {
    let identity = AdapterIdentity::for_seed(SEED);
    let profile = admit_config(&identity, &config(), RESTORABLE).unwrap();
    let bounds = &profile.bounds;
    assert_eq!(bounds.validate(), Ok(()));
    assert_eq!(bounds.max_roots, 1);
    assert_eq!(bounds.max_selections, ROUNDS);
    assert_eq!(bounds.max_retained_candidates, MAX_FRONTIER as u64);
    assert_eq!(bounds.max_outstanding, 1);
    assert_eq!(bounds.max_depth, ROUNDS + 1);
    assert_eq!(bounds.max_rank_dimensions, 3);
    assert_eq!(bounds.max_rank_magnitude, MAX_RANK_MAGNITUDE);
    assert_eq!(bounds.max_rank_magnitude, i64::MAX as u64);
    let branch_entries = super::BRANCHES as u64 * ENTRIES_PER_BRANCH;
    assert_eq!(bounds.max_candidate_batch, branch_entries);
    assert_eq!(bounds.max_prune_batch, branch_entries);
    assert!(bounds.max_guidance_records >= bounds.max_candidates + bounds.max_selections);
    assert_eq!(profile.resource_budget.amount, ROUNDS);
    assert_eq!(profile.selection_limit, SelectionLimit::Rounds(ROUNDS));
    assert_eq!(profile.selection_profile.decay, SelectionCountDecay::None);
    assert_eq!(profile.selection_profile.stopping.finding_limit, None);
    assert_eq!(
        profile.selection_profile.mode,
        SelectionMode::Exploratory {
            threshold_parts_per_million: EXPLORATION_THRESHOLD_PARTS_PER_MILLION,
            entropy_source_id: identity.entropy_source_id,
            exploration_order: EXPLORATION_ORDER.to_vec(),
        }
    );
    assert_eq!(profile.selection_profile.validate(), Ok(()));
}

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
