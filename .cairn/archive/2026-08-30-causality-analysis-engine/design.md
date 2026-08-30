# Design: Causality Analysis Engine

## Context

Replay verdicts, bug reports, snapshots, and counterfactual execution already exist. Delta debugging exists for fault schedules. Interleaving deltas and automated attribution do not.

## Decisions

### 1. Interleaving minimization is pure delta debugging

The core treats a vCPU schedule and its variants as a delta sequence. It applies ddmin to the schedule and variant steps that separate a reproducing run from a non-reproducing prefix. The shell runs the candidate replays; the core decides which candidates to keep.

### 2. Attribution ranks candidate causes

The core ranks candidate cause classes: seed, fault schedule, declared event, and variant policy. Ranking uses whether removing or neutralizing the candidate changes the replay outcome. The rank is a probability estimate, never a proof.

### 3. Shell and core stay separated

The core receives candidate replay outcomes and returns ranked attribution and a minimized delta set. The shell reads verdict and checkpoint artifacts and drives candidate executions.

### 4. Attribution is bounded

A declared budget limits candidate executions. Exhausted budget yields an explicit partial-attribution result, never a fabricated cause.

### 5. Evidence binds the analysis

Every attribution and minimized delta artifact enters the receipt with the replay verdict, the candidate set, and the budget spent.

## Risks

Causality inference can under-rank an indirect cause such as a downstream consistency effect. The engine will report that limitation. Interleaving minimization can be expensive because each candidate is a full replay, so budgets must be explicit and recorded.
