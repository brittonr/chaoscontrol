//! Pure product-side rank conversion for Campaign adoption.
//!
//! This module does not select candidates or authorize execution. Explorer
//! remains on its legacy policy until publication and parity checks pass.

use campaign_core::{derive_id, Id, IdKind, SelectionCountDecay};

/// Effective score, original score, then stable insertion order.
pub const RANK_DIMENSIONS: usize = 3;
const RANK_PROFILE_LABEL: &[u8] = b"chaoscontrol.frontier-ranks.ieee754.v1";
const SIGN_BIT: u64 = 1_u64 << (u64::BITS - 1);

/// Rejected product facts or an incompatible shared decay policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RankError {
    /// Scores must be finite and nonnegative.
    InvalidScore,
    /// The next selection count cannot fit the product counter.
    SelectionCountExhausted,
    /// The adapter already applies the legacy division-based decay.
    DoubleDecay,
    /// The admitted score did not fit the signed rank representation.
    RankOutOfRange,
}

/// Identifies the exact rank conversion and tie-break rules.
#[must_use]
pub fn rank_profile_id() -> Id {
    derive_id(IdKind::GuidanceProfile, RANK_PROFILE_LABEL, &[])
}

/// Converts legacy frontier ordering into lexicographic Campaign ranks.
///
/// Finite nonnegative IEEE-754 bit patterns preserve numeric order. Signed
/// zero is normalized because the legacy comparison treats both zeros alike.
/// Original score and insertion ID preserve the stable frontier tie break.
/// The shared profile must not apply a second, subtractive decay.
///
/// # Errors
/// Rejects invalid scores, exhausted counters, and nonzero shared decay.
pub fn frontier_ranks(
    score: f64,
    times_selected: u32,
    entry_id: u64,
    shared_decay: &SelectionCountDecay,
) -> Result<[i64; RANK_DIMENSIONS], RankError> {
    if !score.is_finite() || score < 0.0 {
        return Err(RankError::InvalidScore);
    }
    if shared_decay.penalty_per_selection != 0 {
        return Err(RankError::DoubleDecay);
    }
    let successor = times_selected
        .checked_add(1)
        .ok_or(RankError::SelectionCountExhausted)?;
    // Every u32 value has an exact f64 representation.
    let effective_score = score / f64::from(successor);
    let effective = score_rank(effective_score)?;
    let original = score_rank(score)?;
    let insertion_order = i64::from_be_bytes((!entry_id ^ SIGN_BIT).to_be_bytes());
    Ok([effective, original, insertion_order])
}

fn score_rank(score: f64) -> Result<i64, RankError> {
    let canonical_score = if score == 0.0 { 0.0 } else { score };
    i64::try_from(canonical_score.to_bits()).map_err(|_| RankError::RankOutOfRange)
}

#[cfg(test)]
mod tests;
