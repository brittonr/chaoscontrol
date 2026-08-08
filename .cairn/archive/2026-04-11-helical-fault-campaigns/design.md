## Context

ChaosControl already has the low-level pieces for helical fault testing: campaign mode, schedule diversity, process/network/disk faults, checkpointing, replay, and minimization. What it lacks is a first-class way to ask for a reusable multi-phase scenario instead of hand-writing concrete fault schedules every time.

The important design constraint is that the explorer, minimizer, and replay engine already understand concrete `FaultSchedule`s well. Helical scenarios should therefore compile down to ordinary schedules rather than teaching every subsystem a new execution model.

## Goals / Non-Goals

**Goals:**
- Introduce named, deterministic helical scenario families that compile to ordinary fault schedules.
- Provide built-in storage-focused helical scenarios that combine disk, restart, and network faults in reusable patterns.
- Preserve high-level scenario metadata across checkpoint, report, replay, and minimization flows.
- Make the campaign CLI ergonomic enough that a user can request a literature-backed scenario without hand-authoring JSON.

**Non-Goals:**
- Replacing direct fault schedules. Hand-written schedules should remain supported.
- Building a full scenario DSL or external language in the first pass.
- Making the fault engine phase-aware at runtime; phase logic belongs in schedule generation.
- Encoding every Jepsen pattern up front. A small built-in family set is enough to start.

## Decisions

### 1. Model helical scenarios as deterministic generators over `FaultSchedule`

A new `ScenarioFamily` enum plus `ScenarioConfig` struct will generate a concrete `FaultSchedule` and a phase summary from `(family, seed, num_vms, phase_ticks, turns, knobs...)`.

**Rationale:** the rest of the system already speaks concrete schedules. Generators preserve determinism while keeping replay, minimization, and bug deduplication simple.

**Alternative considered:** execute a higher-level scenario interpreter during exploration. Rejected because it would force the explorer, checkpointing, and replay paths to understand phases directly.

### 2. Preserve both the high-level scenario config and the materialized phase plan

Bug reports, checkpoints, and campaign reports will store the selected scenario family, its config, and the materialized phase summary alongside the concrete schedule.

**Rationale:** the high-level label explains intent; the materialized phases explain what actually happened; the concrete schedule keeps replay exact.

**Alternative considered:** store only the final `FaultSchedule`. Rejected because it loses the reusable scenario identity and makes reports hard to compare.

### 3. Rotate targets by role-free ring arithmetic in the first version

Helical rotation will initially be based on stable VM indices (`0 -> 1 -> 2 -> ...`) rather than trying to discover semantic roles like leader/follower at generation time.

**Rationale:** role-aware generation would couple scenario generation to guest semantics and make pre-run determinism harder. Index rotation is simple, deterministic, and still exercises the overlapping failure shape.

**Alternative considered:** inspect guest state to target the current leader. Rejected for the first pass because it would require runtime feedback loops and guest-specific hooks.

### 4. Ship a small built-in family set first

The initial families should be enough to exercise the pattern space:
- `network-ring`: rotating partitions and restarts
- `volatile-write-ring`: rotating `DiskFsyncLie` / kill / restart / heal phases
- `degraded-io-ring`: rotating `DiskSlow` or `DiskPartialRead` plus recovery windows

**Rationale:** a small built-in set is easier to validate and document than an open-ended DSL.

**Alternative considered:** scenario files only. Rejected because the first win is convenience and consistency.

### 5. Keep minimization over the concrete schedule, not the generator

The minimizer will continue to reduce the materialized `FaultSchedule`. It will preserve the scenario metadata in the bug artifact, but it will not attempt to synthesize a smaller generator config.

**Rationale:** ddmin already works on concrete schedules. Preserving metadata gives context without making minimization a search over scenario parameters.

**Alternative considered:** minimize at the scenario-parameter level first. Rejected because it is a separate optimization problem with unclear semantics.

## Risks / Trade-offs

- **[Scenario abstraction leak]** -> Users may expect the generator label alone to be enough for exact replay. Mitigation: always persist the concrete schedule and phase summary.
- **[Too much convenience, not enough control]** -> Built-in families may not match every workload. Mitigation: direct schedules remain available and can still coexist with scenario-generated schedules later.
- **[Phase windows too coarse]** -> Fixed phase ticks may miss short timing windows. Mitigation: expose phase duration and turn count as CLI knobs from day one.
- **[Report bloat]** -> Per-phase summaries can grow long on big campaigns. Mitigation: keep full JSON, but truncate human-readable phase tables after a reasonable limit.

## Migration Plan

1. Add `ScenarioFamily`, `ScenarioConfig`, and deterministic materialization helpers in `chaoscontrol-explore` / `chaoscontrol-fault`.
2. Add built-in network and storage helical families with tests proving deterministic generation.
3. Thread scenario metadata through checkpoints, bug reports, reports, replay, and minimization.
4. Add CLI flags on `run` / `campaign` / `campaign resume` and update human-readable reporting.
5. Add smoke tests that run a short scenario end-to-end and confirm metadata survives resume and replay.

## Open Questions

- Should the first storage family always include a clean recovery window, or should that be a caller-controlled flag?
- Do we want to allow helical generators to compose with an explicit user-provided schedule in the first pass, or keep them mutually exclusive until semantics are clearer?
