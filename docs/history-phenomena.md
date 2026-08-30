# History phenomena checker

The history phenomena checker classifies bounded typed operation histories.

The pure core is in `chaoscontrol-smr::phenomena`. It does not read files, clocks, processes, network state, or environment state.

The `check-history-phenomena` shell reads one bounded regular JSON file. It rejects symbolic links and validates the complete history identity before classification.

## Typed history

Each operation has:

- a stable operation identity;
- a process identity and sequence;
- a committed, aborted, or intermediate status;
- a typed read or write record;
- explicit dependency edges.

A read identifies the write that supplied its value. It can instead identify the initial value or an unattributed value.

The history can contain explicit observation gaps. A gap names the affected operation pair and the missing fact.

BLAKE3 identities bind the source artifact, canonical history, each attached operation, and the final report.

## Classifications

The checker emits these classes:

- `aborted_read`: a read observes an aborted write;
- `intermediate_read`: a read observes an intermediate write;
- `garbage_read`: a read value has no attributed write;
- `stale_read`: a read observes an older version after a newer committed write;
- `lost_write`: a later committed write has no dependency path from the prior committed write;
- `write_cycle`: write-write dependency edges contain a directed cycle.

The write-cycle pass uses bounded linear graph traversal. Each violation contains the BLAKE3 identities of the responsible operations.

The lost-write rule is a conservative bounded criterion. It is not a full concurrent-history solver.

## Incomplete histories

If the artifact declares an observation gap, the checker returns `insufficient_data`. It includes the affected pairs and emits no violation.

This rule prevents missing observations from becoming invented diagnoses.

## Command

```bash
cargo run -p chaoscontrol-evidence --bin check-history-phenomena -- \
  validate history.json

cargo run -p chaoscontrol-evidence --bin check-history-phenomena -- \
  check history.json report.json

cargo run -p chaoscontrol-evidence --bin check-history-phenomena -- \
  check-round operation-history.json
```

`check-round` adapts the existing typed single-register operation-history artifact. It does not parse raw logs.

Exit status `2` means the complete history has a classified violation. Exit status `3` means the history has insufficient data.

## Claim boundary

A classification describes the supplied bounded observations. It does not identify the code defect, solve all legal history interpretations, prove replay, or establish release eligibility.
