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
- `wire.rs`: an authenticated serde mirror of every Campaign control event and moment draft. Decoding re-derives event and payload identities and rejects unknown fields, trailing bytes, and any non-canonical encoding (commit `0bbf309`: a payload digest followed by product bytes used to decode as its first 32 bytes).
- `profile.rs`: `admit_config` rejects degenerate configurations before any journal or KVM work: `max_frontier` 0, non-finite or negative weights, no restorable root, and bound overflow. Legacy corpus recycling and the empty-frontier live-controller continuation are reachable only from those configurations, so the adapter does not model them.

Commit `a0b1b3c` moved the pin to Campaign `c5e20cf2` (D41). Projections now advance by fast-forward through `fence::advance_projection`.

Commit `ec1625e` moved Cargo and the Nix `campaign-src` input together to Campaign `260eb9ba` (D42 to D47):

- Publication requires the held projection. `ControlPublicationRequest::previous` changed from `Option<&ValidatedControlProjection>` to `&ValidatedControlProjection`, and `frontier_backend/session.rs` passes its sealed projection.
- `CacheValidationRequest` gained `previous: Option<&ValidatedControlProjection>`. `fence::validate_cache` passes the request through unchanged; the Explorer shell validates no caches (recovery replays the journal).
- The conformance port's `ConsumerShell::load_history(&mut self, request)` became `load_history(&mut self, request, previous: Option<&ValidatedControlProjection>)`. The ChaosControl conformance adapter advances a held projection with `fence::advance_projection` and otherwise cold-starts with `fence::load_projection`, so its answers equal Campaign's on the enumerated load, cache, and gate matrices.

Commit `43042fa` moved Cargo and the Nix `campaign-src` input together to Campaign `99f8eff3` (D48 to D53):

- One guidance rank shape per campaign (D48). The adapter always emits three rank dimensions, so no change was needed.
- Receipts bind the event's own profile, ticket, guidance, and cost reservation, and every receipt projection requires the current binding (D51). ChaosControl projects no Campaign receipts; the conformance port's receipt claims are unchanged.
- Host control capacity (D49). Planning goes through `plan_next_from_published_projection` over the session's held projection, and publication can refuse with `ProjectionCapacityExhausted` or `GenerationCapacityExhausted`. See the capacity stop under "Stop mapping".
- Crossed envelope identities give `Campaign(CrossedIdentity)` on cold load, advance, publication, and cache (D50). The Explorer shell always publishes under its own identities; the conformance suite covers the crossed cases.
- The conformance suite presents non-selection publications to the gate matrices, deeper forks, wide cost units and draws, projection-bound histories, and foreign control roots, and `retire_retained` now picks in suite order (D53). The adapter passes all of them unchanged, and the changed-profile pilot equality is restored.

Commit `97788af` moved Cargo and the Nix `campaign-src` input together to Campaign `a75b5e3a` (D54 to D62):

- Graph event-limit reserve (D54). Publication and `plan_next_from_published_projection` hold the D49 reserve against the Choregraph graph's own `max_events` as well, and refuse with the new `AdapterError::GraphCapacityExhausted`. The planner now returns a typed capacity error when even the planned stop of an open campaign no longer fits, instead of planning a stop that publication would refuse. The Explorer's whole-round check and refusal class cover the graph bound; see "Host control capacity stop".
- Root reserve (D55). `admit_control_root` refuses a root without room for its own stop (`ProjectionCapacityExhausted`, `GenerationCapacityExhausted`, or `GraphCapacityExhausted`), and `derive_projection_binding` refuses an empty or over-long branch name. The Explorer genesis surfaces the typed error before any journal segment is written. With the derived bounds the genesis always fits, and `CONTROL_BRANCH_NAME` (`chaoscontrol.control`, 20 bytes) is within the 64-byte label bound.
- Idempotent republication (D56). Republishing a draft the graph already holds, after a partial commit, now yields the same publication instead of `History(NoChange)`. The Explorer cannot reach that case, so the journal and recovery paths are unchanged. Every control publication is planned on a cloned `ControlSession` and adopted only after its journal segment commits. An uncommitted segment drops the staged graph and halts the backend. Recovery replays each committed record exactly once into a fresh graph, and the one replan after a sequence collision plans from the committed session. No path offers Choregraph a draft that the held graph already contains.
- Host-capacity decisions have no plan receipt (D57, residual). ChaosControl projects no Campaign receipts.
- Rank orders against the fixed rank shape (D58). An exploration or retention order index at or beyond `min(max_rank_dimensions, fixed rank length)` is `InvalidConfig` for every draw and on replay. The adapter's orders (`EXPLORATION_ORDER` and `RETENTION_RANK_ORDER`, both `[1, 2]`) lie inside its fixed three-dimension shape, and `max_rank_dimensions` equals the emitted rank length, so no change was needed. `rank_orders_beyond_the_fixed_rank_shape_are_config_errors_for_every_draw` pins the typed class.
- Budget and guidance waits (D61). A candidate awaits guidance only while it still fits the budget, and budget exhaustion covers every retained candidate. The Explorer records guidance for every candidate in the segment that admits it, so it never waits for guidance, and no change was needed.
- Conformance (D59, D60, D62). The reference planner matches production on budget fit and rank-shape refusals. The catalog adds `selection.level_counts`, `results.out_of_order_settlement`, and `results.child_counts` (74 fixtures) and presents forks at the deepest depth the projection bound allows. A reference-planned selection is refused at adapter publication, and the infra checker classifies Weave-named dependencies as history leaks. The adapter passes every fixture unchanged. The `CrossedIdentities` wrapper also fails `results.out_of_order_settlement`, because Campaign refuses its crossed result with `InvalidChildReservation`.

Commit `f42381c` moved Cargo and the Nix `campaign-src` input together to Campaign `3a182d5a` (D63 to D80):

- Exposed control reserve (D75). A validated projection records the reserve its control history still owes: one settlement per outstanding selection, the stop of an open campaign, and a pending control draft counted as a selection (its own event and one settlement). It exposes `reserved_control_events()`, `reserved_graph_events()` (the same less a pending draft's own event, which the graph already holds), and `graph_room()`, the graph events other appends may still make. Hosts must leave that reserve free and republish a pending draft before any other append. The Explorer's whole-round check now consumes these accessors; see "Host control capacity stop".
- One pending draft per branch head (D63). Publication and planning refuse with `PendingControlDraft { event_id }` while another Campaign draft waits on the head, and `pending_control_draft(..)` decodes one for republication. The Explorer never leaves a pending draft: each publication's graph admission and branch update are staged together on a cloned `ControlSession`, adopted only after the journal segment commits, and replayed together on recovery. Its held projections therefore never carry a pending draft, and the reserve at round start is the stop alone.
- Guidance waits need room to select (D69). The Explorer never waits for guidance, so no change was needed.
- Refusal receipts check state identity first (D72), and core refuses caller-built states whose keyed vectors repeat a key, a result already accepted, and replay output with a repeated key (D79). ChaosControl projects no receipts and builds states only through Campaign publication and replay, so no change was needed.
- Sorted lookups (D71, D79), infra-checker coverage (D66, D78), and spec wording (D70, D80) need no consumer change.
- Conformance (D64, D65, D67, D68, D76, D77, D80). The catalog adds `closures.out_of_order`, `selection.middle_level`, `selection.level_order`, and `selection.interior_level` (78 fixtures) and presents forks over earlier history, exact ticket decoding, held-projection load and cache matrices, exploratory and same-batch claim receipts, binary64-crossing draws, and two finding-bearing results in one campaign. The adapter passes every fixture unchanged, and every broken wrapper fails exactly the fixtures it failed before.

Commit `905c2eb` moved Cargo and the Nix `campaign-src` input together to Campaign `9f1f33a6` (D82 to D90):

- One campaign per graph and envelope (D82). `admit_control_root` refuses with `SharedControlGraph { event_id }` when the graph holds any other Campaign control event under the envelope, and cold load, advance, and publication refuse a graph holding another control root. ChaosControl's envelope (projection schema, adapter, and control domain identities) is seed-independent, so the rule holds only because every campaign gets its own graph: each epoch's genesis admits the root moment into a fresh `HistoryGraph`, `admit_root` refuses a second admission before Campaign sees it, recovery replays each epoch into a fresh graph, and a resume opens a new epoch with its own graph. No code change was needed.
- Occupancy reserve for every descending draft (D83). The reserve counts every Campaign control draft that descends from the branch head through control-parent links, each as its own event plus one settlement, republication counts the drafts chained below, and the planner checks the recorded reserve on every plan. The Explorer never leaves a draft (see D63 above), so its reserve is still the outstanding settlements plus the stop, and `HostCapacity::of` keeps reading the exposed accessors. A planner capacity error still means something outside Campaign publication consumed the reserve; it propagates.
- Stale graph (D84). A held projection refuses to advance onto a graph with fewer events than it recorded, or one lacking a draft it recorded as pending (`StaleGraph`). `ControlSession::seal` always advances over the session's own graph, which only grows, so the Explorer always presents its current graph.
- Nested keyed vectors (D90). Declared costs, reserved costs, and recorded stop dimensions must be strictly increasing. Every ChaosControl candidate declares one cost dimension, so no change was needed.
- `SharedControlGraph` and `StaleGraph` are halting history errors, not capacity refusals: `is_capacity_refusal` is unchanged, and either would propagate as `FrontierError::Adapter(Choregraph(..))`. Neither is reachable from the Explorer.
- Conformance adds `receipts.recorded_stop_overclaim` (79 fixtures) and presents the held matrix across every holder kind and length with the `SharedControlGraph` and `StaleGraph` denial classes. The adapter passes every fixture; `ReceiptOverclaim` also fails the new fixture, and every broken wrapper still fails.

Commit `ccc6ff2` moved Cargo and the Nix `campaign-src` input together to Campaign `1f8e1a74` (D91 to D96):

- Publication only on durably committed branch values (D91, `campaign.publication_fence.durable_base`). Campaign's recovery guarantees now cover only hosts that never plan or publish through a branch value whose compare-and-set is not durable; a pipelined chain of drafts is outside them. The Explorer's graph and branch exist only in its `ControlSession`: `ControlSession::publish` moves the branch to the planned value in the same call that admits the draft, so the graph never holds a draft whose branch update is pending and no draft is ever chained below another. Every publication stages on a clone of the committed session, and `commit` adopts the clone only after its journal segment is durable, so a segment's drafts and branch moves become durable together or not at all. A segment that fails to commit halts the backend (and unknown durability poisons the journal) before anything is planned or published over it, and recovery publishes only on the session replayed from committed segments. No code change was needed; the session module doc names the obligation, and `an_uncommitted_segment_leaves_no_draft_and_nothing_publishes_over_it` pins it.
- Admission currency (D94). Campaign's state-currency check now re-derives and validates the admission, so a caller-built state with an edited admission is `StaleState`. The Explorer takes its first state from Campaign reconstruction (`load_projection` in `ControlSession::genesis`), advances it only with `apply_control_event`, and `seal` requires it to equal the state of the projection Campaign re-validates, so no Explorer path builds a state by hand and no change was needed.
- Claims fixtures present every receipt of their session in every later state (D92). The adapter passes every fixture with the new digests; `ReceiptOverclaim` now fails its non-selection, planner, and recorded-stop fixtures at the admission receipt, and every broken wrapper still fails. Checker (D93) and wording (D95, D96) changes need no consumer change; both pinned conformance tests call `run_suite`.

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
| `control_capacity_exhausted` (Campaign policy only) | `ConsumerRequested{campaign_choregraph::control_capacity_reason_id()}` |

A Campaign wait in the sequential Explorer is the typed `FrontierError::UnexpectedWait`, never exhaustion. Reports print an exhaustive-search non-claim for every stop class. The post-round stop order is shared by `Explorer::run` and the parity driver through `explorer/round_stops.rs`.

#### Host control capacity stop

The host bounds the control lineage three ways (Campaign D49 and D54): the Choregraph projection bound caps the lineage, the control branch advances one generation per event, and the shared Choregraph graph admits at most `max_events` events, child moments included. Campaign's planner and publication reserve room against all three for one selection, one settlement event, and the stop, and every validated projection exposes the reserve its history still owes and the graph room left beside it (D75). A ChaosControl round needs more: it settles with up to `ROUND_CONTROL_EVENTS - 1 + max_candidate_batch` control events (result, candidate batch, one guidance record per entry, decay revision, prune), and it first admits up to `max_child_moments` child moments to the graph. So Campaign's reserve alone could strand an executed selection whose settlement no longer fits. The Explorer therefore handles capacity in three places, all mapped to one product class:

- `begin_round` stops with `ProductStop::ControlCapacityExhausted` before drawing unless the held projection's `stops::HostCapacity` holds `CampaignProfile::round_demand()` (`stops::round_fits_capacity`). `HostCapacity::of` takes the lineage length, `reserved_control_events()`, `maximum_events()`, `graph_room()`, and the admission's branch generation from the projection. The lineage plus the reserved control events plus the round's control events must fit both the projection bound and the branch generation range. Every graph append of the round, its control events and one child moment per branch alike, must fit `graph_room()`. The stop is part of the exposed reserve, so the shell does not add it again. The round therefore never consumes the reserve, and an executed selection always settles in full with the stop still publishable.
- A retirement that publication refuses with `ProjectionCapacityExhausted`, `GenerationCapacityExhausted`, or `GraphCapacityExhausted` (`stops::is_capacity_refusal`) is not staged, and the round becomes the same stop. Campaign keeps the stop reserve, so the stop still publishes.
- `round_decision` maps Campaign's own capacity stop, `ConsumerRequested{control_capacity_reason_id()}`, to the same product class. `Wait(AwaitingResults)` stays the typed `FrontierError::UnexpectedWait`, because the sequential shell settles every round before it plans. A typed capacity error from the planner means another writer consumed the stop reserve itself. The stop would then be refused too, so the error is not mapped to a stop: it propagates as `FrontierError::Adapter(Choregraph(..))`. The Explorer is the only writer of its epoch graph, and the whole-round check keeps its own appends within Campaign's exposed graph room, so this error is not expected.

Mapping of the host-capacity terms to Campaign's exposed values:

| ChaosControl term | Campaign source |
| --- | --- |
| `HostCapacity::lineage` | `ValidatedControlProjection::control_events().len()` |
| `HostCapacity::reserved_control_events` (the stop at round start; settlements and every descending control draft when present) | `reserved_control_events()` |
| `HostCapacity::maximum_events` | `maximum_events()` |
| `HostCapacity::graph_room` | `graph_room()` = `graph_max_events() - graph_events() - reserved_graph_events()`, saturating |
| `HostCapacity::admitted_generation` | `state().admission.projection.generation` |
| `RoundDemand::control_events` | `ROUND_CONTROL_EVENTS + max_candidate_batch` |
| `RoundDemand::history_events` | `RoundDemand::control_events + max_child_moments` |

The product class is `ExplorationStop::ControlCapacityExhausted` (`control_capacity_exhausted`). It is recorded in the Campaign journal, the frontier trace, the exploration report, and multi-seed progress like every other stop, acknowledges incomplete search, and carries no exhaustive or frontier claim. Like the other stop classes, it leaves release eligibility to the frontier policy. With the derived bounds the class is unreachable: the projection bound is the whole history bound, root plus `max_control_events` plus child moments, and `max_control_events` already budgets every round, the genesis, and the stop. It is exercised through test-only tighter projection and graph bounds. The stop uses Campaign's adapter-owned reason, not `ControlEventLimitReached`, because Campaign core replays that reason only from `max_control_events` and cannot see the host bound.

### Frontier trace and replay parity

Both policies write `output_dir/frontier-trace.jsonl` (schema `chaoscontrol.frontier-trace.v1`, `frontier_backend/trace.rs`). It holds header, root, round, and stop records, with draws, selection, branches, admitted, pruned, retained, and work.

The VM-free corpus in `explorer/frontier_backend/parity/` drives both backends with one script and one counted RNG (commit `b6c9194`). It compares every decision field after every round. The report keeps decision-field equality, measured semantic work, and unmeasured time and memory as separate facts. `cutover_readiness` fails with a typed `ParityDrift` on any decision drift; work is reported but not gated.

`bin/check-frontier-parity.rs` (commit `c6c81cc`) replays each recorded run through the other policy. It reconstructs the exact `rand` words: unit word `k << 11` and index word `ceil(index * 2^64 / len)`. It then writes a `chaoscontrol.frontier-parity-receipt.v1` receipt that is never overwritten.

### Evidence boundary

`chaoscontrol-evidence/src/campaign_evidence_boundary.rs` (commit `1c06a11`) classifies the Campaign journal, the frontier trace, and Campaign or Choregraph schemas as structural. Product evidence admission refuses them: receipts, snapshot references, replay verdicts, accepted proofs, readiness promotion, and dogfood admission. `fence::receipt_claims` returns `ReceiptClaims::no_claims()`.

Commit `0bbf309` adds `chaoscontrol.frontier-parity-receipt.v1` (`FrontierParityReceipt`) to the refused structural schemas, mirrored in `contracts/evidence/dogfood-receipt.ncl`. `check-frontier-parity` takes its schema from the same constant. A parity receipt compares structural decisions only, so it never satisfies VM, fault, assertion, finding, replay, minimization, or release evidence.

Product-type leakage is refused at the journal boundary (`frontier_backend/leakage_tests.rs`, commit `0bbf309`). Campaign-owned `MOMENT` and `CONTROL` records carry only opaque identities, digests of at most 32 bytes, integer ranks, counters and ticks, and closed-vocabulary names. Product values (snapshot bytes, coverage bitmaps, assertion reports, fault schedules, finding keys) reach Campaign only as digests. A record carrying one is refused by the closed wire schema or by identity re-derivation, both when decoded and during epoch recovery.

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

- Dependency publication and fresh consumer acquisition pass for Campaign `e23e3edf` and Choregraph history `b3e08e19`. See `public-acquisition-2026-09-07.md`. Cargo and Nix now both pin Campaign development revision `1f8e1a74` (commit `ccc6ff2`); the forge answered HTTP 530, so its locks came from local caches.
- Journal authority: runs without explicit journal authority are rejected before KVM work (`durable-storage-decision.md`). The durable journal, the Campaign adapter, and the publication fence are implemented. See the implementation decisions above and `verification.md`.
- Model parity, the adaptive KVM smoke, and the cutover with legacy-diagnostic rollback are committed. See `verification.md` and `kvm-smoke-2026-09-25.md`.
- The product-type leakage fixtures and parity receipt classification for `r[chaoscontrol.campaign_policy.evidence.invalid]` are committed (`0bbf309`).
- **Blocker: final release pin.** `1f8e1a74` is a development revision. Cargo and Nix must move together to the Campaign release revision, and fresh acquisition from the live endpoint must be observed.
- **Blocker: full gate.** The full `nix flake check -L` gate has not passed on the re-pinned tree.

## Non-Claims

This migration does not prove exhaustive exploration, snapshot durability, deterministic KVM execution, finding correctness, replay success, authorization, production readiness, or release eligibility.
