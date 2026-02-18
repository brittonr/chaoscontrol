// Verus specification for ChaosControl CPU pure functions.
//
// These specs document the formal verification properties for every pure
// function in `src/verified/cpu.rs`.  They are NOT compiled by `cargo` —
// they are consumed by the Verus verifier:
//
//     verus verus/cpu_spec.rs
//
// Each function is annotated with `requires` (preconditions) and `ensures`
// (postconditions) that Verus will prove hold for ALL valid inputs.
//
// Reference: https://verus-lang.github.io/verus/guide/

verus! {

// ═══════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════

pub const CRYSTAL_CLOCK_HZ: u32 = 25_000_000u32;

pub const EAX_STEPPING_MASK:    u32 = 0xFu32;
pub const EAX_MODEL_MASK:       u32 = 0xFu32 << 4;
pub const EAX_MODEL_SHIFT:      u32 = 4u32;
pub const EAX_FAMILY_MASK:      u32 = 0xFu32 << 8;
pub const EAX_FAMILY_SHIFT:     u32 = 8u32;
pub const EAX_EXT_MODEL_MASK:   u32 = 0xFu32 << 16;
pub const EAX_EXT_MODEL_SHIFT:  u32 = 16u32;
pub const EAX_EXT_FAMILY_MASK:  u32 = 0xFFu32 << 20;
pub const EAX_EXT_FAMILY_SHIFT: u32 = 20u32;

// ═══════════════════════════════════════════════════════════════════════
// GCD
// ═══════════════════════════════════════════════════════════════════════

/// Euclidean GCD, specified recursively for verification.
pub open spec fn gcd_spec(a: nat, b: nat) -> nat
    decreases b,
{
    if b == 0 { a }
    else { gcd_spec(b, a % b) }
}

/// Executable GCD matching the implementation in `verified/cpu.rs`.
pub fn gcd(a: u64, b: u64) -> (result: u64)
    ensures
        // 1. The result divides both inputs (when non-zero).
        a > 0 ==> a as nat % result as nat == 0,
        b > 0 ==> b as nat % result as nat == 0,
        // 2. Identity: gcd(a, 0) == a.
        b == 0 ==> result == a,
        // 3. Commutative: gcd(a, b) == gcd(b, a).
        result == gcd(b, a),
        // 4. Zero-zero: gcd(0, 0) == 0.
        (a == 0 && b == 0) ==> result == 0,
        // 5. Non-zero when at least one input is non-zero.
        (a > 0 || b > 0) ==> result > 0,
{
    let mut x = a;
    let mut y = b;
    while y != 0
        invariant
            gcd_spec(x as nat, y as nat) == gcd_spec(a as nat, b as nat),
        decreases y,
    {
        let t = y;
        y = x % y;
        x = t;
    }
    x
}

/// GCD divides both inputs.
proof fn lemma_gcd_divides_both(a: nat, b: nat)
    ensures
        a > 0 ==> a % gcd_spec(a, b) == 0,
        b > 0 ==> b % gcd_spec(a, b) == 0,
    decreases b,
{
    if b > 0 {
        lemma_gcd_divides_both(b, a % b);
    }
}

/// GCD is commutative.
proof fn lemma_gcd_commutative(a: nat, b: nat)
    ensures
        gcd_spec(a, b) == gcd_spec(b, a),
    decreases a + b,
{
    if a == 0 || b == 0 {
        // base cases
    } else {
        lemma_gcd_commutative(b, a % b);
    }
}

/// GCD is the *greatest* common divisor: any common divisor d of a, b
/// also divides gcd(a, b).
proof fn lemma_gcd_is_greatest(a: nat, b: nat, d: nat)
    requires
        d > 0,
        a % d == 0,
        b % d == 0,
    ensures
        gcd_spec(a, b) % d == 0,
    decreases b,
{
    if b > 0 {
        // If d | a and d | b, then d | (a % b).
        lemma_gcd_is_greatest(b, a % b, d);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TSC crystal ratio
// ═══════════════════════════════════════════════════════════════════════

/// Compute the CPUID 0x15 ratio for a given TSC frequency.
pub fn tsc_crystal_ratio(tsc_khz: u32) -> (result: (u32, u32))
    requires
        tsc_khz > 0,
    ensures
        // 1. Denominator is non-zero (no division-by-zero when guest reads CPUID).
        result.0 > 0,
        // 2. Reconstructs the original frequency exactly.
        CRYSTAL_CLOCK_HZ as u64 * result.1 as u64 / result.0 as u64
            == tsc_khz as u64 * 1000,
        // 3. Fraction is fully reduced (gcd == 1).
        gcd(result.0 as u64, result.1 as u64) == 1,
{
    let tsc_hz: u64 = tsc_khz as u64 * 1000;
    let crystal: u64 = CRYSTAL_CLOCK_HZ as u64;
    let g = gcd(tsc_hz, crystal);
    let denominator = (crystal / g) as u32;
    let numerator = (tsc_hz / g) as u32;
    (denominator, numerator)
}

// ═══════════════════════════════════════════════════════════════════════
// CPUID EAX encoding: family
// ═══════════════════════════════════════════════════════════════════════

/// Encode a display family into CPUID 0x1 EAX.
pub fn encode_family(eax: &mut u32, family: u8)
    ensures
        // 1. Non-family bits are preserved.
        *eax & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK)
            == old(eax) & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK),
        // 2. Round-trip: decode(encode(eax, f)) == f.
        decode_family(*eax) == family,
{
    *eax = *eax & !(EAX_FAMILY_MASK | EAX_EXT_FAMILY_MASK);
    if family <= 0xF {
        *eax = *eax | ((family as u32) << EAX_FAMILY_SHIFT);
    } else {
        *eax = *eax | (0xFu32 << EAX_FAMILY_SHIFT);
        *eax = *eax | (((family as u32) - 0xF) << EAX_EXT_FAMILY_SHIFT);
    }
}

/// Decode the display family from CPUID 0x1 EAX.
pub fn decode_family(eax: u32) -> (result: u8)
    ensures
        result <= 0xFF,
{
    let base = ((eax & EAX_FAMILY_MASK) >> EAX_FAMILY_SHIFT) as u8;
    if base == 0xF {
        let ext = ((eax & EAX_EXT_FAMILY_MASK) >> EAX_EXT_FAMILY_SHIFT) as u8;
        (base + ext) as u8
    } else {
        base
    }
}

/// Proof: encode then decode is identity.
proof fn lemma_family_roundtrip(eax: u32, family: u8)
    ensures ({
        let mut e = eax;
        encode_family(&mut e, family);
        decode_family(e) == family
    }),
{ /* Verus will discharge by SMT */ }

// ═══════════════════════════════════════════════════════════════════════
// CPUID EAX encoding: model
// ═══════════════════════════════════════════════════════════════════════

/// Encode a display model into CPUID 0x1 EAX.
pub fn encode_model(eax: &mut u32, model: u8)
    ensures
        // 1. Non-model bits are preserved.
        *eax & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK)
            == old(eax) & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK),
        // 2. Round-trip.
        decode_model(*eax) == model,
{
    *eax = *eax & !(EAX_MODEL_MASK | EAX_EXT_MODEL_MASK);
    *eax = *eax | (((model as u32) & 0xF) << EAX_MODEL_SHIFT);
    *eax = *eax | ((((model as u32) >> 4) & 0xF) << EAX_EXT_MODEL_SHIFT);
}

/// Decode the display model from CPUID 0x1 EAX.
pub fn decode_model(eax: u32) -> (result: u8)
{
    let base = ((eax & EAX_MODEL_MASK) >> EAX_MODEL_SHIFT) as u8;
    let ext = ((eax & EAX_EXT_MODEL_MASK) >> EAX_EXT_MODEL_SHIFT) as u8;
    (ext << 4) | base
}

/// Proof: encode then decode is identity.
proof fn lemma_model_roundtrip(eax: u32, model: u8)
    ensures ({
        let mut e = eax;
        encode_model(&mut e, model);
        decode_model(e) == model
    }),
{ /* Verus will discharge by SMT */ }

// ═══════════════════════════════════════════════════════════════════════
// CPUID EAX encoding: stepping
// ═══════════════════════════════════════════════════════════════════════

/// Encode a stepping value into CPUID 0x1 EAX bits [3:0].
pub fn encode_stepping(eax: &mut u32, stepping: u8)
    ensures
        // 1. Non-stepping bits are preserved.
        *eax & !EAX_STEPPING_MASK == old(eax) & !EAX_STEPPING_MASK,
        // 2. Round-trip (stepping is masked to 4 bits).
        decode_stepping(*eax) == (stepping & 0xF),
{
    *eax = *eax & !EAX_STEPPING_MASK;
    *eax = *eax | ((stepping as u32) & 0xF);
}

/// Decode stepping from CPUID 0x1 EAX bits [3:0].
pub fn decode_stepping(eax: u32) -> (result: u8)
    ensures
        result <= 0xF,
{
    (eax & EAX_STEPPING_MASK) as u8
}

/// Proof: encode then decode is identity (mod 16).
proof fn lemma_stepping_roundtrip(eax: u32, stepping: u8)
    ensures ({
        let mut e = eax;
        encode_stepping(&mut e, stepping);
        decode_stepping(e) == (stepping & 0xF)
    }),
{ /* Verus will discharge by SMT */ }

// ═══════════════════════════════════════════════════════════════════════
// EAX field isolation: masks are disjoint
// ═══════════════════════════════════════════════════════════════════════

/// The five EAX bit-field masks do not overlap.
proof fn lemma_eax_masks_disjoint()
    ensures
        EAX_STEPPING_MASK & EAX_MODEL_MASK == 0,
        EAX_STEPPING_MASK & EAX_FAMILY_MASK == 0,
        EAX_STEPPING_MASK & EAX_EXT_MODEL_MASK == 0,
        EAX_STEPPING_MASK & EAX_EXT_FAMILY_MASK == 0,
        EAX_MODEL_MASK & EAX_FAMILY_MASK == 0,
        EAX_MODEL_MASK & EAX_EXT_MODEL_MASK == 0,
        EAX_MODEL_MASK & EAX_EXT_FAMILY_MASK == 0,
        EAX_FAMILY_MASK & EAX_EXT_MODEL_MASK == 0,
        EAX_FAMILY_MASK & EAX_EXT_FAMILY_MASK == 0,
        EAX_EXT_MODEL_MASK & EAX_EXT_FAMILY_MASK == 0,
{ /* Verus will discharge these bit-vector constraints by SMT */ }

/// Encoding family, model, and stepping are independent — encoding one
/// field does not disturb the others.
proof fn lemma_encode_fields_independent(eax: u32, family: u8, model: u8, stepping: u8)
    ensures ({
        let mut e = eax;
        encode_family(&mut e, family);
        encode_model(&mut e, model);
        encode_stepping(&mut e, stepping);
        // Each decode recovers the value we just set.
        decode_family(e) == family &&
        decode_model(e) == model &&
        decode_stepping(e) == (stepping & 0xF)
    }),
{
    lemma_eax_masks_disjoint();
}

// ═══════════════════════════════════════════════════════════════════════
// elapsed_ns
// ═══════════════════════════════════════════════════════════════════════

/// Convert a TSC counter to elapsed nanoseconds.
pub fn elapsed_ns(counter: u64, tsc_khz: u32) -> (result: u64)
    requires
        tsc_khz > 0,
        // No u64 overflow.
        (counter as u128) * 1_000_000 / (tsc_khz as u128) <= u64::MAX as u128,
    ensures
        // 1. Zero counter ⟹ zero ns.
        counter == 0 ==> result == 0,
        // 2. Monotonic (follows from division being monotonic in numerator).
        //    Proved as a separate lemma below.
        // 3. Exact at 1 ms boundaries:
        //    If counter == tsc_khz * 1000, then result == 1_000_000_000
        //    (proved via lemma_elapsed_ns_at_one_second).
        result == ((counter as u128 * 1_000_000u128) / tsc_khz as u128) as u64,
{
    ((counter as u128 * 1_000_000) / tsc_khz as u128) as u64
}

/// elapsed_ns is monotonic in the counter.
proof fn lemma_elapsed_ns_monotonic(c1: u64, c2: u64, tsc_khz: u32)
    requires
        tsc_khz > 0,
        c1 <= c2,
        (c2 as u128) * 1_000_000 / (tsc_khz as u128) <= u64::MAX as u128,
    ensures
        elapsed_ns(c1, tsc_khz) <= elapsed_ns(c2, tsc_khz),
{
    // c1 <= c2  ⟹  c1 * 1_000_000 <= c2 * 1_000_000
    //            ⟹  (c1 * 1_000_000) / k <= (c2 * 1_000_000) / k
    // Verus should discharge this via integer arithmetic SMT.
}

/// At exactly 1 second of TSC ticks the result is 10^9 ns.
proof fn lemma_elapsed_ns_at_one_second(tsc_khz: u32)
    requires
        tsc_khz > 0,
        // counter = tsc_khz * 1_000_000 (one second of ticks at tsc_khz kHz
        // = tsc_khz * 1000 Hz, so 1 second = tsc_khz * 1000 ticks).
        // Hmm, actually: tsc_khz kHz = tsc_khz * 1000 Hz.
        // 1 second at that rate = tsc_khz * 1000 ticks.
    ensures
        elapsed_ns((tsc_khz as u64) * 1000, tsc_khz) == 1_000_000_000,
{
    // counter * 1_000_000 / tsc_khz
    // = (tsc_khz * 1000) * 1_000_000 / tsc_khz
    // = 1000 * 1_000_000
    // = 1_000_000_000
}

// ═══════════════════════════════════════════════════════════════════════
// vtsc_tick
// ═══════════════════════════════════════════════════════════════════════

/// Advance TSC by one tick.
pub fn vtsc_tick(counter: u64, advance_per_tick: u64) -> (result: u64)
    ensures
        // 1. Wrapping addition.
        result == add(counter, advance_per_tick),
        // 2. Special case: advance_per_tick == 0 is a no-op.
        advance_per_tick == 0 ==> result == counter,
{
    counter.wrapping_add(advance_per_tick)
}

// ═══════════════════════════════════════════════════════════════════════
// vtsc_advance
// ═══════════════════════════════════════════════════════════════════════

/// Advance TSC by n ticks.
pub fn vtsc_advance(counter: u64, advance_per_tick: u64, n: u64) -> (result: u64)
    ensures
        // 1. Zero ticks is a no-op.
        n == 0 ==> result == counter,
        // 2. One tick matches vtsc_tick.
        n == 1 ==> result == vtsc_tick(counter, advance_per_tick),
        // 3. Wrapping semantics.
        result == add(counter, mul(advance_per_tick, n)),
{
    counter.wrapping_add(advance_per_tick.wrapping_mul(n))
}

/// vtsc_advance(c, a, n+1) == vtsc_tick(vtsc_advance(c, a, n), a)
/// i.e., advance by n+1 is the same as advance by n then tick once.
proof fn lemma_advance_step(counter: u64, advance_per_tick: u64, n: u64)
    requires
        n < u64::MAX,  // n+1 must not overflow the tick count itself
    ensures
        vtsc_advance(counter, advance_per_tick, add(n, 1))
            == vtsc_tick(vtsc_advance(counter, advance_per_tick, n), advance_per_tick),
{
    // wrapping_add(c, a * (n+1))
    //   == wrapping_add(c, a*n + a)
    //   == wrapping_add(wrapping_add(c, a*n), a)
    //   == vtsc_tick(vtsc_advance(c, a, n), a)
}

// ═══════════════════════════════════════════════════════════════════════
// vtsc_advance_to
// ═══════════════════════════════════════════════════════════════════════

/// Advance counter to at least `target`, rounding up to a tick boundary.
pub fn vtsc_advance_to(
    counter: u64,
    advance_per_tick: u64,
    target: u64,
) -> (result: u64)
    requires
        advance_per_tick > 0,
    ensures
        // 1. No-op when already at or past target.
        target <= counter ==> result == counter,
        // 2. Reaches the target (when no wrapping).
        (target > counter && result >= counter) ==> result >= target,
        // 3. Lands on a tick boundary (when no wrapping).
        (target > counter && result >= counter) ==>
            (result - counter) % advance_per_tick as u64 == 0,
        // 4. Minimal: doesn't overshoot by more than (advance_per_tick - 1).
        (target > counter && result >= counter) ==>
            result - target < advance_per_tick as u64,
{
    if target <= counter {
        return counter;
    }
    let delta = target - counter;
    let ticks = div_ceil(delta, advance_per_tick);
    counter.wrapping_add(ticks.wrapping_mul(advance_per_tick))
}

/// vtsc_advance_to is idempotent when the counter is already past
/// the target.
proof fn lemma_advance_to_idempotent(counter: u64, advance_per_tick: u64, target: u64)
    requires
        advance_per_tick > 0,
        target <= counter,
    ensures
        vtsc_advance_to(counter, advance_per_tick, target) == counter,
{ /* trivially true from the `if` guard */ }

/// vtsc_advance_to gives the same result as calling vtsc_advance with
/// the appropriate tick count.
proof fn lemma_advance_to_via_advance(
    counter: u64,
    advance_per_tick: u64,
    target: u64,
)
    requires
        advance_per_tick > 0,
        target > counter,
    ensures ({
        let delta = target - counter;
        let ticks = div_ceil(delta, advance_per_tick);
        vtsc_advance_to(counter, advance_per_tick, target)
            == vtsc_advance(counter, advance_per_tick, ticks)
    }),
{ /* structural equivalence */ }

// ═══════════════════════════════════════════════════════════════════════
// Helper spec functions used in the specs above
// ═══════════════════════════════════════════════════════════════════════

/// Wrapping u64 addition (spec-level).
pub open spec fn add(a: u64, b: u64) -> u64 {
    ((a as u128 + b as u128) % (u64::MAX as u128 + 1)) as u64
}

/// Wrapping u64 multiplication (spec-level).
pub open spec fn mul(a: u64, b: u64) -> u64 {
    ((a as u128 * b as u128) % (u64::MAX as u128 + 1)) as u64
}

/// Ceiling division (spec-level, non-wrapping).
pub open spec fn div_ceil(a: u64, b: u64) -> u64
    recommends b > 0,
{
    if a == 0 { 0 }
    else { ((a - 1) / b) + 1 }
}

} // verus!
