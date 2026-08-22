# Benchmark Bug Corpus

## Why

We cannot measure whether exploration improves, and we cannot validate the findability statistics, without known bugs. Antithesis maintains an internal curriculum of systems and bugs to benchmark bug-finding performance. ChaosControl has hunt directories and campaign records, but no versioned, repeatable corpus whose entries carry expected verdicts and expected rarity.

## What Changes

- Add a versioned corpus of named workload entries with known bugs.
- Cover four classes: an interleaving race, a liveness failure, a rare bug with a measured base probability, and a protocol-state bug with an independent oracle.
- Add a Nickel contract for corpus manifests and a shell runner that asserts the expected verdict per entry.
- Emit a receipt per entry binding the config digest, round identity, and verdict.

## Impact

- **Code**: a corpus manifest contract plus a bounded runner.
- **Evidence**: reproducible benchmark receipts and regression baselines for exploration.
- **Testing**: positive entries that reproduce as expected and negative variants that are expected to pass.

## Non-Goals

- No claim that an entry measures production likelihood.
- No external vendored SUT source requirement.
- No change to fault or verdict semantics.
