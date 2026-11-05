## Why

ChaosControl's guest TSC advances by a fixed amount per VM exit
(`DEFAULT_TSC_ADVANCE`), so guest time depends on exit frequency rather than
retired work. A CPU-bound loop that never exits observes no time passing and its
timeouts cannot fire. RewindVM's "counter time" instead advances time from retired
guest work. This change evaluates that model for ChaosControl as an opt-in mode
while the per-exit mode stays the default.

Source comparison: Farid Zakaria, [“A clock made of branches”](https://fzakaria.com/2026/11/01/a-clock-made-of-branches).
It is comparison material, not parity or a correctness claim.

GitHub issue: [#6](https://github.com/brittonr/chaoscontrol/issues/6).

## What Changes

Record a gated design for a progress-based virtual TSC where
`tsc = base + f(guest_instructions)` over the guest-only retired-instruction
progress the scheduler already consumes, plus the fixture criteria that would admit
or reject it. Runtime implementation, a new default, and support promotion stay
out of scope until the PMU prerequisite admits a profile.

The design must define how HLT deadline jumps, snapshot/restore, and PIT `tsc_khz`
interact with the mode, how replay treats measurement skid, and which outcomes
(skew, non-reproducible boundaries, lost progress) must be rejected rather than
repaired.

## Consumer and Ownership

Consumer: timeout-bearing CPU-bound workloads whose guest time must track work.
Durable capability: a reviewed clock-mode contract and fixture plan, not a second
scheduler or PMU evidence rail. Adoption path: a later implementation under
`chaoscontrol-vmm` clock ownership, opt-in by profile, gated on
[`harden-pmu-transition-evidence`](../harden-pmu-transition-evidence/proposal.md)
admitting a PMU profile. Maintenance owner: chaoscontrol-vmm maintainers.
Repeatability: the fixture plan requires bit-exact repeat runs and records the
per-exit outcome alongside the progress outcome.

[`guest-os-determinism-boundary`](../../specs/guest-os-determinism-boundary/spec.md)
owns the admitted guest time surface (monotonic reads, hidden direct TSC, no host
clock). This change owns only how the virtual TSC advances inside that surface and
does not redefine its claims.

## Non-Goals

No default-mode change, PMU support promotion, `claims_pmu_support` flip, branch
counting, MSR writes, vendor workarounds, host-clock reads, wall-clock timeouts,
or a claim that retired-instruction counts are exact/replayable on every host.
`pmu_kvm_test`'s measured per-exit skid remains an open risk to admit, not an
assumption to build on.
