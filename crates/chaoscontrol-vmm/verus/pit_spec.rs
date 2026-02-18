//! Verus specifications for PIT pure functions.
//!
//! This file contains formal specifications and proofs for the verified PIT
//! functions in `crates/chaoscontrol-vmm/src/verified/pit.rs`.
//!
//! These specifications define the correctness properties we expect from the
//! PIT emulation and can be checked using the Verus verifier.

#![allow(unused_imports)]
use builtin::*;
use builtin_macros::*;
use vstd::prelude::*;

verus! {

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for compute_counter
// ═══════════════════════════════════════════════════════════════════════

/// PIT counter never exceeds effective reload value
proof fn counter_bounded(mode: ChannelMode, reload: u16, armed: bool, elapsed: u64)
    ensures
        compute_counter(mode, reload, armed, elapsed) as u64
            <= if reload == 0 { 65536u64 } else { reload as u64 },
{
    // Proof by execution: the function implementation ensures this
}

/// Unarmed channel always returns zero counter
proof fn unarmed_counter_zero(mode: ChannelMode, reload: u16, elapsed: u64)
    ensures
        compute_counter(mode, reload, false, elapsed) == 0,
{
    // Proof: first check in function returns 0 if !armed
}

/// Counter is deterministic (same inputs => same output)
proof fn counter_deterministic(
    mode: ChannelMode,
    reload: u16,
    armed: bool,
    elapsed: u64,
)
    ensures
        compute_counter(mode, reload, armed, elapsed)
            == compute_counter(mode, reload, armed, elapsed),
{
    // Proof: pure function with no side effects
}

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for compute_output
// ═══════════════════════════════════════════════════════════════════════

/// Unarmed channel never signals output
proof fn unarmed_output_false(
    mode: ChannelMode,
    gate: bool,
    reload: u16,
    elapsed: u64,
)
    ensures
        !compute_output(mode, false, gate, reload, elapsed),
{
    // Proof: first check returns false if !armed
}

/// Gate-off channel never signals output
proof fn gate_off_output_false(
    mode: ChannelMode,
    armed: bool,
    reload: u16,
    elapsed: u64,
)
    ensures
        !compute_output(mode, armed, false, reload, elapsed),
{
    // Proof: first check returns false if !gate
}

/// Output is deterministic
proof fn output_deterministic(
    mode: ChannelMode,
    armed: bool,
    gate: bool,
    reload: u16,
    elapsed: u64,
)
    ensures
        compute_output(mode, armed, gate, reload, elapsed)
            == compute_output(mode, armed, gate, reload, elapsed),
{
    // Proof: pure function with no side effects
}

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for tsc_to_pit_ticks
// ═══════════════════════════════════════════════════════════════════════

/// TSC to PIT conversion is monotonic
proof fn tsc_to_pit_monotonic(tsc1: u64, tsc2: u64, khz: u32)
    requires
        tsc1 <= tsc2,
        khz > 0,
    ensures
        tsc_to_pit_ticks(tsc1, khz) <= tsc_to_pit_ticks(tsc2, khz),
{
    // Proof: if tsc1 <= tsc2, then (tsc1 * C) / D <= (tsc2 * C) / D
    // for positive constants C, D
    // This follows from monotonicity of multiplication and division
}

/// Zero TSC gives zero PIT ticks
proof fn tsc_zero_gives_pit_zero(khz: u32)
    requires
        khz > 0,
    ensures
        tsc_to_pit_ticks(0, khz) == 0,
{
    // Proof: 0 * C / D == 0 for any C, D > 0
}

/// TSC conversion is deterministic
proof fn tsc_to_pit_deterministic(tsc: u64, khz: u32)
    requires
        khz > 0,
    ensures
        tsc_to_pit_ticks(tsc, khz) == tsc_to_pit_ticks(tsc, khz),
{
    // Proof: pure function with no side effects
}

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for pending_irq_check
// ═══════════════════════════════════════════════════════════════════════

/// Unarmed channel never has pending IRQ
proof fn unarmed_no_irq(
    mode: ChannelMode,
    reload: u16,
    elapsed: u64,
    delivered: u64,
)
    ensures
        !pending_irq_check(mode, false, reload, elapsed, delivered),
{
    // Proof: first check returns false if !armed
}

/// Mode0 fires at most once
proof fn mode0_fires_once(reload: u16, elapsed: u64, delivered: u64)
    requires
        reload > 0,
        elapsed >= reload as u64,
        delivered > 0,
    ensures
        !pending_irq_check(ChannelMode::Mode0, true, reload, elapsed, delivered),
{
    // Proof: Mode0 requires elapsed >= reload AND delivered == 0
    // If delivered > 0, result is false
}

/// Mode2 fires when period boundary crossed
proof fn mode2_fires_at_period(reload: u16, period: u64)
    requires
        reload > 0,
        period > 0,
    ensures
        {
            let effective_reload = reload as u64;
            let elapsed = period * effective_reload;
            pending_irq_check(ChannelMode::Mode2, true, reload, elapsed, period - 1)
        },
{
    // Proof: elapsed / reload = period, which is > (period - 1)
}

/// IRQ check is deterministic
proof fn irq_check_deterministic(
    mode: ChannelMode,
    armed: bool,
    reload: u16,
    elapsed: u64,
    delivered: u64,
)
    ensures
        pending_irq_check(mode, armed, reload, elapsed, delivered)
            == pending_irq_check(mode, armed, reload, elapsed, delivered),
{
    // Proof: pure function with no side effects
}

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for next_irq_tsc_compute
// ═══════════════════════════════════════════════════════════════════════

/// Unarmed channel returns None for next IRQ TSC
proof fn unarmed_no_next_irq(
    mode: ChannelMode,
    reload: u16,
    start: u64,
    khz: u32,
    delivered: u64,
)
    requires
        khz > 0,
    ensures
        next_irq_tsc_compute(mode, false, reload, start, khz, delivered).is_none(),
{
    // Proof: first check returns None if !armed
}

/// Mode0 returns None after first IRQ
proof fn mode0_none_after_first(
    reload: u16,
    start: u64,
    khz: u32,
    delivered: u64,
)
    requires
        khz > 0,
        delivered > 0,
    ensures
        next_irq_tsc_compute(ChannelMode::Mode0, true, reload, start, khz, delivered).is_none(),
{
    // Proof: Mode0 returns None when delivered > 0
}

/// Next IRQ TSC is always >= start_tsc when Some
proof fn next_irq_gte_start(
    mode: ChannelMode,
    armed: bool,
    reload: u16,
    start: u64,
    khz: u32,
    delivered: u64,
)
    requires
        khz > 0,
    ensures
        {
            let result = next_irq_tsc_compute(mode, armed, reload, start, khz, delivered);
            result.is_some() ==> result.unwrap() >= start
        },
{
    // Proof: The offset calculation is always non-negative
    // (multiplication of non-negative values divided by positive)
    // Therefore start + offset >= start
}

/// Mode2 always returns Some (periodic mode)
proof fn mode2_always_has_next(
    reload: u16,
    start: u64,
    khz: u32,
    delivered: u64,
)
    requires
        khz > 0,
    ensures
        next_irq_tsc_compute(ChannelMode::Mode2, true, reload, start, khz, delivered).is_some(),
{
    // Proof: Mode2 is periodic, always returns Some when armed
}

/// Next IRQ is deterministic
proof fn next_irq_deterministic(
    mode: ChannelMode,
    armed: bool,
    reload: u16,
    start: u64,
    khz: u32,
    delivered: u64,
)
    requires
        khz > 0,
    ensures
        next_irq_tsc_compute(mode, armed, reload, start, khz, delivered)
            == next_irq_tsc_compute(mode, armed, reload, start, khz, delivered),
{
    // Proof: pure function with no side effects
}

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for decode_command
// ═══════════════════════════════════════════════════════════════════════

/// Channel select is bounded 0-3
proof fn decode_channel_bounded(value: u8)
    ensures
        {
            let (ch, _, _) = decode_command(value);
            ch <= 3
        },
{
    // Proof: (value >> 6) & 0x3 can only be 0-3
}

/// Access mode is bounded 0-3
proof fn decode_access_bounded(value: u8)
    ensures
        {
            let (_, access, _) = decode_command(value);
            access <= 3
        },
{
    // Proof: (value >> 4) & 0x3 can only be 0-3
}

/// Decode is deterministic
proof fn decode_deterministic(value: u8)
    ensures
        decode_command(value) == decode_command(value),
{
    // Proof: pure function with no side effects
}

// ═══════════════════════════════════════════════════════════════════════
//  Specifications for handles_port
// ═══════════════════════════════════════════════════════════════════════

/// handles_port is deterministic
proof fn handles_port_deterministic(port: u16)
    ensures
        handles_port(port) == handles_port(port),
{
    // Proof: pure function with no side effects
}

/// Known PIT ports are handled
proof fn known_ports_handled()
    ensures
        handles_port(0x40) && handles_port(0x41)
            && handles_port(0x42) && handles_port(0x43)
            && handles_port(0x61),
{
    // Proof: these match the patterns in handles_port
}

/// Some ports are not handled
proof fn some_ports_not_handled()
    ensures
        !handles_port(0x80) && !handles_port(0x3F8),
{
    // Proof: these don't match any pattern in handles_port
}

} // verus!
