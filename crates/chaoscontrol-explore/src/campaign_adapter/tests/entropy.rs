use campaign_core::{CampaignError, Id, IdKind};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use super::{config, explore, Session, EXPLOIT, SEED};
use crate::campaign_adapter::{
    entropy_ticket, legacy_explores, ticket_explores, AdapterError, AdapterIdentity, EntropyDraws,
    LegacyDraws, RoundDecision, EXPLORATION_THRESHOLD_PARTS_PER_MILLION,
};

/// `2^53`: rand 0.8 unit draws are `k * 2^-53`.
const UNIT_SCALE: f64 = 9_007_199_254_740_992.0;
/// Largest `k` with `k * 2^-53 < 0.1_f64`.
const LAST_EXPLORE_K: u64 = 900_719_925_474_099;
const MAX_K: u64 = (1 << 53) - 1;
const FRONTIER_LEN: usize = 5;
const DRAW_SAMPLES: usize = 20_000;
const OTHER_SEED: u64 = SEED + 1;

fn unit(k: u64) -> f64 {
    k as f64 / UNIT_SCALE
}

// r[verify chaoscontrol.campaign_policy.entropy.valid]
#[test]
fn k_boundaries_match_the_legacy_epsilon() {
    let cases = [
        (0, true, 0),
        (LAST_EXPLORE_K, true, 99_999),
        (LAST_EXPLORE_K + 1, false, 100_000),
        (MAX_K, false, 999_999),
    ];
    for (k, explores, explore_draw) in cases {
        let u = unit(k);
        assert_eq!(legacy_explores(u), explores);
        let index = explores.then_some(FRONTIER_LEN - 1);
        let draws =
            EntropyDraws::from_legacy(LegacyDraws { unit: u, index }, FRONTIER_LEN).unwrap();
        assert_eq!(draws.explore_draw, explore_draw);
        assert_eq!(
            draws.explore_draw < EXPLORATION_THRESHOLD_PARTS_PER_MILLION,
            explores
        );
        let expected_index = if explores { FRONTIER_LEN as u64 - 1 } else { 0 };
        assert_eq!(draws.index_draw, expected_index);
    }
}

// r[verify chaoscontrol.campaign_policy.entropy.valid]
#[test]
fn seeded_chacha_draws_convert_exactly_in_legacy_order() {
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let mut explored = 0;
    for _ in 0..DRAW_SAMPLES {
        let u: f64 = rng.gen();
        let index = legacy_explores(u).then(|| rng.gen_range(0..FRONTIER_LEN));
        let draws =
            EntropyDraws::from_legacy(LegacyDraws { unit: u, index }, FRONTIER_LEN).unwrap();
        let campaign_explores = draws.explore_draw < EXPLORATION_THRESHOLD_PARTS_PER_MILLION;
        assert_eq!(campaign_explores, index.is_some());
        explored += usize::from(campaign_explores);
    }
    assert!(explored > 0 && explored < DRAW_SAMPLES);
}

#[test]
fn ticket_bytes_carry_both_draws_little_endian() {
    let identity = AdapterIdentity::for_seed(SEED);
    let session = Session::open(&config());
    let draws = EntropyDraws {
        explore_draw: 0x0102_0304,
        index_draw: 0x0a0b,
    };
    let ticket = entropy_ticket(
        &identity,
        session.campaign_id(),
        session.state.state_id,
        session.profile.selection_profile.profile_id,
        1,
        draws,
    )
    .unwrap();
    assert_eq!(ticket.bytes[..8], 0x0102_0304_u64.to_le_bytes());
    assert_eq!(ticket.bytes[8..16], 0x0a0b_u64.to_le_bytes());
    assert!(ticket.bytes[16..].iter().all(|byte| *byte == 0));
    assert_eq!(ticket.explore_draw(), draws.explore_draw);
    assert_eq!(ticket.index_draw(), draws.index_draw);
    assert_eq!(ticket.source_id, identity.entropy_source_id);
}

#[test]
fn the_ticket_decides_campaigns_explore_branch() {
    let identity = AdapterIdentity::for_seed(SEED);
    let session = Session::open(&config());
    let mode = &session.profile.selection_profile.mode;
    let ticket = |explore_draw| {
        entropy_ticket(
            &identity,
            session.campaign_id(),
            session.state.state_id,
            session.profile.selection_profile.profile_id,
            1,
            EntropyDraws {
                explore_draw,
                index_draw: 0,
            },
        )
        .unwrap()
    };
    let below = ticket(EXPLORATION_THRESHOLD_PARTS_PER_MILLION - 1);
    let at = ticket(EXPLORATION_THRESHOLD_PARTS_PER_MILLION);
    assert_eq!(ticket_explores(mode, Some(&below)), Ok(true));
    assert_eq!(ticket_explores(mode, Some(&at)), Ok(false));
    assert_eq!(
        ticket_explores(mode, None),
        Err(AdapterError::MissingEntropyTicket)
    );
    assert_eq!(
        ticket_explores(&campaign_core::SelectionMode::Ranked, Some(&below)),
        Ok(false)
    );
}

// r[verify chaoscontrol.campaign_policy.entropy.invalid]
#[test]
fn malformed_recorded_draws_reject() {
    for bad in [0.1, 1.0, f64::NAN, -0.5, f64::INFINITY] {
        assert_eq!(
            EntropyDraws::from_legacy(
                LegacyDraws {
                    unit: bad,
                    index: None
                },
                FRONTIER_LEN
            ),
            Err(AdapterError::NonCanonicalUnitDraw)
        );
    }
    let exploring = unit(0);
    assert_eq!(
        EntropyDraws::from_legacy(
            LegacyDraws {
                unit: exploring,
                index: None
            },
            FRONTIER_LEN
        ),
        Err(AdapterError::MissingIndexDraw)
    );
    assert_eq!(
        EntropyDraws::from_legacy(
            LegacyDraws {
                unit: exploring,
                index: Some(FRONTIER_LEN)
            },
            FRONTIER_LEN
        ),
        Err(AdapterError::IndexDrawOutOfRange)
    );
    assert_eq!(
        EntropyDraws::from_legacy(
            LegacyDraws {
                unit: unit(MAX_K),
                index: Some(0)
            },
            FRONTIER_LEN
        ),
        Err(AdapterError::UnexpectedIndexDraw)
    );
}

// r[verify chaoscontrol.campaign_policy.entropy.valid]
// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn equal_draws_select_equally_and_explore_uses_legacy_vector_order() {
    let scores = [3.0, 9.0, 9.0, 1.0];
    // Legacy vector order: score descending, then insertion order.
    let legacy_order = [1, 2, 0, 3];
    for (position, legacy_index) in legacy_order.iter().enumerate() {
        let mut left = Session::open(&config());
        let mut right = Session::open(&config());
        let root = left.root.moment_id;
        let candidates = left.admit_entries(root, &scores);
        right.admit_entries(root, &scores);
        let chosen = left.executed(explore(position));
        assert_eq!(chosen, right.executed(explore(position)));
        assert_eq!(chosen.candidate_id, candidates[*legacy_index]);
    }
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    let candidates = session.admit_entries(root, &scores);
    assert_eq!(session.executed(EXPLOIT).candidate_id, candidates[1]);
}

// r[verify chaoscontrol.campaign_policy.entropy.invalid]
#[test]
fn missing_or_crossed_tickets_select_nothing() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[1.0, 2.0]);
    let binding = session.binding();
    let mut missing = session.request(EXPLOIT);
    missing.entropy = None;
    assert_eq!(
        campaign_core::plan_next(&session.state, missing, &binding),
        Err(CampaignError::InvalidEntropy)
    );
    let other_seed = AdapterIdentity::for_seed(OTHER_SEED);
    let mut crossed = session.request(EXPLOIT);
    let ticket = crossed.entropy.clone().unwrap();
    crossed.entropy = Some(
        entropy_ticket(
            &other_seed,
            ticket.campaign_id,
            ticket.state_id,
            ticket.profile_id,
            ticket.counter,
            EntropyDraws {
                explore_draw: ticket.explore_draw(),
                index_draw: ticket.index_draw(),
            },
        )
        .unwrap(),
    );
    assert_eq!(
        campaign_core::plan_next(&session.state, crossed, &binding),
        Err(CampaignError::InvalidEntropy)
    );
    let mut stale = session.request(EXPLOIT);
    let mut ticket = stale.entropy.clone().unwrap();
    ticket.state_id = Id {
        kind: IdKind::State,
        digest: [5; 32],
    };
    stale.entropy = Some(ticket.derive().unwrap());
    assert_eq!(
        campaign_core::plan_next(&session.state, stale, &binding),
        Err(CampaignError::InvalidEntropy)
    );
    assert!(matches!(session.select(EXPLOIT), RoundDecision::Execute(_)));
}
