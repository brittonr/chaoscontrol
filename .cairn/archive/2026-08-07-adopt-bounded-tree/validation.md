# Validation

Date: 2026-08-07

## Result

ChaosControl now uses Bounded Tree for bounded initrd directory observation and source revalidation. Newc mapping, encoding, modes, inode order, padding, duplicate handling, and output bounds remain local.

The dependency is pinned to Radicle revision `b0fd0103bc9eed2c1b6d852045959462d105d8f1`. The producer archive exists at `bounded-tree/cairn/archive/1970-01-01-establish-bounded-tree`.

## Baseline

Before the change, four focused `kernel_bundle_initrd` tests passed.

## Post-change checks

The following commands passed:

```console
cargo test -p chaoscontrol-evidence
cargo test -p chaoscontrol-evidence kernel_bundle_initrd
cargo clippy -p chaoscontrol-evidence --all-targets -- -D warnings
```

Seven focused initrd tests passed. Positive parity compares complete legacy and shared-backed Newc bytes. It also proves repeatable output for files, directories, and internal links.

Negative tests reject relative closure roots, escaping links, overlong paths, output-byte overflow, and invalid init-script inputs.

## Octet baseline blocker

`cargo octet check` reports the same pre-existing repository error on `main` and this branch:

```text
tigerstyle::path_segment_repetition
crates/chaoscontrol-fault/src/schedule.rs:234
FaultScheduleBuilder
```

The adoption adds no Octet finding for `chaoscontrol-evidence`. Strict Clippy for the changed crate passes.

## Lifecycle checks

The canonical policy was:

`/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`

Repository validation and the proposal, design, and tasks gates passed. Sync added the accepted `bounded-tree-adoption` specification.

Archive execution succeeded at:

`cairn/archive/2026-08-07-adopt-bounded-tree`

Post-archive validation passed with seven active changes.

## Claim boundary

Bounded Tree proves only bounded source observation, member facts, and source revalidation under the selected policy. It does not prove root authority, Newc correctness, boot success, replay correctness, publication durability, or release eligibility.
