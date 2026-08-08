# Validation

Date: 2026-08-07

## Result

The active package duplicated the stronger assertion-identity contract and implementation already accepted on `origin/main`.

The duplicate delta now matches the accepted contract. Sync made no accepted-spec change.

## Focused checks

The following commands passed:

```console
cargo test -p chaoscontrol-protocol
cargo test -p chaoscontrol-sdk assert
cargo test -p chaoscontrol-fault oracle
cargo test -p chaoscontrol-evidence sdk_local
cargo test -p chaoscontrol-vmm controller
cargo clippy -p chaoscontrol-protocol -p chaoscontrol-sdk -p chaoscontrol-fault -p chaoscontrol-evidence -p chaoscontrol-vmm --all-targets -- -D warnings
```

Positive tests cover catalog admission, stable SDK identities, bound events, report projection, snapshot restore, and controller aggregation.

Negative tests cover malformed catalogs, events without accepted identity, inactive runs, forged snapshots, legacy carriers, unknown report kinds, and failed controller restore.

## Lifecycle checks

The canonical policy was:

`/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`

Repository validation and the proposal, design, and tasks gates passed.

The sync dry-run and execution made no accepted-spec change. Archive execution succeeded at:

`cairn/archive/2026-08-07-reject-assertion-identity-conflicts`

Post-archive validation passed with eight active changes.

## Claim boundary

These checks prove the tested assertion identity, transport, snapshot, report, and aggregation behavior. They do not prove BLAKE3 collision impossibility, arbitrary guest correctness, or whole-system release eligibility.
