# Design: SUT-Declared Event Branching

## Context

The SDK already emits reachable, sometimes, and always assertion kinds with versioned identities. The VMM already takes full snapshots and the explorer already maintains a frontier of parent snapshots. What is missing is a guest-declared event that the harness treats as a branch opportunity.

## Decisions

### 1. A declared marker is an assertion-family event

The new SDK surface is a marker with a stable logical key in the existing assertion identity namespace. It records that control reached an interesting state. It can carry structured details plus optional canonical state and logical-position refs. Those instance refs do not change the marker's logical identity. The marker does not pass or fail on its own.

### 2. Markers produce frontier entries

When the VMM observes a marker, it captures a snapshot as a parent candidate and yields a frontier entry. The explorer assigns priority by marker rarity and novelty, reusing the existing frontier scoring path.

### 3. Markers are bounded

Marker count per run is bounded. Repeated identical markers at the same state collapse. A marker that exceeds its bound records a typed limit event instead of corrupting the frontier.

### 4. Evidence binds markers

Bug reports and replay verdicts record the marker identity, owning guest or process, tick, optional state and logical-position refs, and parent snapshot reference. A reproduced marker-linked bug requires every present ref and the parent snapshot to validate.

### 5. Missing markers fail closed

A declared marker that exploration never reaches produces coverage-gap evidence. A campaign that claims marker coverage must record marker reachability.

## Risks

Marker placement is workload discipline. Too many markers flood the frontier. The rarity and novelty scoring must suppress common markers, or exploration loses focus. Evidence identity must stay stable across source moves, matching the existing assertion-catalog rules.
