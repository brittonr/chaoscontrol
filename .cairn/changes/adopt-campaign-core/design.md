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

## Implementation Decisions

These decisions were taken during implementation. Paths are relative to `crates/chaoscontrol-explore/src/` unless stated. Commits are on `cairn/adopt-campaign-core-20260906`.

### Adapter tree and identities

The pure adapter lives in `campaign_adapter.rs` and `campaign_adapter/` (commit `fb9a221`). It has no filesystem, VM, or clock access.

- `identity.rs`: `AdapterIdentity::for_seed` derives the adapter, projection, control-domain, guidance, entropy, operation, cost, policy, and retention identities. Only the entropy source identity binds the seed.
- `moments.rs`: retained moments become Choregraph drafts. Payload identities bind digests and ticks, never snapshot bytes.
- `candidates.rs`: `candidate_record` binds one frontier entry to its parent moment, mode, entry sequence, marker digest, and operation-generation profile. Reusing a candidate yields distinct selection identities.
- `wire.rs`: an authenticated serde mirror of every Campaign control event and moment draft. Decoding re-derives event and payload identities and rejects unknown fields and trailing bytes.
- `profile.rs`: `admit_config` rejects degenerate configurations before any journal or KVM work: `max_frontier` 0, non-finite or negative weights, no restorable root, and bound overflow. Legacy corpus recycling and the empty-frontier live-controller continuation are reachable only from those configurations, so the adapter does not model them.

Commit `a0b1b3c` moved the pin to Campaign `c5e20cf2` (D41). Projections now advance by fast-forward through `fence::advance_projection`.

### Integer ranks and explicit division decay

`ranks.rs` emits three rank dimensions: effective score, original score, and `i64::MAX - entry_seq`. A finite nonnegative `f64` keeps its numeric order as its IEEE-754 bit pattern, so `score_rank` uses those bits. It normalizes signed zero and rejects non-finite or negative scores. Sequence numbers above `i64::MAX` are rejected.

Legacy selection divides the score by `1 + times_selected`. Campaign decay cannot express division. The admitted profile therefore uses `SelectionCountDecay::None`, and a subtractive shared decay is rejected as `DoubleDecay`.

After every settlement, the shell admits an explicit guidance revision for the selected candidate (`decay_revision`, `round.rs` `stage_decay`). The revision's generation must be exactly `n + 1`, and it carries reason `chaoscontrol.selection-count-decay.v1`.

### Entropy tickets from legacy draws

Both policies draw the same random values in the same order through `frontier::draw_selection`:

- one `gen::<f64>` unit draw;
- a `gen_range` index draw only when exploring;
- no draw on an empty frontier.

`entropy.rs` `EntropyDraws::from_legacy` converts the draws exactly. The unit draw becomes `k = u * 2^53`, and `explore_draw = (k * 10^6) >> 53` is compared with Campaign threshold `100_000`. `k = 900719925474099` explores and `k = 900719925474100` exploits, which matches the legacy `u < 0.1` predicate.

`entropy_ticket` stores `explore_draw` and the index draw little-endian in bytes 0 to 8 and 8 to 16. The ticket counter is `selection_count + 1`. The Campaign backend takes its explore flag from its own ticket through `ticket_explores` (commit `b6c9194`), not from the legacy predicate. The exploration order `[1, 2]` reproduces legacy vector order.

### Retention rank order and genesis pruning

Capacity pruning uses `plan_pruning` with retention rank order `[1, 2]` (`profile.rs` `retention_plan_request`, `round.rs` `stage_prune`). That order is original score, then insertion order. Among equal lowest scores it drops the newest entry, which is the legacy truncation.

Legacy prunes when the root is admitted. The corpus case `root_beyond_max_frontier` showed that Campaign pruned only at the first settlement. The genesis segment now stages the same prune (commit `3b5cb62`, `frontier_backend/campaign.rs`). The control-event bound counts one root guidance record per possible bootstrap marker.

### Round protocol and publication fence

`Explorer` runs every round through the `FrontierBackend` trait in `explorer/frontier_backend.rs` (commit `3314e8b`). The steps are `begin_round`, `authorize`, execute, `finish_round`, and `record_stop`.

- **`begin_round`**: retires candidates whose parent snapshot is no longer restorable. It then draws entropy and plans. It has no other effect.
- **`authorize`**: stages the selection segment `[SELECTION, CONTROL SelectionPublished]` and appends it. After the commit, `fence::execution_authority` must grant the publication. Only then is the parent snapshot's eligibility checked (`round.rs` `executable`).
- **Execute**: only `Authorization::Execute(AuthorizedRound)` exposes the parent snapshot. Mutation, input-tree alternatives, probes, restore, worker dispatch, and KVM stay in `explorer.rs`.
- **Ineligible after publication**: the backend closes the selection unobserved, retires the candidate, and returns `Authorization::Skipped`. The legacy policy has no analogue for this path.
- **`finish_round`** (`round.rs` `settle`): writes one settlement segment. Its records are child MOMENTs, `ResultObserved`, `CandidatesAdmitted`, per-candidate guidance, ENTRY records, the decay revision, `PruneFrontier`, and ROUND. Snapshots are retained or released only after this segment commits.
- **Checkpoint**: `checkpoint.json` is written after the settlement segment and binds that segment's epoch, sequence, and digest (commit `1ba05d1`).

### Journal segments, fences, and durability classes

`explorer/journal/` is a product-owned segment journal under `output_dir/campaign-journal/` (commit `a60cccd`).

- A non-blocking `flock` on `LOCK` excludes concurrent writers.
- Segments are `epoch-NNNN/seg-NNNNNNNN.cbor`. Each is canonical CBOR carrying the schema `chaoscontrol.campaign-journal-segment.v1`, epoch, sequence, previous-segment BLAKE3 digest, record count, records, and a body digest. All bounds are named constants.
- Publication stages the segment in the destination directory, then writes it, fsyncs it, persists it without replacement, and fsyncs the directory. The I/O is injectable through `PublicationIo`.

`publication.rs` `classify_failure` assigns every failure one durability class:

| Class | Cause | Effect |
| --- | --- | --- |
| `NotCommitted` | Failure before the rename | Nothing becomes visible, and the same sequence can be retried. |
| `DestinationExists` | The target already exists | The writer is stale until `resynchronize`. |
| `DurabilityUnknown` | Any other rename failure, or a directory fsync failure | The writer is poisoned, and only a fresh open recovers it. |

The backend (`frontier_backend/campaign.rs` and `session.rs`) halts on `NotCommitted` and `DurabilityUnknown`. A `DestinationExists` on a selection segment resynchronizes and replays the tail. The rebuilt state must be equal before the backend replans once with the recorded draws. A second collision, or a foreign segment, is `StaleControlBranch`.

Product records use kinds 1 to 6: GENESIS, MOMENT, CONTROL, ENTRY, SELECTION, and ROUND (`frontier_backend/records.rs`). Replay rejects unknown kinds and out-of-order genesis records.

### Recovery epochs

On open, the journal verifies every epoch's segment chain, and `records.rs` `replay_epoch` replays only the last epoch. If the recovered epoch has not stopped, `close_recovered_epoch` does two things:

- it closes each outstanding selection unobserved with reason `snapshot-state-lost`;
- it records the stop `ConsumerRequested{resume-epoch-closed}`.

`begin_epoch` then opens a new epoch. Its GENESIS record carries `previous_epoch_digest`. A resumed checkpoint's journal binding is compared with the recovered segment. On a mismatch the journal wins and a warning is logged.

### Stop mapping

`campaign_adapter/stops.rs` `stop_reason` and the backend's `record_stop` record only the first stop, as a structural class:

| Product stop | Structural class |
| --- | --- |
| `coverage_plateau`, short-run `bug_found`, `interrupted` | `ConsumerRequested` with a bounded reason identity |
| `completed` | `SelectionLimitReached` |
| `completed` with `max_rounds` 0 | `ConsumerRequested{completed-without-selection}` |
| `frontier_exhausted` | `FrontierExhausted` |

A Campaign wait in the sequential Explorer is the typed `FrontierError::UnexpectedWait`, never exhaustion. Reports print an exhaustive-search non-claim for every stop class. The post-round stop order is shared by `Explorer::run` and the parity driver through `explorer/round_stops.rs`.

### Frontier trace and replay parity

Both policies write `output_dir/frontier-trace.jsonl` (schema `chaoscontrol.frontier-trace.v1`, `frontier_backend/trace.rs`). It holds header, root, round, and stop records, with draws, selection, branches, admitted, pruned, retained, and work.

The VM-free corpus in `explorer/frontier_backend/parity/` drives both backends with one script and one counted RNG (commit `b6c9194`). It compares every decision field after every round. The report keeps decision-field equality, measured semantic work, and unmeasured time and memory as separate facts. `cutover_readiness` fails with a typed `ParityDrift` on any decision drift; work is reported but not gated.

`bin/check-frontier-parity.rs` (commit `c6c81cc`) replays each recorded run through the other policy. It reconstructs the exact `rand` words: unit word `k << 11` and index word `ceil(index * 2^64 / len)`. It then writes a `chaoscontrol.frontier-parity-receipt.v1` receipt that is never overwritten.

### Evidence boundary

`chaoscontrol-evidence/src/campaign_evidence_boundary.rs` (commit `1c06a11`) classifies the Campaign journal, the frontier trace, and Campaign or Choregraph schemas as structural. Product evidence admission refuses them: receipts, snapshot references, replay verdicts, accepted proofs, readiness promotion, and dogfood admission. `fence::receipt_claims` returns `ReceiptClaims::no_claims()`.

### Cutover and legacy-diagnostic rollback gates

Commit `420cb05` makes `FrontierPolicy::Campaign` the default for the CLI, profiles, checkpoints, and campaign progress (`explorer/frontier_policy.rs`). The retired spellings `legacy` and `campaign-candidate` fail with a typed error that names the replacement.

`LegacyDiagnostic` must be selected explicitly and has these rules:

- It requires `output_dir`.
- It prints a banner.
- It writes a durable `frontier-policy.json` receipt with `release_eligible: false`.
- It marks checkpoints and reports release-ineligible. The mark is sticky across a Campaign resume.
- A resume never selects it implicitly.

Commit `be95656` adds `chaoscontrol-evidence/src/frontier_policy_gate.rs`. The dogfood receipt, accepted-dogfood-config, and KVM release matrix gates refuse diagnostic outputs with a typed `LegacyDiagnosticRefusal`. The Nickel contracts and the flake evidence check mirror that refusal.

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

## Current State and Remaining Blockers

- Dependency publication and fresh consumer acquisition pass for Campaign `e23e3edf` and Choregraph history `b3e08e19`. See `public-acquisition-2026-09-07.md`.
- Journal authority: runs without explicit journal authority are rejected before KVM work (`durable-storage-decision.md`). The durable journal, the Campaign adapter, and the publication fence are implemented. See the implementation decisions above and `verification.md`.
- Model parity, the adaptive KVM smoke, and the cutover with legacy-diagnostic rollback are committed. See `verification.md` and `kvm-smoke-2026-09-25.md`.
- **Blocker: final release pin.** Cargo pins Campaign development revision `c5e20cf2`, but the Nix `campaign-src` input still names `e23e3edf`. Both must move to the Campaign release revision. This needs the forge to serve that revision.
- **Blocker: full gate.** The full `nix flake check -L` gate has not run. Nix builds that vendor `Cargo.lock` cannot acquire Campaign until the re-pin lands.
- **Blocker: evidence fixture.** The `product-type leakage` fixture for `r[chaoscontrol.campaign_policy.evidence.invalid]` is missing. Today only the dependency direction enforces that boundary.

## Non-Claims

This migration does not prove exhaustive exploration, snapshot durability, deterministic KVM execution, finding correctness, replay success, authorization, production readiness, or release eligibility.
