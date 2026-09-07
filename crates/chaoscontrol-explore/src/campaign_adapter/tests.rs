use super::{frontier_ranks, rank_profile_id, RankError};
use campaign_core::{IdKind, SelectionCountDecay};

const HIGH_SCORE: f64 = 20.0;
const LOW_SCORE: f64 = 15.0;
const REPEATED_SELECTIONS: u32 = 2;
const NO_SHARED_DECAY: SelectionCountDecay = SelectionCountDecay {
    penalty_per_selection: 0,
};

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
    let last = frontier_ranks(LOW_SCORE, 0, u64::MAX, &NO_SHARED_DECAY).unwrap();
    assert!(first > last);
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
}

#[test]
fn a_second_decay_rejects_without_a_rank() {
    let subtractive = SelectionCountDecay {
        penalty_per_selection: 1,
    };
    assert_eq!(
        frontier_ranks(LOW_SCORE, 0, 0, &subtractive),
        Err(RankError::DoubleDecay)
    );
}
