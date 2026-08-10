# Validation evidence

## Dependency admission

- `bounded-exec` source: `https://git.onix.computer/z2CpqLFpdP36fZXYUK5ZNWxMibpCo.git`
- admitted revision: `29dac88ecded94457572db3fdfaaaab95fa91525`
- license: `AGPL-3.0-or-later`
- exact Cargo checkout revision: PASS
- exact revision `cargo test --workspace`: PASS, 11 tests
- ChaosControl dependency policy: PASS for bans, licenses, and sources
- no sibling path dependency is present

## Typed command checks

- Nickel valid projection and committed JSON comparison: PASS
- Nickel free-form negative fixture: PASS by rejection
- Nickel traversal negative fixture: PASS by rejection
- pure command-plan tests: PASS
- direct execution tests: PASS
- scheduler, fleet, hosted, networked, and multi-hypervisor model tests: PASS
- Rust-owned CI plan materialization and KVM caller plan materialization: PASS
- direct packaged replay-readiness execution from typed CI plans: PASS
- command-interpreter source regression: PASS
- architecture boundary check: PASS
- product-scope and evidence-contract checks: PASS
- Rust formatting: PASS
- Rust Clippy with warnings denied: PASS

The adversarial checks cover literal shell metacharacters, an accepted nonzero exit, legacy text, parent traversal, ambient environment, missing identity, malformed limits, output flood, timeout, signal termination, cancellation classification, teardown policy, and evidence overclaim rejection.

## Repository checks

- `cargo test -p chaoscontrol-evidence`: PASS
- `nix flake check -L`: PASS after the final CI and KVM caller migration
- focused `replay-readiness` and `local-multi-hypervisor-kvm-smoke` Nix checks: PASS
- focused final Nix formatting, tests, Clippy, evidence-contract, and dependency-policy checks: PASS
- Cairn proposal, design, and tasks gates: PASS

The remote builder emitted the known nonfatal SSH connection error. Nix continued on the local builder.

## Compatibility and claim boundary

Input plans now execute only the typed `command_plan` object. Legacy `command` strings remain diagnostic-only and never enter process creation.

Generated receipts preserve the existing string `command` field as a non-executable display value. They add `command_plan` and `command_observation` fields for exact typed facts and bounded process results.

This evidence proves bounded behavior for the selected plans, fixtures, exact mechanism revision, and local platform. It does not prove sandboxing, hermeticity, executable trust, child correctness, platform equivalence, orchestration correctness, release eligibility, or absence of defects.
