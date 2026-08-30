# Findability Survival Statistics

## Why

A green run does not prove a rare bug is fixed. Antithesis treats this as a statistics problem and reports survival curves, mean time-to-bug, and projected runs to a stated confidence. ChaosControl records pass and fail verdicts and bug findings, but a receipt carries no rarity-adjusted confidence. Without statistics, an operator can mistake luck for a verified fix.

## What Changes

- Add a pure findability core that fits an exponential model over first-bug-per-subtree observations.
- Add a conservative posterior survival curve using a gamma prior on the bug rate (Lomax tail).
- Add receipt fields for mean time-to-bug, p_survival, confidence threshold, and projected runs.
- Flag subtrees where the independence assumption is violated (a baked-in bug) instead of reporting a false confidence.

## Impact

- **Code**: a pure statistics core plus a shell over round and verdict artifacts.
- **Evidence**: receipts gain bounded, statistical claims about remaining bug presence.
- **Testing**: positive fixtures with a known bug probability; negative fixtures for empty data, a single observation, no-bug runs, and a baked-in bug.

## Non-Goals

- No hosted dashboard or UI.
- No change to fault or verdict semantics.
- No claim that any single run proves absence of a bug.
