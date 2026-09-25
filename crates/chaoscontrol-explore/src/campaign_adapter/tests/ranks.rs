use crate::campaign_adapter::{frontier_ranks, rank_profile_id, RankError};
use campaign_core::{IdKind, SelectionCountDecay};

const HIGH_SCORE: f64 = 20.0;
const LOW_SCORE: f64 = 15.0;
const REPEATED_SELECTIONS: u32 = 2;
const MAX_ENTRY_SEQ: u64 = i64::MAX as u64;
const NO_SHARED_DECAY: SelectionCountDecay = SelectionCountDecay::None;

#[test]
fn repeated_selection_preserves_division_not_subtraction() {
    let selected = frontier_ranks(HIGH_SCORE, 1, 0, &NO_SHARED_DECAY).unwrap();
    let untouched = frontier_ranks(LOW_SCORE, 0, 1, &NO_SHARED_DECAY).unwrap();
    assert!(untouched > selected);
    let before_selection = frontier_ranks(HIGH_SCORE, 0, 0, &NO_SHARED_DECAY).unwrap();
    assert!(before_selection > untouched);
    assert!(rank_profile_id()
        .require_kind(IdKind::GuidanceProfile)
        .is_ok());
    assert!(rank_profile_id().require_kind(IdKind::Adapter).is_err());
}

#[test]
fn bounded_score_and_count_grid_matches_legacy_order() {
    let scores = [
        0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        LOW_SCORE,
        HIGH_SCORE,
        f64::MAX,
    ];
    let counts = [0, 1, REPEATED_SELECTIONS, u32::MAX - 1];
    for left in scores {
        for right in scores {
            for left_count in counts {
                for right_count in counts {
                    let left_rank = frontier_ranks(left, left_count, 0, &NO_SHARED_DECAY).unwrap();
                    let right_rank =
                        frontier_ranks(right, right_count, 1, &NO_SHARED_DECAY).unwrap();
                    let left_effective = left / (1.0 + f64::from(left_count));
                    let right_effective = right / (1.0 + f64::from(right_count));
                    let expected = left_effective
                        .total_cmp(&right_effective)
                        .then_with(|| left.total_cmp(&right))
                        .then(std::cmp::Ordering::Greater);
                    assert_eq!(left_rank.cmp(&right_rank), expected);
                }
            }
        }
    }
}

#[test]
fn ties_preserve_original_score_and_full_width_insertion_order() {
    let high_selected = frontier_ranks(HIGH_SCORE, 1, 1, &NO_SHARED_DECAY).unwrap();
    let low_untouched = frontier_ranks(
        HIGH_SCORE / f64::from(REPEATED_SELECTIONS),
        0,
        0,
        &NO_SHARED_DECAY,
    )
    .unwrap();
    assert!(high_selected > low_untouched);
    let first = frontier_ranks(LOW_SCORE, 0, 0, &NO_SHARED_DECAY).unwrap();
    let last = frontier_ranks(LOW_SCORE, 0, MAX_ENTRY_SEQ, &NO_SHARED_DECAY).unwrap();
    assert!(first > last);
    assert_eq!(last[2], 0);
    assert_eq!(
        frontier_ranks(-0.0, 0, 0, &NO_SHARED_DECAY),
        frontier_ranks(0.0, 0, 0, &NO_SHARED_DECAY)
    );
}

#[test]
fn invalid_scores_and_counter_exhaustion_reject() {
    for score in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(
            frontier_ranks(score, 0, 0, &NO_SHARED_DECAY),
            Err(RankError::InvalidScore)
        );
    }
    assert_eq!(
        frontier_ranks(LOW_SCORE, u32::MAX, 0, &NO_SHARED_DECAY),
        Err(RankError::SelectionCountExhausted)
    );
    assert_eq!(
        frontier_ranks(LOW_SCORE, 0, MAX_ENTRY_SEQ + 1, &NO_SHARED_DECAY),
        Err(RankError::SequenceOutOfRange)
    );
}

#[test]
fn a_second_decay_rejects_without_a_rank() {
    let subtractive = SelectionCountDecay::SubtractFromFirstRank {
        penalty_per_selection: 1,
    };
    assert_eq!(
        frontier_ranks(LOW_SCORE, 0, 0, &subtractive),
        Err(RankError::DoubleDecay)
    );
}

mod guidance {
    use super::super::{config, Session, EXPLOIT, SEED};
    use crate::campaign_adapter::{
        decay_revision, frontier_ranks, guidance_record, rank_profile_id, AdapterError,
        AdapterIdentity, AdapterReason, GuidanceFacts, RankError,
    };
    use campaign_core::{Id, IdKind, SelectionCountDecay};

    const SCORE: f64 = 20.0;
    const SEQ: u64 = 3;

    fn candidate() -> Id {
        Id {
            kind: IdKind::Candidate,
            digest: [9; 32],
        }
    }

    fn campaign() -> Id {
        Id {
            kind: IdKind::Campaign,
            digest: [8; 32],
        }
    }

    fn facts(times_selected: u32) -> GuidanceFacts {
        GuidanceFacts {
            score: SCORE,
            times_selected,
            entry_seq: SEQ,
        }
    }

    // r[verify chaoscontrol.campaign_policy.ranks.valid]
    #[test]
    fn guidance_binds_the_rank_profile_and_generation() {
        let identity = AdapterIdentity::for_seed(SEED);
        let record = guidance_record(&identity, campaign(), candidate(), facts(2)).unwrap();
        let ranks = frontier_ranks(SCORE, 2, SEQ, &SelectionCountDecay::None).unwrap();
        assert_eq!(record.ranks, ranks.to_vec());
        assert_eq!(record.generation, 2);
        assert_eq!(record.ordering_profile_id, rank_profile_id());
        assert_eq!(record.adapter_id, identity.adapter_id);
        assert_eq!(record.validate(campaign()), Ok(()));
    }

    // r[verify chaoscontrol.campaign_policy.ranks.invalid]
    #[test]
    fn unsafe_scores_and_crossed_identities_deny_guidance() {
        let identity = AdapterIdentity::for_seed(SEED);
        let nan = GuidanceFacts {
            score: f64::NAN,
            ..facts(0)
        };
        assert_eq!(
            guidance_record(&identity, campaign(), candidate(), nan),
            Err(AdapterError::Rank(RankError::InvalidScore))
        );
        let crossed = guidance_record(&identity, candidate(), candidate(), facts(0));
        assert_eq!(
            crossed,
            Err(AdapterError::Campaign(
                campaign_core::CampaignError::CrossedIdentity
            ))
        );
    }

    #[test]
    fn decay_revision_advances_exactly_one_selection() {
        let identity = AdapterIdentity::for_seed(SEED);
        let latest = guidance_record(&identity, campaign(), candidate(), facts(0)).unwrap();
        let revision = decay_revision(&identity, &latest, facts(1)).unwrap();
        assert_eq!(revision.successor.generation, 1);
        assert_eq!(revision.invalidated_guidance_id, latest.guidance_id);
        assert_eq!(
            revision.reason_id,
            identity.reason_id(AdapterReason::SelectionCountDecay)
        );
        for stale in [0, 2] {
            assert_eq!(
                decay_revision(&identity, &latest, facts(stale)),
                Err(AdapterError::StaleGuidanceGeneration)
            );
        }
        let mut foreign = latest;
        foreign.adapter_id = Id {
            kind: IdKind::Adapter,
            digest: [1; 32],
        };
        assert_eq!(
            decay_revision(&identity, &foreign, facts(1)),
            Err(AdapterError::CrossedIdentity)
        );
    }

    /// Legacy exploit: first maximum of `score / (1 + n)` in score-descending,
    /// insertion-ascending vector order, starting from zero.
    fn legacy_exploit(entries: &[(f64, u32)]) -> usize {
        let mut best = 0.0;
        let mut index = 0;
        for (position, (score, count)) in entries.iter().enumerate() {
            let effective = score / (1.0 + f64::from(*count));
            if effective > best {
                best = effective;
                index = position;
            }
        }
        index
    }

    // r[verify chaoscontrol.campaign_policy.ranks.valid]
    #[test]
    fn campaign_exploit_with_explicit_revisions_matches_legacy_division() {
        let scores = [20.0, 15.0, 15.0, 4.0];
        let mut session = Session::open(&config());
        let root = session.root.moment_id;
        let candidates = session.admit_entries(root, &scores);
        let mut counts = [0_u32; 4];
        for _ in 0..super::super::ROUNDS {
            let legacy = legacy_exploit(
                &scores
                    .iter()
                    .copied()
                    .zip(counts.iter().copied())
                    .collect::<Vec<_>>(),
            );
            let selection = session.executed(EXPLOIT);
            assert_eq!(selection.candidate_id, candidates[legacy]);
            session.settle(&selection, &super::super::no_children());
            counts[legacy] += 1;
        }
        assert_eq!(counts, [2, 1, 1, 0]);
    }
}
