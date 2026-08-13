# Design: Benchmark Bug Corpus

## Context

Dogfood rounds, bug reports, and hunt directories exist. The exploration engine has coverage guidance and campaign profiles. A corpus of known-bug entries lets us measure the engine and validate rarity statistics against a known distribution.

## Decisions

### 1. Corpus entries are versioned and self-contained

Each entry carries buildable workload source, a property, an expected verdict, a rarity profile, and a canonical round identity. Entries live in-repo under a declared corpus root.

### 2. The manifest is Nickel-owned

A Nickel contract defines the corpus schema: entry id, entry class, expected verdict, rarity profile, and runtime bounds. Rust reads the exported JSON projection. This follows the repo rule that human-authored configuration and receipts are Nickel-backed.

### 3. Three entry classes match known bug families

- An interleaving entry reproduces only under a specific schedule, in the async-heartbeat class.
- A liveness entry reproduces only under a specific sequence, in the stuck-progress class.
- A rarity entry has a measured base probability over a seeded distribution, used to validate the findability model.

### 4. The runner is bounded and receipts bind

The runner brings up each entry, asserts the expected verdict, and emits a receipt binding the config digest, round identities, and verdict. Nothing is marked complete without a passing assertion. Entries follow the generic-workload and generic-assertion doctrine that Antithesis reports for its own benchmark curriculum.

## Risks

A corpus entry that stops reproducing can mean the bug is fixed or the harness changed. The runner must report the ambiguity. A rarity entry with an unstable probability estimate misleads the findability check, so the probability must be re-measured when the harness changes.
