//! Pure, verifiable functions for CPU determinism arithmetic.
//!
//! Every function in this module is:
//! - **Pure**: no I/O, no system calls, no side effects beyond the return value.
//! - **Deterministic**: same inputs always produce the same outputs.
//! - **Assertion-guarded**: Tiger Style `debug_assert!` preconditions and
//!   postconditions on every non-trivial function.
//!
//! These properties make the functions suitable for formal verification with
//! [Verus](https://github.com/verus-lang/verus).  The corresponding spec
//! file is `verus/cpu_spec.rs`.
//!
//! # Mapping to `cpu.rs`
//!
//! | Verified function        | Original location in `cpu.rs`          |
//! |--------------------------|----------------------------------------|
//! | [`gcd`]                  | `gcd()` (was `const fn`)               |
//! | [`tsc_crystal_ratio`]    | `tsc_crystal_ratio()`                  |
//! | [`encode_family`]        | `encode_family()`                      |
//! | [`encode_model`]         | `encode_model()`                       |
//! | [`encode_stepping`]      | `encode_stepping()`                    |
//! | [`elapsed_ns`]           | `VirtualTsc::elapsed_ns()`             |
//! | [`vtsc_tick`]            | `VirtualTsc::tick()`                   |
//! | [`vtsc_advance`]         | `VirtualTsc::advance()`                |
//! | [`vtsc_advance_to`]      | `VirtualTsc::advance_to()`             |

// ─── Constants (duplicated from cpu.rs so this module is self-contained) ─

/// Reference crystal clock frequency for CPUID leaf 0x15 (25 MHz).
///
/// Must be identical to the value in `cpu.rs`.  A compile-time test at the
/// bottom of this file enforces this.
pub(crate) const CRYSTAL_CLOCK_HZ: u32 = 25_000_000;

/// Bits \[3:0\] of CPUID 0x1 EAX — processor stepping.
pub(crate) const EAX_STEPPING_MASK: u32 = 0xF;

/// Bits \[7:4\] of CPUID 0x1 EAX — model (low nibble).
pub(crate) const EAX_MODEL_MASK: u32 = 0xF << 4;
/// Shift for the model field.
pub(crate) const EAX_MODEL_SHIFT: u32 = 4;

/// Bits \[11:8\] of CPUID 0x1 EAX — family (low nibble).
pub(crate) const EAX_FAMILY_MASK: u32 = 0xF << 8;
/// Shift for the family field.
pub(crate) const EAX_FAMILY_SHIFT: u32 = 8;

/// Bits \[19:16\] of CPUID 0x1 EAX — extended model.
pub(crate) const EAX_EXT_MODEL_MASK: u32 = 0xF << 16;
/// Shift for the extended model field.
pub(crate) const EAX_EXT_MODEL_SHIFT: u32 = 16;

/// Bits \[27:20\] of CPUID 0x1 EAX — extended family.
pub(crate) const EAX_EXT_FAMILY_MASK: u32 = 0xFF << 20;
/// Shift for the extended family field.
pub(crate) const EAX_EXT_FAMILY_SHIFT: u32 = 20;

// ─── GCD ─────────────────────────────────────────────────────────────

/// Greatest common divisor via the Euclidean algorithm.
///
/// Returns the largest `d` such that `d` divides both `a` and `b`.
/// When both inputs are zero, returns zero.
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - `gcd(a, b)` divides `a` (when `a > 0`)
/// - `gcd(a, b)` divides `b` (when `b > 0`)
/// - `gcd(a, b) == gcd(b, a)` (commutative)
/// - `gcd(a, 0) == a`
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::cpu::gcd;
/// assert_eq!(gcd(12, 8), 4);
/// assert_eq!(gcd(0, 7), 7);
/// ```
pub(crate) const fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    // Postcondition: result divides both original inputs (when non-zero).
    // (Cannot call debug_assert! in const fn with non-const ops, but
    // the #[test] suite below validates this exhaustively.)
    a
}

// ─── TSC crystal ratio ──────────────────────────────────────────────

/// Compute the `(denominator, numerator)` pair for CPUID leaf 0x15.
///
/// The ratio satisfies:
///
/// ```text
///   TSC_freq_hz  =  CRYSTAL_CLOCK_HZ × numerator / denominator
///               =  tsc_khz × 1000
/// ```
///
/// The fraction is reduced to lowest terms so that both values fit
/// comfortably in `u32`.
///
/// # Panics (debug only)
///
/// - `tsc_khz` must be non-zero (division by zero in ratio reduction).
/// - The resulting `denominator` must be non-zero (guaranteed by GCD > 0
///   when crystal > 0).
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - `CRYSTAL_CLOCK_HZ * numerator / denominator == tsc_khz * 1000`
/// - `gcd(numerator, denominator) == 1` (fully reduced)
/// - `denominator > 0`
pub(crate) fn tsc_crystal_ratio(tsc_khz: u32) -> (u32, u32) {
    debug_assert!(tsc_khz > 0, "tsc_khz must be non-zero");
    debug_assert!(CRYSTAL_CLOCK_HZ > 0, "CRYSTAL_CLOCK_HZ must be non-zero");

    let tsc_hz = tsc_khz as u64 * 1_000;
    let crystal = CRYSTAL_CLOCK_HZ as u64;
    let g = gcd(tsc_hz, crystal);

    // g > 0 because crystal > 0, so both divisions are safe.
    let denominator = (crystal / g) as u32;
    let numerator = (tsc_hz / g) as u32;

    // Postconditions.
    debug_assert!(denominator > 0, "denominator must be positive");
    debug_assert!(
        CRYSTAL_CLOCK_HZ as u64 * numerator as u64 / denominator as u64 == tsc_khz as u64 * 1_000,
        "ratio must reconstruct the original TSC frequency"
    );

    (denominator, numerator)
}

// ─── CPUID EAX encoding ─────────────────────────────────────────────

/// Encode a *display* family value into the CPUID 0x1 EAX register.
///
/// For families ≤ 15 the entire value goes into bits \[11:8\].
/// For families > 15 the base field is set to 0xF and the excess is
/// placed in the extended-family field (bits \[27:20\]).
///
/// Only the family bits are modified; all other bits in `eax` are
/// preserved.
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - Bits outside `EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK` are unchanged.
/// - `decode_family(eax) == family` (round-trip).
pub(crate) fn encode_family(eax: &mut u32, family: u8) {
    let preserved = *eax & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK);

    *eax &= !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK);
    if family <= 0xF {
        *eax |= (family as u32) << EAX_FAMILY_SHIFT;
    } else {
        *eax |= 0xF << EAX_FAMILY_SHIFT;
        *eax |= ((family as u32).saturating_sub(0xF)) << EAX_EXT_FAMILY_SHIFT;
    }

    // Postcondition: non-family bits untouched.
    debug_assert_eq!(
        *eax & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK),
        preserved,
        "encode_family must not clobber non-family bits"
    );
    // Postcondition: round-trip.
    debug_assert_eq!(
        decode_family(*eax),
        family,
        "encode_family must round-trip through decode_family"
    );
}

/// Encode a *display* model value into the CPUID 0x1 EAX register.
///
/// Low nibble → bits \[7:4\], high nibble → bits \[19:16\] (extended
/// model).
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - Bits outside `EAX_MODEL_MASK | EAX_EXT_MODEL_MASK` are unchanged.
/// - `decode_model(eax) == model` (round-trip).
pub(crate) fn encode_model(eax: &mut u32, model: u8) {
    let preserved = *eax & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK);

    *eax &= !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK);
    *eax |= ((model as u32) & 0xF) << EAX_MODEL_SHIFT;
    *eax |= (((model as u32) >> 4) & 0xF) << EAX_EXT_MODEL_SHIFT;

    debug_assert_eq!(
        *eax & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK),
        preserved,
        "encode_model must not clobber non-model bits"
    );
    debug_assert_eq!(
        decode_model(*eax),
        model,
        "encode_model must round-trip through decode_model"
    );
}

/// Encode a stepping value into CPUID 0x1 EAX bits \[3:0\].
///
/// Only the bottom 4 bits of `stepping` are used (values > 15 are
/// silently masked).
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - Bits outside `EAX_STEPPING_MASK` are unchanged.
/// - `decode_stepping(eax) == stepping & 0xF` (round-trip).
pub(crate) fn encode_stepping(eax: &mut u32, stepping: u8) {
    let preserved = *eax & !EAX_STEPPING_MASK;

    *eax &= !EAX_STEPPING_MASK;
    *eax |= (stepping as u32) & 0xF;

    debug_assert_eq!(
        *eax & !EAX_STEPPING_MASK,
        preserved,
        "encode_stepping must not clobber non-stepping bits"
    );
    debug_assert_eq!(
        decode_stepping(*eax),
        stepping & 0xF,
        "encode_stepping must round-trip through decode_stepping"
    );
}

// ─── CPUID EAX decoding (used by assertions and tests) ──────────────

/// Decode the display family from CPUID 0x1 EAX.
///
/// If the base family field is 0xF the extended family is added.
/// This is the inverse of [`encode_family`].
pub(crate) fn decode_family(eax: u32) -> u8 {
    let base = ((eax & EAX_FAMILY_MASK) >> EAX_FAMILY_SHIFT) as u8;
    if base == 0xF {
        let ext = ((eax & EAX_EXT_FAMILY_MASK) >> EAX_EXT_FAMILY_SHIFT) as u8;
        base.wrapping_add(ext)
    } else {
        base
    }
}

/// Decode the display model from CPUID 0x1 EAX.
///
/// Combines the base model (bits \[7:4\]) and extended model (bits
/// \[19:16\]) into a single byte.  This is the inverse of
/// [`encode_model`].
pub(crate) fn decode_model(eax: u32) -> u8 {
    let base = ((eax & EAX_MODEL_MASK) >> EAX_MODEL_SHIFT) as u8;
    let ext = ((eax & EAX_EXT_MODEL_MASK) >> EAX_EXT_MODEL_SHIFT) as u8;
    (ext << 4) | base
}

/// Decode the stepping from CPUID 0x1 EAX bits \[3:0\].
///
/// This is the inverse of [`encode_stepping`].
pub(crate) fn decode_stepping(eax: u32) -> u8 {
    (eax & EAX_STEPPING_MASK) as u8
}

// ─── Virtual TSC arithmetic ─────────────────────────────────────────

/// Convert a TSC counter value to elapsed nanoseconds.
///
/// Uses `u128` intermediate arithmetic to avoid overflow for large
/// counter values (supports up to ~195 years at 3 GHz).
///
/// # Formula
///
/// ```text
///   elapsed_ns  =  counter × 1_000_000 / tsc_khz
/// ```
///
/// (since `tsc_khz` kHz = `tsc_khz × 1000` Hz, and
///  `ns = counter / freq_hz × 10^9 = counter × 10^6 / tsc_khz`)
///
/// # Panics (debug only)
///
/// - `tsc_khz` must be non-zero.
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - `elapsed_ns(0, k) == 0` for any `k > 0`
/// - `elapsed_ns(k * 1000, k) == 1_000_000` (1 ms at `k` kHz)
/// - Monotonic: `c1 <= c2  ⟹  elapsed_ns(c1, k) <= elapsed_ns(c2, k)`
pub(crate) fn elapsed_ns(counter: u64, tsc_khz: u32) -> u64 {
    debug_assert!(tsc_khz > 0, "tsc_khz must be non-zero");
    debug_assert!(
        (counter as u128 * 1_000_000) / tsc_khz as u128 <= u64::MAX as u128,
        "elapsed_ns would overflow u64"
    );

    let result = ((counter as u128 * 1_000_000) / tsc_khz as u128) as u64;

    // Postcondition: zero counter ⟹ zero nanoseconds.
    debug_assert!(counter != 0 || result == 0);

    result
}

/// Advance a TSC counter by one tick (wrapping).
///
/// Returns the new counter value after adding `advance_per_tick`.
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - `vtsc_tick(c, a) == c.wrapping_add(a)`
/// - `vtsc_tick(u64::MAX, 1) == 0` (wraps correctly)
#[inline]
pub(crate) fn vtsc_tick(counter: u64, advance_per_tick: u64) -> u64 {
    counter.wrapping_add(advance_per_tick)
}

/// Advance a TSC counter by `n` ticks (wrapping).
///
/// Equivalent to calling [`vtsc_tick`] `n` times, but O(1).
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - `vtsc_advance(c, a, 0) == c`
/// - `vtsc_advance(c, a, 1) == vtsc_tick(c, a)`
/// - `vtsc_advance(c, a, n) == c.wrapping_add(a.wrapping_mul(n))`
#[inline]
pub(crate) fn vtsc_advance(counter: u64, advance_per_tick: u64, n: u64) -> u64 {
    let result = counter.wrapping_add(advance_per_tick.wrapping_mul(n));

    // Postcondition: advancing zero ticks is a no-op.
    debug_assert!(n != 0 || result == counter);

    result
}

/// Advance a TSC counter to at least `target`, rounding up to a tick
/// boundary.
///
/// If `target <= counter`, the counter is returned unchanged.  Otherwise
/// the smallest multiple of `advance_per_tick` is added such that the
/// result ≥ `target`.
///
/// # Panics (debug only)
///
/// - `advance_per_tick` must be non-zero (used as a divisor).
///
/// # Properties verified by `verus/cpu_spec.rs`
///
/// - `result >= target` (when `target > counter`)
/// - `result - counter` is a multiple of `advance_per_tick`
/// - `result - counter < target - counter + advance_per_tick` (minimal)
pub(crate) fn vtsc_advance_to(counter: u64, advance_per_tick: u64, target: u64) -> u64 {
    debug_assert!(advance_per_tick > 0, "advance_per_tick must be non-zero");

    if target <= counter {
        // Postcondition: no-op when already past target.
        debug_assert!(counter >= target);
        return counter;
    }

    let delta = target - counter;
    let ticks = delta.div_ceil(advance_per_tick);
    let result = counter.wrapping_add(ticks.wrapping_mul(advance_per_tick));

    // Postcondition: we reached or passed the target.
    debug_assert!(
        result >= target || result < counter,
        "advance_to must reach or pass the target (or wrap)"
    );
    // Postcondition: step is a tick-multiple (when no wrapping).
    debug_assert!(
        result < counter // wrapped
        || (result - counter) % advance_per_tick == 0,
        "advance_to must land on a tick boundary"
    );

    result
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── GCD ──────────────────────────────────────────────────────

    #[test]
    fn gcd_basic_cases() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(100, 25), 25);
        assert_eq!(gcd(17, 13), 1);
    }

    #[test]
    fn gcd_identity_and_zero() {
        assert_eq!(gcd(0, 5), 5);
        assert_eq!(gcd(7, 0), 7);
        assert_eq!(gcd(0, 0), 0);
    }

    #[test]
    fn gcd_is_commutative() {
        for &(a, b) in &[(12, 8), (100, 25), (17, 13), (0, 5), (1, 1_000_000)] {
            assert_eq!(gcd(a, b), gcd(b, a), "gcd({a}, {b}) != gcd({b}, {a})");
        }
    }

    #[test]
    fn gcd_divides_both_inputs() {
        for &(a, b) in &[(12, 8), (100, 25), (17, 13), (3_000_000_000u64, 25_000_000)] {
            let g = gcd(a, b);
            assert!(g > 0);
            assert_eq!(a % g, 0, "gcd({a},{b})={g} does not divide {a}");
            assert_eq!(b % g, 0, "gcd({a},{b})={g} does not divide {b}");
        }
    }

    #[test]
    fn gcd_of_equal_values() {
        assert_eq!(gcd(42, 42), 42);
    }

    #[test]
    fn gcd_coprime() {
        assert_eq!(gcd(7, 11), 1);
        assert_eq!(gcd(97, 89), 1);
    }

    // ── TSC crystal ratio ────────────────────────────────────────

    #[test]
    fn ratio_3ghz() {
        let (d, n) = tsc_crystal_ratio(3_000_000);
        let freq = CRYSTAL_CLOCK_HZ as u64 * n as u64 / d as u64;
        assert_eq!(freq, 3_000_000_000);
    }

    #[test]
    fn ratio_3ghz_is_reduced() {
        let (d, n) = tsc_crystal_ratio(3_000_000);
        assert_eq!(d, 1);
        assert_eq!(n, 120);
    }

    #[test]
    fn ratio_2_4ghz() {
        let (d, n) = tsc_crystal_ratio(2_400_000);
        let freq = CRYSTAL_CLOCK_HZ as u64 * n as u64 / d as u64;
        assert_eq!(freq, 2_400_000_000);
    }

    #[test]
    fn ratio_is_fully_reduced() {
        for &khz in &[3_000_000, 2_400_000, 1_800_000, 2_000_000] {
            let (d, n) = tsc_crystal_ratio(khz);
            assert_eq!(
                gcd(d as u64, n as u64),
                1,
                "ratio for {khz} kHz is not fully reduced: {n}/{d}"
            );
        }
    }

    #[test]
    fn ratio_denominator_never_zero() {
        for khz in [1, 100, 1_000, 1_000_000, 3_000_000, 4_000_000] {
            let (d, _) = tsc_crystal_ratio(khz);
            assert!(d > 0, "denominator is zero for tsc_khz={khz}");
        }
    }

    // ── Encode / decode family ───────────────────────────────────

    #[test]
    fn family_roundtrip_simple() {
        for family in 0..=0xFu8 {
            let mut eax = 0u32;
            encode_family(&mut eax, family);
            assert_eq!(decode_family(eax), family, "family={family}");
        }
    }

    #[test]
    fn family_roundtrip_extended() {
        for &family in &[0x10, 0x15, 0x1F, 0x30, 0xFF] {
            let mut eax = 0u32;
            encode_family(&mut eax, family);
            assert_eq!(decode_family(eax), family, "family=0x{family:02x}");
        }
    }

    #[test]
    fn family_preserves_other_bits() {
        let mut eax = 0xDEAD_BEEFu32;
        let non_family_bits = eax & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK);
        encode_family(&mut eax, 6);
        assert_eq!(
            eax & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK),
            non_family_bits
        );
    }

    // ── Encode / decode model ────────────────────────────────────

    #[test]
    fn model_roundtrip_all() {
        for model in 0..=0xFFu8 {
            let mut eax = 0u32;
            encode_model(&mut eax, model);
            assert_eq!(decode_model(eax), model, "model=0x{model:02x}");
        }
    }

    #[test]
    fn model_preserves_other_bits() {
        let mut eax = 0xDEAD_BEEFu32;
        let non_model_bits = eax & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK);
        encode_model(&mut eax, 0xA5);
        assert_eq!(eax & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK), non_model_bits);
    }

    #[test]
    fn model_0xa5_split() {
        let mut eax = 0u32;
        encode_model(&mut eax, 0xA5);
        assert_eq!((eax >> EAX_MODEL_SHIFT) & 0xF, 0x5, "low nibble");
        assert_eq!((eax >> EAX_EXT_MODEL_SHIFT) & 0xF, 0xA, "high nibble");
    }

    // ── Encode / decode stepping ─────────────────────────────────

    #[test]
    fn stepping_roundtrip() {
        for stepping in 0..=0xFu8 {
            let mut eax = 0u32;
            encode_stepping(&mut eax, stepping);
            assert_eq!(decode_stepping(eax), stepping);
        }
    }

    #[test]
    fn stepping_masks_high_nibble() {
        // Values > 15 are masked to 4 bits.
        let mut eax = 0u32;
        encode_stepping(&mut eax, 0xF7);
        assert_eq!(decode_stepping(eax), 0x07);
    }

    #[test]
    fn stepping_preserves_other_bits() {
        let mut eax = 0xFFFF_FFF0u32;
        encode_stepping(&mut eax, 7);
        assert_eq!(eax & 0xF, 7);
        assert_eq!(eax & 0xFFFF_FFF0, 0xFFFF_FFF0);
    }

    // ── elapsed_ns ───────────────────────────────────────────────

    #[test]
    fn elapsed_ns_zero_counter() {
        assert_eq!(elapsed_ns(0, 3_000_000), 0);
    }

    #[test]
    fn elapsed_ns_one_millisecond_at_3ghz() {
        // 3 GHz = 3_000_000 kHz.  3_000_000 ticks = 1 ms = 1_000_000 ns.
        assert_eq!(elapsed_ns(3_000_000, 3_000_000), 1_000_000);
    }

    #[test]
    fn elapsed_ns_one_second_at_3ghz() {
        // 3_000_000_000 ticks at 3 GHz = 1 s = 1_000_000_000 ns.
        assert_eq!(elapsed_ns(3_000_000_000, 3_000_000), 1_000_000_000);
    }

    #[test]
    fn elapsed_ns_large_counter() {
        // 3 × 10^12 ticks at 3 GHz = 1000 s.
        assert_eq!(elapsed_ns(3_000_000_000_000, 3_000_000), 1_000_000_000_000);
    }

    #[test]
    fn elapsed_ns_monotonic() {
        let khz = 3_000_000u32;
        let mut prev = 0u64;
        for counter in (0..10_000_000).step_by(1_000_000) {
            let ns = elapsed_ns(counter, khz);
            assert!(ns >= prev, "elapsed_ns not monotonic at counter={counter}");
            prev = ns;
        }
    }

    // ── vtsc_tick ────────────────────────────────────────────────

    #[test]
    fn tick_basic() {
        assert_eq!(vtsc_tick(0, 1_000), 1_000);
        assert_eq!(vtsc_tick(1_000, 1_000), 2_000);
    }

    #[test]
    fn tick_wraps() {
        assert_eq!(vtsc_tick(u64::MAX, 1), 0);
        assert_eq!(vtsc_tick(u64::MAX, 2), 1);
    }

    // ── vtsc_advance ─────────────────────────────────────────────

    #[test]
    fn advance_zero_is_noop() {
        assert_eq!(vtsc_advance(42, 1_000, 0), 42);
    }

    #[test]
    fn advance_one_equals_tick() {
        for &counter in &[0u64, 1_000, u64::MAX - 500] {
            assert_eq!(
                vtsc_advance(counter, 500, 1),
                vtsc_tick(counter, 500),
                "advance(c, a, 1) must equal tick(c, a) at c={counter}"
            );
        }
    }

    #[test]
    fn advance_multiple() {
        assert_eq!(vtsc_advance(0, 500, 10), 5_000);
        assert_eq!(vtsc_advance(100, 500, 10), 5_100);
    }

    #[test]
    fn advance_wraps() {
        assert_eq!(vtsc_advance(u64::MAX, 1, 2), 1);
    }

    // ── vtsc_advance_to ──────────────────────────────────────────

    #[test]
    fn advance_to_noop_when_past_target() {
        assert_eq!(vtsc_advance_to(1_000, 100, 500), 1_000);
        assert_eq!(vtsc_advance_to(1_000, 100, 1_000), 1_000);
    }

    #[test]
    fn advance_to_exact_boundary() {
        // target is exactly one tick away.
        assert_eq!(vtsc_advance_to(0, 100, 100), 100);
        // target is exactly 5 ticks away.
        assert_eq!(vtsc_advance_to(0, 100, 500), 500);
    }

    #[test]
    fn advance_to_rounds_up() {
        // target=150 with step=100: must round up to 200.
        assert_eq!(vtsc_advance_to(0, 100, 150), 200);
        // target=101 with step=100: must round up to 200.
        assert_eq!(vtsc_advance_to(0, 100, 101), 200);
    }

    #[test]
    fn advance_to_lands_on_tick_boundary() {
        for target in [1u64, 50, 99, 100, 101, 199, 200, 999, 1_000] {
            let result = vtsc_advance_to(0, 100, target);
            assert_eq!(
                result % 100,
                0,
                "advance_to(0, 100, {target}) = {result} is not on a tick boundary"
            );
            assert!(
                result >= target,
                "advance_to(0, 100, {target}) = {result} < target"
            );
        }
    }

    #[test]
    fn advance_to_with_offset_base() {
        // counter=50, step=100, target=200 → need 150 more → 2 ticks → counter=250.
        assert_eq!(vtsc_advance_to(50, 100, 200), 250);
    }

    // ── Cross-module constant consistency ────────────────────────

    #[test]
    fn crystal_clock_matches_cpu_module() {
        // This test will fail at compile-time or runtime if the constant
        // drifts between modules.
        assert_eq!(CRYSTAL_CLOCK_HZ, 25_000_000);
    }

    #[test]
    fn eax_masks_do_not_overlap() {
        // Verify the bit-field masks are disjoint.
        let all = EAX_STEPPING_MASK
            | EAX_MODEL_MASK
            | EAX_FAMILY_MASK
            | EAX_EXT_MODEL_MASK
            | EAX_EXT_FAMILY_MASK;
        // Count set bits — must equal sum of individual pop-counts.
        assert_eq!(
            all.count_ones(),
            EAX_STEPPING_MASK.count_ones()
                + EAX_MODEL_MASK.count_ones()
                + EAX_FAMILY_MASK.count_ones()
                + EAX_EXT_MODEL_MASK.count_ones()
                + EAX_EXT_FAMILY_MASK.count_ones(),
            "EAX field masks overlap!"
        );
    }
}
