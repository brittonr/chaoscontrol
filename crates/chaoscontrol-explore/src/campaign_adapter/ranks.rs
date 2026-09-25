//! Versioned score-to-rank conversion, guidance records, and explicit decay revisions.
//!
//! Legacy selection divides a score by `1 + times_selected`. Campaign decay cannot
//! express division, so the admitted profile uses `SelectionCountDecay::None` and the
//! adapter revises the selected candidate's guidance after every settlement.

use campaign_core::{
    FrameWriter, GuidanceRecord, GuidanceRevisionRequest, Id, IdKind, SelectionCountDecay,
};

use super::identity::{AdapterIdentity, AdapterReason};
use super::AdapterError;

/// Effective score, original score, then stable insertion order.
pub const RANK_DIMENSIONS: usize = 3;
/// Largest absolute rank the adapter emits; every finite nonnegative score fits.
pub const MAX_RANK_MAGNITUDE: u64 = i64::MAX as u64;
/// Rank dimension of the decayed (effective) score.
pub const EFFECTIVE_SCORE_DIMENSION: u32 = 0;
/// Rank dimension of the undecayed score.
pub const ORIGINAL_SCORE_DIMENSION: u32 = 1;
/// Rank dimension of the stable insertion order.
pub const INSERTION_ORDER_DIMENSION: u32 = 2;
const RANK_PROFILE_LABEL: &[u8] = b"chaoscontrol.frontier-ranks.ieee754.v1";
const SOURCE_FACTS_LABEL: &[u8] = b"chaoscontrol.frontier-guidance-facts.v1";

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
    /// The entry sequence number exceeds the nonnegative insertion-rank domain.
    SequenceOutOfRange,
}

/// Identifies the exact rank conversion and tie-break rules.
#[must_use]
pub fn rank_profile_id() -> Id {
    campaign_core::derive_id(IdKind::GuidanceProfile, RANK_PROFILE_LABEL, &[])
}

/// Converts legacy frontier ordering into lexicographic Campaign ranks.
///
/// Finite nonnegative IEEE-754 bit patterns preserve numeric order. Signed
/// zero is normalized because the legacy comparison treats both zeros alike.
/// Original score and `i64::MAX - entry_seq` preserve the stable frontier tie break.
/// The shared profile must not apply a second, subtractive decay.
///
/// # Errors
/// Rejects invalid scores, exhausted counters, out-of-range sequence numbers,
/// and nonzero shared decay.
// r[impl chaoscontrol.campaign_policy.ranks]
pub fn frontier_ranks(
    score: f64,
    times_selected: u32,
    entry_seq: u64,
    shared_decay: &SelectionCountDecay,
) -> Result<[i64; RANK_DIMENSIONS], RankError> {
    if !score.is_finite() || score < 0.0 {
        return Err(RankError::InvalidScore);
    }
    if let SelectionCountDecay::SubtractFromFirstRank { .. } = shared_decay {
        return Err(RankError::DoubleDecay);
    }
    let successor = times_selected
        .checked_add(1)
        .ok_or(RankError::SelectionCountExhausted)?;
    // Every u32 value has an exact f64 representation.
    let effective_score = score / f64::from(successor);
    let effective = score_rank(effective_score)?;
    let original = score_rank(score)?;
    Ok([effective, original, insertion_rank(entry_seq)?])
}

/// Converts one finite nonnegative score to its order-preserving IEEE-754 rank.
///
/// # Errors
/// Rejects non-finite or negative scores.
pub fn score_rank(score: f64) -> Result<i64, RankError> {
    if !score.is_finite() || score < 0.0 {
        return Err(RankError::InvalidScore);
    }
    let canonical_score = if score == 0.0 { 0.0 } else { score };
    i64::try_from(canonical_score.to_bits()).map_err(|_| RankError::RankOutOfRange)
}

/// Strictly decreasing insertion rank: earlier entries rank higher.
///
/// # Errors
/// Rejects sequence numbers above `i64::MAX`.
pub fn insertion_rank(entry_seq: u64) -> Result<i64, RankError> {
    let seq = i64::try_from(entry_seq).map_err(|_| RankError::SequenceOutOfRange)?;
    Ok(i64::MAX - seq)
}

/// Product facts that one guidance generation ranks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuidanceFacts {
    /// Legacy frontier score.
    pub score: f64,
    /// Legacy selection count; also the guidance generation.
    pub times_selected: u32,
    /// Product frontier sequence number of the entry.
    pub entry_seq: u64,
}

impl GuidanceFacts {
    fn source_facts_id(self) -> Id {
        FrameWriter::new(IdKind::SourceFacts, SOURCE_FACTS_LABEL)
            .u64(self.score.to_bits())
            .u32(self.times_selected)
            .u64(self.entry_seq)
            .finish(IdKind::SourceFacts)
    }
}

/// Builds the guidance record of one admitted candidate at generation `times_selected`.
///
/// # Errors
/// Returns the typed rank error or a Campaign identity error.
pub fn guidance_record(
    identity: &AdapterIdentity,
    campaign_id: Id,
    candidate_id: Id,
    facts: GuidanceFacts,
) -> Result<GuidanceRecord, AdapterError> {
    let ranks = frontier_ranks(
        facts.score,
        facts.times_selected,
        facts.entry_seq,
        &SelectionCountDecay::None,
    )?;
    ranked_guidance(
        identity,
        campaign_id,
        candidate_id,
        u64::from(facts.times_selected),
        ranks.to_vec(),
        facts.source_facts_id(),
    )
}

/// Builds the explicit post-settlement decay revision for the selected candidate.
///
/// `facts.times_selected` is the count after the settled selection; it must be
/// exactly one more than the generation of `latest`.
///
/// # Errors
/// Returns `StaleGuidanceGeneration` when the count does not advance by one,
/// `CrossedIdentity` for another candidate or campaign, or the rank error.
pub fn decay_revision(
    identity: &AdapterIdentity,
    latest: &GuidanceRecord,
    facts: GuidanceFacts,
) -> Result<GuidanceRevisionRequest, AdapterError> {
    let expected = latest
        .generation
        .checked_add(1)
        .ok_or(AdapterError::Rank(RankError::SelectionCountExhausted))?;
    if u64::from(facts.times_selected) != expected {
        return Err(AdapterError::StaleGuidanceGeneration);
    }
    if latest.adapter_id != identity.adapter_id {
        return Err(AdapterError::CrossedIdentity);
    }
    let successor = guidance_record(identity, latest.campaign_id, latest.candidate_id, facts)?;
    Ok(GuidanceRevisionRequest {
        candidate_id: latest.candidate_id,
        successor,
        invalidated_guidance_id: latest.guidance_id,
        reason_id: identity.reason_id(AdapterReason::SelectionCountDecay),
    })
}

/// Derives one guidance record from already converted ranks.
pub(super) fn ranked_guidance(
    identity: &AdapterIdentity,
    campaign_id: Id,
    candidate_id: Id,
    generation: u64,
    ranks: Vec<i64>,
    source_facts_id: Id,
) -> Result<GuidanceRecord, AdapterError> {
    debug_assert!(ranks
        .iter()
        .all(|rank| rank.unsigned_abs() <= MAX_RANK_MAGNITUDE));
    Ok(GuidanceRecord {
        guidance_id: Id::absent(),
        campaign_id,
        candidate_id,
        generation,
        ranks,
        source_facts_id,
        ordering_profile_id: identity.guidance_profile_id,
        adapter_id: identity.adapter_id,
    }
    .derive()?)
}
