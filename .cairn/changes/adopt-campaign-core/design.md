## Context

`Explorer` owns an adaptive search loop. It selects a `FrontierEntry`, creates fault-schedule or input-tree variants, executes branches, observes coverage and findings, and retains interesting snapshots.

`Frontier` currently stores VM snapshots, coverage bitmaps, floating-point scores, selection counts, depth, schedules, and parent IDs. It applies score decay, epsilon-greedy choice, and capacity pruning.

Campaign cannot own these product values. It will own deterministic policy over opaque identities and bounded ranks. Choregraph history will own immutable moment and control-event structure.

## Goals

- Preserve current adaptive frontier behavior under an explicit shared policy boundary.
- Keep KVM, snapshot, schedule, coverage, assertion, finding, and evidence meaning in ChaosControl.
- Replace hidden frontier randomness with explicit entropy tickets.
- Publish selections before expansion work starts.
- Keep multi-seed execution and aggregation outside Campaign.

## Non-goals

- Change the VMM, fault engine, input-tree protocol, mutator, corpus, replay, or minimizer.
- Move snapshot bytes, schedules, choice records, coverage bitmaps, findings, or reports into Campaign.
- Replace the multi-seed runner, its progress format, or its aggregation behavior.
- Make Campaign or Choregraph an executor, persistence service, or evidence authority.

## Decisions

### Decision: Adopt Campaign inside each Explorer

**Choice:** The adapter replaces frontier policy inside one `Explorer` run. It does not replace `campaign.rs`.

**Rationale:** The reusable adaptive-search mechanism is inside `frontier.rs` and `explorer.rs`. The multi-seed runner owns a separate execution concern.

### Decision: Use Choregraph for authoritative structure

**Choice:** Each retained exploration moment maps to one immutable Choregraph event identity. Campaign control events record candidate admission, selection, result acceptance, pruning, and stop decisions.

ChaosControl persists history through its own shell. Choregraph and Campaign never store `SimulationSnapshot` bytes.

**Rationale:** Structural lineage is reusable. Snapshot storage and restorable-state truth remain product concerns.

### Decision: Keep snapshot eligibility explicit

**Choice:** The adapter marks a candidate eligible only when ChaosControl supplies an exact restorable snapshot identity or an explicit clean-bootstrap operation.

A structural Choregraph moment without a current restorable snapshot is not automatically executable.

**Rationale:** History structure does not prove that VM state exists or can be restored.

### Decision: Model one frontier entry as one reusable candidate

**Choice:** A candidate binds one parent moment, exploration mode, mutation or choice-selection profile, branch budget, and cost profile.

Each selection binds a new operation identity, successor selection count, current guidance record, and entropy ticket.

After durable selection, ChaosControl derives and executes the bounded branch group. The result can report zero or more child moments.

**Rationale:** Existing rounds can select one frontier entry many times and execute many variants per selection.

### Decision: Convert product scores at the adapter

**Choice:** ChaosControl converts frontier scores and rare-edge policy to bounded integer ranks under one versioned adapter profile.

Campaign stores the latest guidance record and applies declared ordering, selection-count decay, and canonical tie break. ChaosControl retains score meaning.

The migration corpus must expose any ordering difference caused by numeric conversion.

**Rationale:** Floating-point comparison and coverage policy do not belong in a product-neutral core.

### Decision: Supply explicit epsilon entropy

**Choice:** ChaosControl obtains deterministic entropy from its seeded exploration source. It supplies a bounded entropy ticket and source identity to Campaign selection.

Campaign applies the selected epsilon threshold and candidate-index rule without reading randomness.

**Rationale:** Equal state and entropy must produce equal policy decisions.

### Decision: Fence expansion with durable selection

**Choice:** ChaosControl can generate a selection plan in memory. It cannot execute KVM expansion until its shell durably accepts the exact Campaign control event and Choregraph branch update.

A stale control generation causes a fresh projection and replan.

**Rationale:** Concurrent or resumed planners must not execute one candidate from stale state.

### Decision: Keep branch generation and execution local

**Choice:** `ScheduleMutator`, `input_tree::select_alternatives`, worker pools, sequential execution, VMM controllers, clocks, signals, and KVM operations remain in ChaosControl.

Campaign sees only the selected opaque expansion operation and later bounded structural results.

**Rationale:** These mechanisms carry product execution and nondeterminism authority.

### Decision: Keep observations and findings local

**Choice:** ChaosControl computes enriched coverage, corpus interest, assertion states, finding identity, and result truth.

The adapter sends only bound child-moment identities, candidate ranks, resource usage, and opaque finding references to Campaign.

**Rationale:** A generic policy cannot establish product observation truth.

### Decision: Preserve stop meaning

**Choice:** Campaign can classify frontier exhaustion, resource exhaustion, policy stop, and explicit consumer stop. ChaosControl retains coverage plateau, short-run finding, signal, maximum-round, and operator meanings.

The adapter maps product stop facts to shared structural classes without changing product reports.

**Rationale:** Shared stop structure must not absorb product evidence or operator semantics.

### Decision: Keep progress and resume in ChaosControl

**Choice:** Existing checkpoints, multi-seed progress, output directories, and resume behavior remain product-owned.

A cached Campaign frontier and outstanding-selection index are derived. ChaosControl validates them against exact history identities or reconstructs them.

**Rationale:** Campaign and Choregraph do not own durable storage or VM snapshot recovery.

### Decision: Cut over through typed parity

**Choice:** Legacy and Campaign-backed policy evaluate the same bounded frontier states, scores, selection counts, entropy tickets, candidate updates, pruning cases, and stop facts.

A selected KVM smoke then compares structural decisions and product observations.

**Rationale:** Compilation and generic conformance cannot detect product adapter drift.

## Dependency Direction

```text
chaoscontrol-explore
  -> chaoscontrol Campaign adapter
      -> published campaign-choregraph
          -> published choregraph-history
```

No dependency points from Campaign or Choregraph to ChaosControl.

## Risks and Trade-offs

- Score conversion can change ordering near numeric boundaries.
- Durable selection adds a publication step before branch execution.
- Choregraph history can outlive snapshot retention. Structural reachability does not imply executable restore state.
- Existing checkpoints omit complete frontier snapshots. Resume can require re-bootstrap and structural reconstruction.
- The shared policy can make current selection behavior easier to inspect, but it does not prove better exploration.

## Current Blockers

- Choregraph branchable history has no published implementation revision.
- Campaign has no Rust implementation or published source revision.
- Exact adapter APIs can only be finalized after both contracts exist.

## Non-Claims

This migration does not prove exhaustive exploration, snapshot durability, deterministic KVM execution, finding correctness, replay success, authorization, production readiness, or release eligibility.
