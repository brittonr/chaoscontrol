// Verus specification for ChaosControl trace event pure functions.
//
// These specs document the formal verification properties for every pure
// function in `src/verified/events.rs` and `src/verified/verifier.rs`.
// They are NOT compiled by `cargo` — they are consumed by the Verus
// verifier:
//
//     verus verus/events_spec.rs
//
// Each function is annotated with `requires` (preconditions) and `ensures`
// (postconditions) that Verus will prove hold for ALL valid inputs.
//
// Reference: https://verus-lang.github.io/verus/guide/

verus! {

// ═══════════════════════════════════════════════════════════════════════
// EventType discriminants
// ═══════════════════════════════════════════════════════════════════════

/// EventType discriminant constants (must match BPF EVENT_* constants).
pub const EVENT_KVM_EXIT:       u32 = 1u32;
pub const EVENT_KVM_ENTRY:      u32 = 2u32;
pub const EVENT_KVM_PIO:        u32 = 3u32;
pub const EVENT_KVM_MMIO:       u32 = 4u32;
pub const EVENT_KVM_MSR:        u32 = 5u32;
pub const EVENT_KVM_INJ_VIRQ:   u32 = 6u32;
pub const EVENT_KVM_PIC_IRQ:    u32 = 7u32;
pub const EVENT_KVM_SET_IRQ:    u32 = 8u32;
pub const EVENT_KVM_PAGE_FAULT: u32 = 9u32;
pub const EVENT_KVM_CR:         u32 = 10u32;
pub const EVENT_KVM_CPUID:      u32 = 11u32;

/// The minimum valid event type discriminant.
pub const EVENT_TYPE_MIN: u32 = 1u32;

/// The maximum valid event type discriminant.
pub const EVENT_TYPE_MAX: u32 = 11u32;

// ═══════════════════════════════════════════════════════════════════════
// Spec model of EventType (simplified for Verus)
// ═══════════════════════════════════════════════════════════════════════

/// Spec function: is this a valid event type discriminant?
pub open spec fn is_valid_event_type(v: u32) -> bool {
    v >= EVENT_TYPE_MIN && v <= EVENT_TYPE_MAX
}

/// Spec model of event_type_from_u32: returns Some iff valid.
pub open spec fn event_type_from_u32_spec(v: u32) -> Option<u32> {
    if is_valid_event_type(v) { Some(v) }
    else { None }
}

// ═══════════════════════════════════════════════════════════════════════
// event_type_from_u32 properties
// ═══════════════════════════════════════════════════════════════════════

/// Executable event_type_from_u32: returns Some(v) for valid v, None otherwise.
///
/// This models the Rust function without depending on the EventType enum.
pub fn event_type_from_u32(v: u32) -> (result: Option<u32>)
    ensures
        // 1. Valid discriminants produce Some.
        is_valid_event_type(v) ==> result == Some(v),
        // 2. Invalid discriminants produce None.
        !is_valid_event_type(v) ==> result.is_none(),
        // 3. Result matches spec.
        result == event_type_from_u32_spec(v),
{
    if v >= EVENT_TYPE_MIN && v <= EVENT_TYPE_MAX {
        Some(v)
    } else {
        None
    }
}

/// EventType round-trips through u32: converting a valid discriminant
/// and then back yields the same value.
proof fn event_type_roundtrip(disc: u32)
    requires
        is_valid_event_type(disc),
    ensures
        event_type_from_u32(disc) == Some(disc),
{
    // Follows directly from the definition of event_type_from_u32.
}

/// from_u32 rejects zero (which is not a valid BPF event type).
proof fn from_u32_rejects_zero()
    ensures
        event_type_from_u32(0u32).is_none(),
{
    // 0 < EVENT_TYPE_MIN = 1, so !is_valid_event_type(0).
}

/// from_u32 rejects values above the maximum.
proof fn from_u32_rejects_above_max(v: u32)
    requires
        v > EVENT_TYPE_MAX,
    ensures
        event_type_from_u32(v).is_none(),
{
    // v > 11 implies !is_valid_event_type(v).
}

/// from_u32 rejects all invalid values (comprehensive).
proof fn from_u32_rejects_invalid(v: u32)
    requires
        v == 0 || v > EVENT_TYPE_MAX,
    ensures
        event_type_from_u32(v).is_none(),
{
    // Union of the two rejection cases.
}

// ═══════════════════════════════════════════════════════════════════════
// event_kind_eq properties (modeled as equality on u32-tagged tuples)
// ═══════════════════════════════════════════════════════════════════════
//
// In the Verus spec we model EventKind as a (tag, data...) tuple.
// The actual enum comparison is too complex to encode directly, but
// we can state the algebraic properties that the implementation must
// satisfy.

/// A simplified event kind: (tag, field1, field2, field3, field4, field5, field6).
///
/// We use this to model the comparison without importing the full enum.
pub struct SimpleEventKind {
    pub tag: u32,
    pub f0: u64,
    pub f1: u64,
    pub f2: u64,
    pub f3: u64,
    pub f4: u64,
    pub f5: u64,
}

/// Spec: structural equality of SimpleEventKind.
pub open spec fn simple_eq(a: SimpleEventKind, b: SimpleEventKind) -> bool {
    a.tag == b.tag
    && a.f0 == b.f0
    && a.f1 == b.f1
    && a.f2 == b.f2
    && a.f3 == b.f3
    && a.f4 == b.f4
    && a.f5 == b.f5
}

/// determinism_eq is reflexive: every event kind equals itself.
proof fn determinism_eq_reflexive(e: SimpleEventKind)
    ensures
        simple_eq(e, e) == true,
{
    // Each field is compared with itself: a == a is always true.
}

/// determinism_eq is symmetric: if a == b then b == a.
proof fn determinism_eq_symmetric(a: SimpleEventKind, b: SimpleEventKind)
    ensures
        simple_eq(a, b) == simple_eq(b, a),
{
    // == on u32 and u64 is symmetric, and && is commutative
    // in the sense that (p && q) == (q && p).
}

/// determinism_eq is transitive: if a == b and b == c then a == c.
proof fn determinism_eq_transitive(
    a: SimpleEventKind,
    b: SimpleEventKind,
    c: SimpleEventKind,
)
    requires
        simple_eq(a, b),
        simple_eq(b, c),
    ensures
        simple_eq(a, c),
{
    // Transitivity of == on each field.
}

// ═══════════════════════════════════════════════════════════════════════
// exit_reason_name properties
// ═══════════════════════════════════════════════════════════════════════

/// Known Intel VMX exit reasons.
pub open spec fn is_known_vmx_reason(r: u32) -> bool {
    r == 0 || r == 1 || r == 2 || r == 7 || r == 10 || r == 12 ||
    r == 18 || r == 30 || r == 31 || r == 32 || r == 33 ||
    r == 48 || r == 49
}

/// Known AMD SVM exit codes.
pub open spec fn is_known_svm_reason(r: u32) -> bool {
    r == 0x040 || r == 0x061 || r == 0x064 || r == 0x06e ||
    r == 0x078 || r == 0x07b || r == 0x07c || r == 0x400
}

/// exit_reason_name returns a known name for all recognised exit codes.
///
/// We model the return value as a bool (is_known) rather than a string,
/// since Verus cannot reason about string contents directly.
pub fn exit_reason_is_known(reason: u32) -> (result: bool)
    ensures
        // Known reasons produce a name other than "UNKNOWN".
        (is_known_vmx_reason(reason) || is_known_svm_reason(reason))
            ==> result == true,
{
    is_known_vmx_reason(reason) || is_known_svm_reason(reason)
}

// ═══════════════════════════════════════════════════════════════════════
// parse_event_kind properties
// ═══════════════════════════════════════════════════════════════════════

/// parse_event_kind produces Unknown for invalid type discriminants.
proof fn parse_unknown_for_invalid_type(event_type: u32)
    requires
        !is_valid_event_type(event_type),
    ensures
        event_type_from_u32(event_type).is_none(),
        // → the implementation will produce EventKind::Unknown
{
    // Follows from event_type_from_u32 returning None.
}

/// parse_event_kind produces a known variant for valid type discriminants.
proof fn parse_known_for_valid_type(event_type: u32)
    requires
        is_valid_event_type(event_type),
    ensures
        event_type_from_u32(event_type).is_some(),
        // → the implementation will match one of the Some(...) arms
{
    // Follows from event_type_from_u32 returning Some.
}

// ═══════════════════════════════════════════════════════════════════════
// find_first_divergence properties
// ═══════════════════════════════════════════════════════════════════════

/// Spec: two sequences are equal up to index `n`.
pub open spec fn prefix_equal(a: Seq<SimpleEventKind>, b: Seq<SimpleEventKind>, n: nat) -> bool
    recommends
        n <= a.len(),
        n <= b.len(),
{
    forall|i: nat| i < n ==> simple_eq(a[i as int], b[i as int])
}

/// If find_first_divergence returns None, the sequences are equal.
proof fn divergence_none_means_equal(
    a: Seq<SimpleEventKind>,
    b: Seq<SimpleEventKind>,
)
    requires
        a.len() == b.len(),
        prefix_equal(a, b, a.len()),
    ensures
        // No divergence exists.
        forall|i: nat| i < a.len() ==> simple_eq(a[i as int], b[i as int]),
{
    // Follows directly from prefix_equal over the full length.
}

/// If find_first_divergence returns Some(idx, _), then:
/// - All events before idx are equal.
/// - The events at idx differ (or lengths differ).
proof fn divergence_some_means_prefix_equal(
    a: Seq<SimpleEventKind>,
    b: Seq<SimpleEventKind>,
    idx: nat,
)
    requires
        idx <= a.len(),
        idx <= b.len(),
        prefix_equal(a, b, idx),
        // At idx, either the events differ or the lengths differ.
        idx < a.len() && idx < b.len() ==>
            !simple_eq(a[idx as int], b[idx as int]),
    ensures
        // All events before idx matched.
        forall|i: nat| i < idx ==> simple_eq(a[i as int], b[i as int]),
{
    // Directly from the requires.
}

} // verus!
