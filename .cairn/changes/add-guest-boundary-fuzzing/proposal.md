## Why

Two guest-controlled boundaries decode or plan from bytes the guest writes: the
hypercall page payload and assertion catalog/event frames in
`chaoscontrol-protocol`, and virtio descriptor chains and requests in
`chaoscontrol-vmm`. Both are pure, bounded functions with adversarial unit tests but
no fuzzing. A simulator that finds guest bugs must not panic, wrap, or mis-plan on a
hostile guest.

Source comparison: Chris Wellons, [“Tips for more effective fuzz testing with
AFL++”](https://nullprogram.com/blog/2025/02/05/). It is guidance, not a parity or
coverage claim. GitHub issue: [#5](https://github.com/brittonr/chaoscontrol/issues/5).

## What Changes

Add libFuzzer targets for the payload decoder, the catalog/descriptor/event frame
decoders, the descriptor-chain planner, and the block/net/entropy/scratch request
planners. Targets parse fuzzer input deterministically, use small queue, chain, and
memory shapes, and check round-trip, canonicality, and plan-soundness invariants
independently of the code under test. Add a bounded smoke script and a non-default
`nix run .#fuzz-smoke` app, exclude both fuzz workspaces from the main workspace,
and commit seed corpora derived from the existing protocol and virtio fixtures.

KVM, VM execution, and guest runtime are not involved: every target is a pure
function over arguments.

## Consumer and Ownership

Consumer: host-side safety review of the protocol and virtio trust boundaries.
Durable capability: repeatable, bounded fuzzing of guest-controlled decoders and
planners. Adoption path: `nix run .#fuzz-smoke` in operator or scheduled jobs;
findings become regression tests beside the existing adversarial tests. Maintenance
owner: chaoscontrol-protocol and chaoscontrol-vmm maintainers. Repeatability:
committed corpora, a documented bounded command, and deterministic input parsing.

## Non-Goals

No KVM or in-guest fuzzing, no coverage-parity target, no sanitizer claim beyond
libFuzzer defaults, no CI required lane by default (the app is opt-in), no crash
triage service, no replacement of adversarial unit tests, and no claim that a clean
smoke run proves decoder or planner correctness.
