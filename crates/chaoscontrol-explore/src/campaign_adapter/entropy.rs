//! Explicit Campaign entropy tickets from the recorded legacy epsilon draws.
//!
//! The shell draws `u = rng.gen::<f64>()` exactly where the legacy frontier draws,
//! and `r = rng.gen_range(0..len)` only on the explore branch. Rand builds `u` as
//! `k * 2^-53` from the top 53 bits of one `next_u64`, so `k = u * 2^53` is exact and
//! `explore_draw = floor(k * 10^6 / 2^53)` is below `100_000` exactly when `u < 0.1`.
//! A replan reuses the round's recorded draws and never redraws.

use campaign_core::{
    EntropyTicket, FrontierState, Id, SelectionMode, DIGEST_LENGTH, ENTROPY_INDEX_DRAW_END,
    ENTROPY_PREFIX_BYTES, ENTROPY_SCALE,
};

use super::identity::AdapterIdentity;
use super::ranks::{INSERTION_ORDER_DIMENSION, ORIGINAL_SCORE_DIMENSION};
use super::AdapterError;

/// Legacy epsilon: explore when `u < 0.1`.
pub const LEGACY_EXPLORATION_EPSILON: f64 = 0.1;
/// Campaign threshold equivalent to the legacy epsilon.
pub const EXPLORATION_THRESHOLD_PARTS_PER_MILLION: u64 = 100_000;
/// Explore-branch order: original score, then insertion order (legacy vector order).
pub const EXPLORATION_ORDER: [u32; 2] = [ORIGINAL_SCORE_DIMENSION, INSERTION_ORDER_DIMENSION];
/// Mantissa bits of one `rand` 0.8 `f64` draw.
const UNIT_DRAW_BITS: u32 = 53;
/// `2^53` as an exact `f64`.
const UNIT_DRAW_SCALE: f64 = 9_007_199_254_740_992.0;

/// Draws the legacy frontier recorded for one selection, before any Campaign call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyDraws {
    /// `rng.gen::<f64>()` drawn at selection time.
    pub unit: f64,
    /// `rng.gen_range(0..len)`, present only on the explore branch.
    pub index: Option<usize>,
}

/// Ticket draws in Campaign's explicit entropy vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntropyDraws {
    /// Explore-branch draw in parts per million (ticket bytes `0..8`).
    pub explore_draw: u64,
    /// Explore-index draw (ticket bytes `8..16`); zero on the exploit branch.
    pub index_draw: u64,
}

/// Legacy explore predicate.
#[must_use]
pub fn legacy_explores(unit: f64) -> bool {
    unit < LEGACY_EXPLORATION_EPSILON
}

impl EntropyDraws {
    /// Converts one recorded legacy draw pair for a frontier of `frontier_len` entries.
    ///
    /// # Errors
    /// Rejects a non-canonical unit draw, an explore-predicate disagreement, and a
    /// missing, unexpected, or out-of-range index draw.
    // r[impl chaoscontrol.campaign_policy.entropy]
    pub fn from_legacy(draws: LegacyDraws, frontier_len: usize) -> Result<Self, AdapterError> {
        let k = unit_draw_bits(draws.unit)?;
        let scaled = u128::from(k) * u128::from(ENTROPY_SCALE);
        let explore_draw = u64::try_from(scaled >> UNIT_DRAW_BITS)
            .map_err(|_| AdapterError::NonCanonicalUnitDraw)?;
        let campaign_explores = explore_draw < EXPLORATION_THRESHOLD_PARTS_PER_MILLION;
        if campaign_explores != legacy_explores(draws.unit) {
            return Err(AdapterError::ExploreDisagreement);
        }
        let index_draw = match (campaign_explores, draws.index) {
            (true, None) => return Err(AdapterError::MissingIndexDraw),
            (false, Some(_)) => return Err(AdapterError::UnexpectedIndexDraw),
            (false, None) => 0,
            (true, Some(index)) if index >= frontier_len => {
                return Err(AdapterError::IndexDrawOutOfRange)
            }
            (true, Some(index)) => {
                u64::try_from(index).map_err(|_| AdapterError::IndexDrawOutOfRange)?
            }
        };
        debug_assert!(explore_draw < ENTROPY_SCALE);
        Ok(Self {
            explore_draw,
            index_draw,
        })
    }
}

/// Recovers the exact 53-bit integer behind one `rand` 0.8 unit draw.
fn unit_draw_bits(unit: f64) -> Result<u64, AdapterError> {
    if !(0.0..1.0).contains(&unit) {
        return Err(AdapterError::NonCanonicalUnitDraw);
    }
    let scaled = unit * UNIT_DRAW_SCALE;
    if scaled.fract() != 0.0 {
        return Err(AdapterError::NonCanonicalUnitDraw);
    }
    // `scaled` is an exact integer in `[0, 2^53)`, so the conversion is lossless.
    let k = scaled as u64;
    debug_assert!(k < 1_u64 << UNIT_DRAW_BITS);
    Ok(k)
}

/// Whether Campaign's exploratory selection takes the explore branch for one
/// request: the ticket's explore draw is below the mode's threshold. Ranked
/// selection never explores.
///
/// # Errors
/// Returns `MissingEntropyTicket` for an exploratory request without a ticket.
pub fn ticket_explores(
    mode: &SelectionMode,
    ticket: Option<&EntropyTicket>,
) -> Result<bool, AdapterError> {
    match mode {
        SelectionMode::Ranked => Ok(false),
        SelectionMode::Exploratory {
            threshold_parts_per_million,
            ..
        } => ticket
            .map(|ticket| ticket.explore_draw() < *threshold_parts_per_million)
            .ok_or(AdapterError::MissingEntropyTicket),
    }
}

/// Explicit exploratory selection mode bound to the seed's entropy source.
#[must_use]
pub fn selection_mode(identity: &AdapterIdentity) -> SelectionMode {
    SelectionMode::Exploratory {
        threshold_parts_per_million: EXPLORATION_THRESHOLD_PARTS_PER_MILLION,
        entropy_source_id: identity.entropy_source_id,
        exploration_order: EXPLORATION_ORDER.to_vec(),
    }
}

/// Ticket counter of the next selection: the next ordinal.
///
/// # Errors
/// Returns `ArithmeticOverflow` when the selection count is exhausted.
pub fn ticket_counter(state: &FrontierState) -> Result<u64, AdapterError> {
    state
        .selection_count()
        .checked_add(1)
        .ok_or(AdapterError::Campaign(
            campaign_core::CampaignError::ArithmeticOverflow,
        ))
}

/// Builds the explicit entropy ticket for one selection request.
///
/// Bytes `0..8` carry `explore_draw` and `8..16` carry `index_draw`, little endian;
/// the remaining bytes are zero.
///
/// # Errors
/// Returns a Campaign identity error for a crossed campaign, state, or profile.
pub fn entropy_ticket(
    identity: &AdapterIdentity,
    campaign_id: Id,
    state_id: Id,
    selection_profile_id: Id,
    counter: u64,
    draws: EntropyDraws,
) -> Result<EntropyTicket, AdapterError> {
    let mut bytes = [0_u8; DIGEST_LENGTH];
    bytes[..ENTROPY_PREFIX_BYTES].copy_from_slice(&draws.explore_draw.to_le_bytes());
    bytes[ENTROPY_PREFIX_BYTES..ENTROPY_INDEX_DRAW_END]
        .copy_from_slice(&draws.index_draw.to_le_bytes());
    let ticket = EntropyTicket {
        entropy_id: Id::absent(),
        campaign_id,
        state_id,
        profile_id: selection_profile_id,
        source_id: identity.entropy_source_id,
        counter,
        bytes,
    }
    .derive()?;
    debug_assert_eq!(ticket.explore_draw(), draws.explore_draw);
    debug_assert_eq!(ticket.index_draw(), draws.index_draw);
    Ok(ticket)
}
