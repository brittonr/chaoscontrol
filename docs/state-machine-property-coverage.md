# State-machine property coverage

ChaosControl runs bounded command sequences against small reference models. The implementation and model must agree after each accepted or rejected command.

## Covered models

The suite covers these stateful boundaries:

- Scheduler selection, blocking, halt, wake, stale events, and exact commits.
- Scheduler snapshots, restore, overlay materialization, and continuation.
- Fault ordering, observation, network heal, reset, and subset rejection.
- Assertion catalog binding, event counters, snapshots, merge, and rejection.
- Virtio queue capacity and transport status transitions.
- Evidence rows that are partial, stale, duplicate, malformed, or complete.

The models also check independent invariants. These checks include state validity, no prohibited mutation, exact identity binding, capacity, continuation, and deterministic output.

## Profiles

`contracts/property-coverage/profiles.ncl` owns the reviewed profiles. `profiles.json` is its exact runtime projection.

The `fast` lane uses small bounds in normal CI. The `deep` lane uses larger finite bounds in scheduled CI. Each profile records target models, named seeds, sequence limits, step limits, shrink limits, receipt limits, and a time budget.

Committed baseline receipts live in `dogfood-results/state-machine-property-coverage-20260809/`. CI compares new receipts with these deterministic baselines.

Run the fast lane:

```bash
cargo run -p chaoscontrol-property-suite --bin run-property-lane -- \
  --lane fast \
  --output target/property-coverage/fast-receipt.json
```

Run the deep lane:

```bash
cargo run -p chaoscontrol-property-suite --bin run-property-lane -- \
  --lane deep \
  --output target/property-coverage/deep-receipt.json
```

Run the portable Nix gate:

```bash
nix build .#checks.x86_64-linux.property-coverage -L
```

## Counterexamples

A failure records the profile, target suite, seed, named invariant, original length, minimized length, shrink attempts, and minimized command sequence.

The shrinker removes command ranges. It accepts a smaller sequence only when the same named invariant still fails. Reviewed minimized cases live under `contracts/property-coverage/fixtures/regressions/` and run as normal tests.

## Claim boundary

A passing lane reports bounded model agreement for the selected profile. It is not a formal proof. It does not establish complete state coverage, KVM behavior, whole-system correctness, or the absence of defects.

The property lanes do not replace the required KVM release matrix. Portable CI keeps these claims separate.
