# Verification plan for adopt-campaign-core

## Positive checks

The implementation collects these checks. The `Evidence` section below records the checks that ran.

- Focused legacy `frontier.rs`, `input_tree.rs`, and `explorer.rs` tests pass before core changes.
- Campaign conformance passes for the ChaosControl adapter.
- Equal snapshots and product facts map to equal opaque moment and candidate identities.
- Integer-rank conversion preserves selected corpus ordering and tie classes.
- Equal seeded entropy produces equal ranked or exploratory choices.
- Reusable candidates produce distinct successor selections with matching selection-count decay.
- Capacity pruning keeps the same selected candidates for the parity corpus.
- Exact durable selection publication occurs before mutation, worker, snapshot, or KVM effects.
- One selected expansion can report a bounded group of child moments and close one outstanding selection.
- Full parallel capacity produces a wait decision, not a false frontier or budget stop.
- Coverage, assertions, findings, corpus retention, and report meaning remain in ChaosControl.
- Existing checkpoints and multi-seed progress remain product-owned.
- The selected adaptive KVM smoke produces matching bounded decisions and product observations.
- Clippy, Octet, Cairn, and `nix flake check -L` pass.

Expected evidence includes baseline output, dependency pins, Campaign conformance, model parity, KVM smoke receipts, and current Cairn gate receipts.

## Negative checks

The test suite will reject or expose:

- moving, sibling-path, unpublished, incompatible, or mismatched Campaign and Choregraph revisions;
- ChaosControl schedules, snapshots, coverage, assertions, findings, VMM, storage, or evidence types in Campaign crates;
- unknown moments, duplicate candidates, duplicate selection ordinals, stale guidance, crossed policy, and malformed operation identities;
- floating-point values at the Campaign boundary, integer conversion overflow, and changed near-boundary ordering;
- missing, stale, crossed, or unbound epsilon entropy;
- selection of a moment without a restorable snapshot or clean-bootstrap fact;
- branch execution before exact durable selection publication;
- stale generation, stale head, duplicate selection, and concurrent planner conflicts;
- crossed or duplicate child moments and false observation binding;
- Campaign pruning that erases Choregraph history or product artifacts;
- Campaign or Choregraph receipts presented as VM, fault, assertion, finding, replay, minimization, or release evidence;
- Campaign adoption of multi-seed dispatch, progress, resume, aggregation, or report projection;
- implicit legacy policy selection after supported cutover.

## Evidence

Every commit below is on `cairn/adopt-campaign-core-20260906`. The implementation head is `f45a40f` (Campaign `65eee6e7` re-pin, which adds the `history.state_coverage` fixture; earlier: Campaign `1f8e1a74` re-pin with a test of publication only on durably committed branch values `ccc6ff2`, Campaign `9f1f33a6` re-pin with tests of one campaign per graph, the current graph, and no control drafts `905c2eb`, Campaign `3a182d5a` re-pin and the round capacity taken from Campaign's exposed reserve `f42381c`, Campaign `a75b5e3a` re-pin and graph-bound round reserve `97788af`, Campaign `99f8eff3` re-pin and host control capacity stop `43042fa`, Campaign `260eb9ba` re-pin `ec1625e`, evidence fixtures `0bbf309`); the counts in this subsection are from `420cb05` unless a re-pin subsection below says otherwise. Command prefixes for the development shell and target directory are omitted. All cargo commands ran offline against the committed `Cargo.lock`.

### Focused tests, lint, and static gates

`cargo test --offline -p chaoscontrol-explore -p chaoscontrol-evidence` at `420cb05` ran across 85 test binaries: 706 passed, 0 failed, 1 ignored. The run was repeated at `420cb05` during this lifecycle update with the same totals:

- `chaoscontrol-explore` library: 377 passed, 1 ignored (the existing KVM placeholder).
- `chaoscontrol-explore` CLI binary: 6 passed.
- `chaoscontrol-evidence` library: 161 passed.

Library test counts at each implementation step:

| Commit | Slice | `chaoscontrol-explore --lib` |
| --- | --- | --- |
| `02b78d7` | Baseline before adapter work | 214 passed, 1 ignored |
| `fb9a221` / `2da6277` | Pure adapter and conformance | 261 passed, 1 ignored (52 adapter tests, 7 of them conformance) |
| `a60cccd` | Durable journal (38 journal tests) | 299 passed, 1 ignored |
| `d7838b2` | Backend integration, plumbing, evidence boundary | 339 passed, 1 ignored |
| `c6c81cc` | Parity corpus and checker | 368 passed, 1 ignored |
| `420cb05` | Cutover and diagnostic rollback | 377 passed, 1 ignored |

At `a60cccd`, the journal slice also ran two hand-injected mutation checks, reverted afterwards:

- Removing the poison on unknown durability failed 4 tests.
- Weakening the chain check failed `recovery_rejects_broken_chain`.

Static and lint gates passed at `420cb05` unless noted:

- `cargo clippy --offline -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings`
- `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check`
- `check-evidence-contracts --root .`
- `check-kvm-release-matrix --root .` (positive 1, negative 11)
- `run-kvm-release-matrix --dry-run`
- `check-contract-registry .`
- `check-architecture-boundaries .`
- `nix-instantiate --parse flake.nix`
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib`: exit 0 with no deny-level findings at `fb9a221`, `a60cccd`, `3314e8b`, `c6c81cc`, and `420cb05`. Only warn-level style findings remain.

### Campaign conformance

The adapter's conformance module is `campaign_adapter/tests/conformance.rs`.

- **At `2da6277`** (Campaign `c7226084`): every catalog fixture passed, and the outcome count equals `fixture_names().len()`. The transcript digest equaled Campaign's reference digest `424ae98a1d0a747a4195830626db6fb2589a181fc4875602e652da53d860a890`.
- **From `a0b1b3c`** (Campaign `c5e20cf2`): `chaoscontrol_adapter_passes_every_fixture_with_the_reference_transcript` compares the adapter's transcript digest with the digest of Campaign's own reference pilot, `campaign_conformance::fixtures::animus_shaped::OpaqueCandidateAdapter`, run on the same profile. The digests are equal.
- **From `ec1625e`** (Campaign `260eb9ba`): all 71 fixtures pass, including the new `budgets.declared_dimensions` and `selection.second_level`. The transcript digest equaled the pilot's and Campaign's announced reference digest `94f68aec45deae859800316bd96f399848744b60b124d19f2b535b3cf7548d70`.
- **From `43042fa`** (Campaign `99f8eff3`): all 71 fixtures pass, including the D53 additions (non-selection publications in both gate matrices, forks at depths 1 to 3 that drop the held head, projection-bound, two-parent, replayed, and foreign-root history classes, cost units above `2^53`, and draws above `u32::MAX`). The default-profile transcript digest equaled the pilot's and Campaign's announced reference digest `bdf563a29c60ec71b3f508d5743eaf36d7a562196fa3d47b06e1d515f520e517` (1092 entries), and the reordered digest was `7be1415ddfd115f2a9dacacdddec80e3c20eebd94cedfef6e99246aadf673273`.
- **From `97788af`** (Campaign `a75b5e3a`): all 74 fixtures pass, including the D60 additions `selection.level_counts`, `results.out_of_order_settlement`, and `results.child_counts`, and forks at the deepest depth the presented projection bound allows. The default-profile transcript digest equaled the pilot's and Campaign's announced reference digest `19446c4854e4a1679db4793a7b2dbf65e7da0acb0239c90ca069b99dccd2cb5c`, and the reordered digest was `bf04eddf8845c9cfc38e30cd49099fd1fbe95f9f65065794741b9078ac5f3564`.
- **From `f42381c`** (Campaign `3a182d5a`): all 78 fixtures pass (39 positive, 39 negative). The four new fixtures are `closures.out_of_order` (D65), `selection.middle_level` and `selection.level_order` (D67), and `selection.interior_level` (D80). Existing fixtures now also present forks over the history before H0's head (D64), entropy tickets decoded against their exact draws (D67), the history and cache matrices through held projections of every length and holder kind with rewinds of every depth and forks of every head kind (D76), planner waits and stops projected from ranked and exploratory requests plus a same-batch refusal receipt (D77), explore draws that cross the threshold only in binary64, and two finding-bearing results in one campaign (D80). The default-profile transcript digest equaled the pilot's and Campaign's announced reference digest `77b07d8b1e9462658ce100e28300d300b6523c1f2caa6d35ba51283d02675379`, and the reordered digest was `5807991414f8669bcd4347d0cf1f2a126b3226075bbc205faa64460bdc33b550`.
- **From `905c2eb`** (Campaign `9f1f33a6`): all 79 fixtures pass. The new negative fixture is `receipts.recorded_stop_overclaim`: it records every planner stop reason and presents the ranked plan, exploratory plan, and `StopAccepted` event receipts of each to the claims oracle. The held-projection load and cache matrices now meet every holder kind and length with the honest head, every rewind, forks at depths 0 to 3 and at the deepest admitted depth, the forged and suffix catalog, and post-stop settlements, and present Campaign's new refusals as denial classes: a second campaign root on the same graph (`SharedControlGraph`, D82) and a held lineage shown on an older graph (`StaleGraph`, D84). The default-profile transcript digest equals the pilot's and Campaign's announced reference digest `31bf9eb2bcfc46317eab97d9f6b18143e6e695a3b1444c660f74380a7ba2730f`. The test pins it as a constant together with the fixture count.
- **From `ccc6ff2`** (Campaign `1f8e1a74`): all 79 fixtures pass. The fixture catalog is unchanged; the claims fixtures now present the event receipt of every accepted control event of their session against every later state, running and stopped, plus the plan receipts of every planner stop before and after recording and refusal receipts (D92). The default-profile transcript digest equaled the pilot's and Campaign's announced reference digest `ffebdd683ae698d41f65b4b74670af41837d5e05d48b432eefae9c2bd7bd8ffc`, and the reordered digest was `b94eedaed98ba3373a8f2fdfe07623cedff587012ed0ecaa75e0089277c20e1c`.
- **Since `f45a40f`** (Campaign `65eee6e7`): all 80 fixtures pass. The new negative fixture is `history.state_coverage` (D97): it replays 167 canonical recipes through the adapter, one per state class of a bounded 4-way covering (3 candidates; batches with coalesced duplicates; guidance discovery, improvement, equal, and revision; ranked and exploratory selections; results with and without a child and a finding; closures; consumer and policy retirements; explicit prunes; consumer, host-capacity, policy, and planner stops), and presents loads, caches, and claims at each reached state. The D98 repairs tighten existing fixtures: the gate matrices mutate two evidence fields self-consistently and repeat their gate questions after the host loads the publication, the roles sample includes a result with a child and a finding, guidance revises after a coalesced update, and an entropy ticket's counter must equal the next selection ordinal (`EntropyContext::counter`). The default-profile transcript digest equals the pilot's and Campaign's announced reference digest `cbad4c2a51cfe5514fdf115425496ce3b54eb8d29f5e4f275a40a0ee812e9835`.
- **Profile sensitivity**: `a_changed_profile_changes_the_transcript_digest` runs `exploration_order = [1]` and `retention_rank_order = [1]`. Every fixture passes. The digest differs from the default profile's and equals both the pilot's and Campaign's announced reordered digest `199cef7e172250c274c478658adffe598ab707d9e93e80ef34ee4215c67f0889` (since `f45a40f`), pinned in the test. This strict equality was relaxed at `260eb9ba`, where the suite's `retire_retained` picked by adapter candidate-id order. Campaign fixed that in D53g (it now picks in suite order), and the `99f8eff3` re-pin restored the assertion.

Broken-wrapper negative controls fail exactly their named fixtures. The expectations set at `ec1625e` held unchanged at `99f8eff3`. At `a75b5e3a` only `CrossedIdentities` gained a fixture. At `3a182d5a` every expectation held unchanged. At `9f1f33a6` only `ReceiptOverclaim` gained a fixture, `receipts.recorded_stop_overclaim`. At `1f8e1a74` only `ReceiptOverclaim`'s reported violations change. At `65eee6e7` `StaleHistory`, `HistoryRewrite`, and `ReceiptOverclaim` each also fail `history.state_coverage`, as Campaign's own broken-wrapper tests expect; `ExecutionBeforePublication`, `RoleCollapse`, `CrossedIdentities`, and `BudgetMiscount` pass it and the repaired gate, roles, guidance, and ticket fixtures, and fail exactly their previous fixtures. Every wrapper still fails:

- `ExecutionBeforePublication` fails `fencing.{unpublished_execution, non_durable_execution, stale_branch, foreign_selection, stale_state_selection, gate_matrix_ranked, gate_matrix_exploratory}`; both gate matrices report "a selection executed without a publication or evidence".
- `RoleCollapse` fails `roles.collapsed_identity`.
- `StaleHistory` fails `caches.{stale, moved_binding, validation_matrix}`; the matrix reports "a cache of a stale state was used as current state". Since `f45a40f` it also fails `history.state_coverage` with "a cache of a held state's lineage on an older graph was used as current state": the fixture first presents each held state's lineage on a graph older than its projection's, and an unvalidated cache uses it.
- `HistoryRewrite` fails `history.{rewritten_payload, altered_graph, ambiguous_lineage, load_matrix}`; the matrix reports "a rewound control branch was loaded as current history". Since `f45a40f` it also fails `history.state_coverage` with "a held state's lineage on an older graph was loaded as current": Campaign refuses that load as `StaleGraph`, and the journal fallback returns it.
- `CrossedIdentities` fails `results.crossed_selection`. Since `97788af` it also fails `results.out_of_order_settlement` with `NegativeFixtureFailed(Campaign(InvalidChildReservation))`: the crossed result names the other outstanding selection but carries the executed selection's reserved children, so Campaign refuses it.
- `BudgetMiscount` fails `budgets.overdraw`, and `budgets.reservation` and `budgets.declared_dimensions` report positive-fixture failures.
- `ReceiptOverclaim` fails `receipts.{promotion_overclaim, non_selection_overclaim, planner_overclaim, recorded_stop_overclaim}`; since `ccc6ff2` the non-selection, planner, and recorded-stop fixtures all report "a campaign admission event receipt carried a promotion claim", because each claims session now presents its admission receipt first (before: "an event receipt carried a promotion claim", "a frontier-exhausted stop receipt carried a promotion claim", and "a recorded frontier-exhausted stop receipt carried a promotion claim"). Since `f45a40f` it also fails `history.state_coverage`, first in suite order, with the same admission-receipt violation.

### Adapter, journal, and fence tests

- **Adapter** (`fb9a221`, `campaign_adapter/tests/`):
  - moments and wire round trips, plus tampering;
  - candidate reuse and crossed identities;
  - ranks: grid, ties, `InvalidScore`, `SequenceOutOfRange`, `DoubleDecay`. Campaign exploit plus the explicit revisions reproduces legacy division over 4 rounds;
  - entropy: `k` boundaries, agreement on 20,000 seeded ChaCha8 draws, and missing, crossed, or stale tickets;
  - eligibility: `NotRetained`, `StaleBaseSet`, `StaleEpoch`;
  - results: `DuplicateChild`, `CrossedCampaign`, `UnknownSelection`;
  - stops: `UnmappedStop`, `CrossedIdentity`, wait handling, and the host control capacity stop (`the_host_capacity_stop_maps_to_its_own_product_class`, `an_outstanding_selection_without_capacity_waits_for_its_result`), at the graph bound (`the_graph_event_limit_plans_and_publishes_the_capacity_stop`, `a_graph_without_room_for_the_stop_refuses_planning_with_the_typed_error`, `97788af`), and against Campaign's exposed reserve (`a_round_fits_only_beside_the_exposed_reserve_and_within_the_graph_room`, which replaced `a_round_fits_only_with_room_for_its_settlement_and_the_stop`, and `host_capacity_takes_campaigns_exposed_reserve`, `f42381c`); `SharedControlGraph` and `StaleGraph` are pinned as halting errors, never capacity refusals (`905c2eb`);
  - fence: `advancing_onto_a_graph_older_than_the_held_projection_is_stale` (`905c2eb`);
  - rank shape: `rank_orders_beyond_the_fixed_rank_shape_are_config_errors_for_every_draw` (`97788af`);
  - profile degenerate configurations: `ZeroMaxFrontier`, NaN or negative weight, `MissingRestorableRoot`, `BoundOverflow`.
- **Journal** (`a60cccd`, `explorer/journal/tests.rs`):
  - Fault injection covers every publication phase: `pre_rename_faults_are_not_committed_and_leave_nothing_visible`, `rename_uncertainty_poisons_and_recovery_admits_the_visible_segment`, `directory_sync_failure_poisons_the_journal`, and `sequence_collision_marks_the_writer_stale_until_resynchronized`.
  - Recovery rejects truncation, garbage, a broken chain, gaps, aliases, and non-regular entries.
  - Recovery leaves stage leftovers and unrelated files untouched.
- **Backend** (`3314e8b`, `explorer/frontier_backend/tests.rs`):
  - fence: `the_selection_segment_commits_before_the_round_can_execute`, `every_failed_selection_publication_blocks_the_round`, `uncertain_durability_poisons_the_journal_and_stops_every_later_step`, `a_stale_selection_sequence_replans_once_with_the_same_draws`, `a_foreign_segment_at_the_selection_sequence_is_a_stale_branch`;
  - eligibility: `eligible_parents_restore_their_own_snapshot`, `an_unrestorable_snapshot_after_publication_closes_and_retires`;
  - recovery: `recovery_closes_the_outstanding_selection_and_starts_a_new_epoch`, `an_unknown_record_kind_fails_recovery_without_touching_the_journal`, and `a_genesis_prune_replays_on_recovery` (`3b5cb62`);
  - stops: `product_stops_record_structural_classes_only`, `an_outstanding_selection_is_a_wait_never_exhaustion`, and the capacity stop (`a_tight_projection_bound_ends_the_run_with_the_capacity_stop`, `one_event_short_of_a_whole_round_stops_before_the_first_selection`, `a_retirement_the_host_capacity_refuses_becomes_the_capacity_stop`), and at the graph bound (`a_tight_graph_bound_ends_the_run_with_the_capacity_stop`, `one_graph_event_short_of_a_whole_round_stops_before_the_first_selection`, `a_retirement_the_graph_bound_refuses_becomes_the_capacity_stop`, `a_genesis_without_room_for_its_stop_is_refused_before_the_journal`, `97788af`; the first two also assert Campaign's exposed graph room at the bound since `f42381c`);
  - config admission: `degenerate_configurations_are_rejected_before_the_journal_opens`, `campaign_policy_rejects_degenerate_configs_before_kvm`;
  - tenancy and graph freshness (`905c2eb`): `every_commit_presents_the_current_graph_and_leaves_no_control_draft`, `the_explorers_graph_admits_no_second_campaign_root`;
  - durable base (`ccc6ff2`): `an_uncommitted_segment_leaves_no_draft_and_nothing_publishes_over_it`.
- **Evidence boundary** (`1c06a11`): the `campaign_evidence_boundary` unit and integration tests pass. Invalid fixtures `receipt.{campaign-journal,frontier-trace,choregraph-schema}-promotion.invalid.{json,ncl}` and `replay-verdict.frontier-trace-promotion.invalid.json` are refused.
- **Product-type leakage** (`0bbf309`, `explorer/frontier_backend/leakage_tests.rs`). Both fixtures run the Campaign backend for 2 rounds over distinctive real product values: a `SimulationSnapshot`, a `CoverageBitmap` with a 16-counter pattern, an `OracleReport` with an assertion record and failure details, a `FaultSchedule`, and a finding key. Digests come from the shell's own `frontier_backend::digests`.
  - Positive: `campaign_records_carry_only_opaque_identities_digests_ranks_and_ticks`. It checks every journaled `MOMENT` (5) and `CONTROL` record. Each one decodes and authenticates. Each carries only integers, flags, byte strings or integer arrays of at most 32 bytes, and names of at most 64 identifier characters, with no float and no free text. None contains a product sentinel: the coverage counters, assertion message and details, snapshot tick, fault time and latency, branch ticks, finding key, or the encoded product values. The same journal then recovers.
  - Negative: `product_values_injected_into_campaign_records_are_refused`. Each of the four product values is injected five ways, and each injection is refused:
    - beside a control event: `Decode`;
    - beside a moment event: `Decode`;
    - in place of the moment payload digest: `Decode`;
    - appended to the honest payload digest: `Decode`;
    - cut to digest shape: `IdentityMismatch`. This shape passes the record-shape check, so identity re-derivation is the boundary.
  - Epoch recovery refuses a journal that persisted a leaked control record (`Wire(Decode)`), an appended-digest moment (`Wire(Decode)`), or a digest-shaped moment (`Wire(IdentityMismatch)`).
  - Before `0bbf309`, the appended-digest case decoded `Ok` for all four values: `wire::from_cbor` read the first 32 bytes of a longer byte string as the digest. Decoding now requires the canonical re-encoding.
- **Parity receipt classification** (`0bbf309`). `chaoscontrol.frontier-parity-receipt.v1` is refused for every product evidence class. It is covered by the unit tables, the integration schema list (`schema_version` and `schema`), and two new invalid fixtures, both refused at the boundary by `check-evidence-contracts`:
  - `receipt.frontier-parity-schema-promotion.invalid.{json,ncl}`: a dogfood receipt that declares the parity schema.
  - `receipt.frontier-parity-receipt-promotion.invalid.json`: the committed KVM parity receipt, verbatim.
  - The Nickel negative fails with the contract mirror (`contract broken by the value of schema_version`). It exports cleanly without the mirror entry (checked in a scratch copy).

### Re-pin to Campaign `260eb9ba` (`ec1625e`, `0bbf309`)

API migrations:

| API | Before (`c5e20cf2`) | After (`260eb9ba`) |
| --- | --- | --- |
| `ControlPublicationRequest::previous` | `Option<&ValidatedControlProjection>` | `&ValidatedControlProjection` (D45) |
| `CacheValidationRequest` | no held projection | `previous: Option<&ValidatedControlProjection>` (D45) |
| `ConsumerShell::load_history` | `(&mut self, request)` | `(&mut self, request, previous: Option<&ValidatedControlProjection>)` (D42/D43) |

Gates run on the tree at `0bbf309`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence`: 85 test binaries, 708 passed, 0 failed, 1 ignored (was 706 at `420cb05`). The `chaoscontrol-explore` library has 379 passed and 1 ignored (was 377); the two new tests are the leakage fixtures. The `chaoscontrol-evidence` library has 161 passed, and `tests/campaign_evidence_boundary.rs` has 7.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings` and `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .`, and `check-architecture-boundaries .` exit 0.
- `check-frontier-parity` on the committed fixtures:
  - the valid pair exits 0 and writes a `chaoscontrol.frontier-parity-receipt.v1` receipt with `decision_fields_equal` and `cutover_ready` true;
  - a flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`;
  - an existing receipt exits 2.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, no error-level findings. Only warn-level style findings remain.
- Nix: `nix flake update campaign-src` locked `260eb9ba` from the local Nix Git cache while the forge answered HTTP 530, giving narHash `sha256-H0/tzc+oi+E0zNUNLccicfvZIenaX6TpQ8pDpHwhULU=` and revCount 41. `nix hash path` of the fetched source reproduces that narHash. `nix eval .#packages.x86_64-linux.default.drvPath` evaluates, and `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run.
- `cargo deny` is not in the development shell, so the `dependency-policy` check did not run locally.

### Re-pin to Campaign `99f8eff3` (`43042fa`)

Cargo (`campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency) and the Nix `campaign-src` input moved together from `260eb9ba` to `99f8eff32519b481770c54d95dc980000f7f3141`. `Cargo.lock` changed only the three package source lines.

API and behaviour migrations:

| Change (Campaign decision) | ChaosControl migration |
| --- | --- |
| One guidance rank shape per campaign (D48) | None: the adapter always emits three rank dimensions. |
| `project_event_receipt` binds the event's own profile, ticket, guidance, and cost reservation; every receipt projection requires the current binding (D51) | None: ChaosControl projects no Campaign receipts. |
| `plan_next_from_published_projection` can return `Wait(AwaitingResults)` or `Stop(ConsumerRequested{control_capacity_reason_id()})`; publication can refuse with `AdapterError::{ProjectionCapacityExhausted, GenerationCapacityExhausted}` (D49) | `round.rs` plans through the held projection (new `ControlSession::projection`). `begin_round` checks the whole round against the host capacity (`stops::round_fits_capacity`, `CampaignProfile::round_control_events`) and maps a refused retirement (`stops::is_capacity_refusal`) to the stop. The new class is `ProductStop::ControlCapacityExhausted` / `ExplorationStop::ControlCapacityExhausted`. `history_limits` takes the projection bound separately; the derived bound is unchanged. See `design.md`, "Host control capacity stop". |
| Crossed envelope identities give `Campaign(CrossedIdentity)` on every path (D50) | None: the shell publishes only under its own identities, and the conformance port passes Campaign's answer through. |
| Conformance catalog additions and `retire_retained` in suite order (D53) | New pinned digests; the strict changed-profile equality is restored. Broken-wrapper expectations are unchanged. |

Capacity-stop evidence:

- `a_tight_projection_bound_ends_the_run_with_the_capacity_stop`: with a projection bound of genesis lineage + one round + the stop, round 0 plans, executes, and settles in full. Round 1 stops with `ControlCapacityExhausted` before drawing (the RNG position is unchanged). The stop is recorded as `ConsumerRequested{control_capacity_reason_id()}` with incomplete search acknowledged, serializes as `control_capacity_exhausted`, and the reopened journal replays the stopped epoch with nothing to close.
- `one_event_short_of_a_whole_round_stops_before_the_first_selection`: one event less stops at round 0 with zero selections.
- `a_retirement_the_host_capacity_refuses_becomes_the_capacity_stop`: with room for the genesis and the stop only, the pre-round retirement is refused and not staged, the backend stays live, and the capacity stop records.
- Adapter tests: Campaign's own capacity stop maps to the same class one event below `lineage + 3`, and the selection publication there is refused with `ProjectionCapacityExhausted`. The stop publishes and reloads within that bound. With a selection outstanding, the plan becomes `Wait(AwaitingResults)` one event below the bound and stays `Wait(AwaitingCapacity)` at it.
- Mutation check (reverted): disabling the whole-round check fails `a_tight_projection_bound_ends_the_run_with_the_capacity_stop` ("a round was planned beyond the host capacity"), because Campaign's one-settlement reserve alone lets round 1 select.

Gates run on the tree at `43042fa`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence` (all targets plus doctests): 85 test binaries, 714 passed, 0 failed, 1 ignored (was 708 at `0bbf309`). The `chaoscontrol-explore` library has 385 passed and 1 ignored (was 379); the six new tests are the capacity-stop tests above. The `chaoscontrol-evidence` library has 161 passed.
- Campaign conformance: all 71 fixtures pass; default digest `bdf563a2…e517` and reordered digest `7be1415d…3273` equal the pilot's and Campaign's announced values; all 7 broken wrappers fail their named fixtures.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings` and `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .`, and `check-architecture-boundaries .` exit 0.
- `check-frontier-parity` on the committed fixtures: the valid pair exits 0 with `decision_fields_equal` and `cutover_ready` true; a flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`; an existing receipt exits 2. The VM-free parity corpus (`frontier_backend::parity`, 26 tests) passes with the Campaign backend planning through the held projection.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, `Errors: 0`. Only warn-level style findings remain (1154 and 888).
- `cairn validate` with the installed policy reports only the stale assurance review (`assurance-review is blocked`, `assurance-plan-review` not approved).
- Nix: `nix flake update campaign-src` locked `99f8eff3` from the local Nix Git cache while the forge answered HTTP 530, giving narHash `sha256-7p0NdJMZvrxEg8/1cX95qNg4s8nYlNNqFh7TyhwYKCY=`, revCount 45, and lastModified 1790379434. `nix hash path` of the fetched source reproduces the narHash. The first `nix eval .#packages.x86_64-linux.default.drvPath` failed because the shallow fetch of the locked input could not find `99f8eff3` on the cached `refs/heads/main`; `nix flake prefetch` of the locked URL put the same narHash into the store, after which the evaluation succeeds. `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run on this tree.

### Re-pin to Campaign `a75b5e3a` (`97788af`)

Cargo (`campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency) and the Nix `campaign-src` input moved together from `99f8eff3` to `a75b5e3a829d47c86c9f0a2a5c7d1d42d58b67cd`. `Cargo.lock` changed only the three package source lines. `docs/campaign-source-acquisition.md` names the new revision.

API and behaviour migrations:

| Change (Campaign decision) | ChaosControl migration |
| --- | --- |
| New `campaign_choregraph::AdapterError::GraphCapacityExhausted`. Publication and `plan_next_from_published_projection` also reserve against the graph's `max_events`, and the planner returns a typed capacity error when the planned stop of an open campaign no longer fits (D54) | `stops::round_fits_capacity(&HostCapacity, RoundDemand)` replaces the four-argument form. `HostCapacity::of` reads the held projection's lineage, `graph_events`, `maximum_events`, `graph_max_events`, and admission generation. `CampaignProfile::round_demand` adds `max_child_moments` child moments to the round's control events for the graph. `is_capacity_refusal` includes `GraphCapacityExhausted`. A planner capacity error propagates as `FrontierError::Adapter(Choregraph(..))`, because the stop would be refused too. `history_limits` takes the graph bound through a new test hook (`tighten_graph_bound`); the derived bound is unchanged. |
| `admit_control_root` refuses a root without room for its stop; `derive_projection_binding` refuses empty or over-long branch names (D55) | None beyond the typed refusal: the genesis fails before any journal segment. With the derived bounds the root always fits, and `CONTROL_BRANCH_NAME` is 20 bytes. |
| Idempotent republication after a partial commit instead of `History(NoChange)` (D56) | None: the Explorer cannot republish a draft its graph already holds. Publications are staged on a cloned session and adopted only after the journal segment commits. Recovery replays each committed record once into a fresh graph, and the replan after a sequence collision plans from the committed session. See `design.md`. |
| Host-capacity decisions have no plan receipt (D57, residual) | None: ChaosControl projects no Campaign receipts. |
| Rank orders validated against the fixed rank shape: `InvalidConfig` for every draw, in retention planning, and on replay (D58) | None: the declared orders `[1, 2]` lie inside the fixed three-dimension shape. `rank_orders_beyond_the_fixed_rank_shape_are_config_errors_for_every_draw` pins the typed class on the explore and exploit draws and in `plan_pruning`. |
| Guidance waits require budget fit; budget exhaustion covers every retained candidate (D61) | None: every candidate gets its guidance in the segment that admits it. |
| Reference planner parity, conformance sampling at the bound, direct tests (D59, D60, D62) | New pinned fixture count (74) and digests; `CrossedIdentities` also fails `results.out_of_order_settlement`. |

Graph-bound evidence (the projection bound stays at the derived value in the backend tests and at 512 in the adapter tests, so only the graph binds):

- `a_tight_graph_bound_ends_the_run_with_the_capacity_stop`: the graph bound holds the genesis graph (root moment plus lineage), one whole round with a child moment per branch, and the stop. Round 0 retains both branches and settles in full. Round 1 stops with `ControlCapacityExhausted` before drawing (the RNG position is unchanged), and the stop records as `ConsumerRequested{control_capacity_reason_id()}` with incomplete search acknowledged.
- `one_graph_event_short_of_a_whole_round_stops_before_the_first_selection`: one graph event less stops at round 0 with zero selections, although Campaign's own reserve (selection, result, stop) still fits.
- `a_retirement_the_graph_bound_refuses_becomes_the_capacity_stop`: with graph room for the genesis and the stop only, the pre-round retirement is refused with `GraphCapacityExhausted` and not staged, and the capacity stop records.
- `a_genesis_without_room_for_its_stop_is_refused_before_the_journal`: a graph bound of 2 gives `Adapter(Choregraph(GraphCapacityExhausted))` and a projection bound of 1 gives `ProjectionCapacityExhausted` from `admit_root`; no segment is committed and no session opens.
- Adapter tests: one graph event below the one-entry graph plus 3, Campaign plans its capacity stop, which maps to `ControlCapacityExhausted`. The selection core alone would plan is refused with `GraphCapacityExhausted` (a capacity refusal), and the stop publishes and reloads cold. At the bound, the selection plans and publishes. When a foreign writer takes one event, the stop is still planned. When it fills the graph, `plan_next_from_published_projection` returns `GraphCapacityExhausted` and so does the stop publication.
- Mutation check (reverted): dropping the graph clause from `round_fits_capacity` fails `a_tight_graph_bound_ends_the_run_with_the_capacity_stop` and `one_graph_event_short_of_a_whole_round_stops_before_the_first_selection`, because Campaign's one-settlement reserve alone lets the round select.

Gates run on the tree at `97788af`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets` plus `--doc`: 85 test binaries, 721 passed, 0 failed, 1 ignored (was 714 at `43042fa`). The `chaoscontrol-explore` library has 392 passed and 1 ignored (was 385); the seven new tests are the graph-bound and rank-shape tests above. The `chaoscontrol-evidence` library has 161 passed.
- Campaign conformance: all 74 fixtures pass. The default digest `19446c48…cb5c` and the reordered digest `bf04eddf…3564` equal the pilot's and Campaign's announced values. All 7 broken wrappers fail their named fixtures.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings` and `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .` (21 families), and `check-architecture-boundaries .` (4 pure cores) exit 0.
- `check-frontier-parity` on the committed fixtures: the valid pair exits 0 with `decision_fields_equal` and `cutover_ready` true. A flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`. An existing receipt exits 2. The VM-free parity corpus (`frontier_backend::parity`, 26 tests) passes.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, `Errors: 0`. Only warn-level style findings remain (1157 and 888).
- `cairn validate` with the installed policy reports only the stale assurance review (`assurance-review is blocked`, `assurance-plan-review` not approved).
- Nix: `nix flake update campaign-src` locked `a75b5e3a` from the local Nix Git cache while the forge answered HTTP 530. The lock has narHash `sha256-cfp0fkC5X4533Qf7Ffv0hrz5hOQFqVkWDJakyZC1U5o=`, revCount 50, and lastModified 1790384464. As at `99f8eff3`, the first `nix eval .#packages.x86_64-linux.default.drvPath` failed because the shallow fetch could not find the revision on the cached `refs/heads/main`. `nix flake prefetch` of the locked URL put the same narHash into the store, and `nix hash path` of that store path reproduces it. After the prefetch the evaluation succeeds. `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run on this tree.

### Re-pin to Campaign `3a182d5a` (`f42381c`)

Cargo (`campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency) and the Nix `campaign-src` input moved together from `a75b5e3a` to `3a182d5a980bce045090909f704fc472c024c3a6`, which carries Campaign's seventh- and eighth-audit repairs (D63 to D80). `Cargo.lock` changed only the three package source lines; Choregraph history stays at `b3e08e19`. `docs/campaign-source-acquisition.md` names the new revision.

API and behaviour migrations:

| Change (Campaign decision) | ChaosControl migration |
| --- | --- |
| A validated projection records the reserve its control history still owes (one settlement per outstanding selection, the stop of an open campaign, a pending control draft counted as a selection) and exposes `reserved_control_events()`, `reserved_graph_events()`, and `graph_room()`. Hosts must leave the exposed reserve free and republish a pending draft before any other append (D75) | `stops::HostCapacity` now carries `lineage`, `reserved_control_events`, `maximum_events`, `graph_room`, and `admitted_generation`, read by `HostCapacity::of` from the held projection. `round_fits_capacity` requires `lineage + reserved_control_events + round.control_events` within the projection bound and the branch generation range, and `round.history_events` (the round's control events plus one child moment per branch) within `graph_room`. The stop is part of the exposed reserve, so the shell no longer adds `CLOSING_CONTROL_EVENTS` itself. See `design.md`, "Host control capacity stop". |
| One pending control draft per branch head: `AdapterError::PendingControlDraft { event_id }`, `pending_control_draft(..)` for recovery, idempotent `admit_control_root` (D63) | None: the Explorer never leaves a pending draft. Each publication's graph admission and branch update are staged together on a cloned `ControlSession` and adopted only after the journal segment commits, and recovery replays committed records into a fresh graph. The backend tests assert that the held projection's reserve at round start is the stop alone. |
| Guidance waits need room to select (D69) | None: the Explorer records guidance for every candidate in the segment that admits it and never waits for guidance. |
| Refusal receipts check state identity first (D72); core refuses non-canonical keyed states, duplicate accepted results, and replay output with a repeated key (D79) | None: ChaosControl projects no Campaign receipts, and it builds states only through Campaign's own publication and replay. |
| Sorted lookups and in-loop scans (D71, D79), infra-checker coverage (D66, D78), spec wording (D70, D80) | None. |
| Conformance: forks over earlier history (D64), `closures.out_of_order` (D65), mapping sampling (D67), reference parity (D68, D80), held-projection matrices (D76), exploratory and same-batch claims (D77) | New pinned fixture count (78) and digests. The broken-wrapper expectations hold unchanged. |

Capacity evidence against the exposed reserve:

- `host_capacity_takes_campaigns_exposed_reserve` (adapter, graph limit 48, real Choregraph projections). With nothing outstanding, the reserve is `(control 1, graph 1)`, the stop. With a selection admitted to the graph but its branch update not committed (a pending draft), the reserve is `(3, 2)`, the lineage is unchanged, and the graph room is two events below the idle room. With the selection committed, the reserve is `(2, 2)` and the room equals the pending room. In each state a round demand of exactly the exposed control and graph room fits, and one more graph event or one more control event does not.
- `a_round_fits_only_beside_the_exposed_reserve_and_within_the_graph_room` pins the arithmetic, the generation range, and `BoundOverflow`.
- `a_tight_graph_bound_ends_the_run_with_the_capacity_stop` now asserts that the held projection's `graph_room()` equals `round_demand().history_events` exactly before round 0, with a reserve of one graph event (the stop). The reserve stays free after planning, after the selection publishes, and after settlement, and the recorded stop leaves a reserve of 0. Round 1 stops with `ControlCapacityExhausted`.
- `one_graph_event_short_of_a_whole_round_stops_before_the_first_selection` asserts that the exposed room is exactly one event short of the round demand, and the run stops before round 0.
- Mutation check (reverted): building `HostCapacity` from the raw graph size less the stop, with a fixed control reserve of 1 (the pre-D75 accounting), fails `host_capacity_takes_campaigns_exposed_reserve`. The backend tests still pass, because the Explorer's reserve at round start is the stop alone.

Gates run on the tree at `f42381c`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets` plus `--doc`: 85 test binaries, 722 passed, 0 failed, 1 ignored (was 721 at `97788af`). The `chaoscontrol-explore` library has 393 passed and 1 ignored (was 392; the new test is `host_capacity_takes_campaigns_exposed_reserve`). The `chaoscontrol-evidence` library has 161 passed.
- Campaign conformance: all 78 fixtures pass. The default digest `77b07d8b…5379` and the reordered digest `58079914…b550` equal the pilot's and Campaign's announced values. All 7 broken wrappers fail their named fixtures.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings`, `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check`, and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps -p chaoscontrol-explore -p chaoscontrol-evidence` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .` (21 families), and `check-architecture-boundaries .` (4 pure cores) exit 0.
- `check-frontier-parity` on the committed fixtures: the valid pair exits 0 with `decision_fields_equal` and `cutover_ready` true. A flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`. An existing receipt exits 2. The VM-free parity corpus (`frontier_backend::parity`, 26 tests) passes.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, `Errors: 0`. Only warn-level style findings remain (1156 and 888).
- Cairn (build `f4d97a5`, which loads the vendored policy): `validate --root .` reports only the stale assurance review (`assurance-review is blocked`, `assurance-plan-review` not approved). `gate proposal` and `gate design` pass; `gate tasks` fails on the same stale review. The archive dry run is blocked by the two open tasks (release re-pin and final gate) and the same stale review, and mutates nothing.
- Nix: `nix flake update campaign-src` locked `3a182d5a` from the local Nix Git cache while the forge answered HTTP 530. The lock has narHash `sha256-XsT86curYcZhh8D98inHrx7xgRRlMhUijGPvolWtFJY=`, revCount 63, and lastModified 1790433764. `nix flake prefetch` of the locked URL gives the same hash, and `nix hash path` of the store path reproduces it. `nix eval .#packages.x86_64-linux.default.drvPath` first failed to fetch `campaign-src`, and after the prefetch failed to fetch the Cargo git sources of `choregraph-history` (`b3e08e19`) and `bounded-exec` (`29dac88e`), whose Nix Git caches were empty. Seeding those caches with the locked commits from the local Cargo Git databases let the evaluation succeed (`chaoscontrol-0.1.0.drv`). `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run on this tree.

### Re-pin to Campaign `9f1f33a6` (`905c2eb`)

Cargo (`campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency) and the Nix `campaign-src` input moved together from `3a182d5a` to `9f1f33a65918baaecf06c2b621db5b989b57a0a0`, which carries Campaign's ninth-audit repairs (D82 to D90). `Cargo.lock` changed only the three package source lines; Choregraph history stays at `b3e08e19`. `docs/campaign-source-acquisition.md` names the new revision.

API and behaviour migrations:

| Change (Campaign decision) | ChaosControl migration |
| --- | --- |
| One Campaign campaign per graph and envelope. `admit_control_root` refuses with the new `AdapterError::SharedControlGraph { event_id }` when the graph holds any other Campaign control event under the envelope, and cold load, advance, and publication refuse a graph holding another control root (D82) | None in code. Each epoch's genesis admits the root moment into a fresh `HistoryGraph` and the campaign root onto it, `admit_root` refuses a second admission with `Sequence("campaign root admitted twice")` before Campaign sees it, recovery replays each epoch into a fresh graph, and a resumed run opens a new epoch with its own graph. The adapter identity (the envelope) is seed-independent, so two ChaosControl campaigns could only collide by sharing a graph, which no path does. `the_explorers_graph_admits_no_second_campaign_root` pins the typed refusal on the Explorer's own graph after a settled round. |
| Occupancy reserve for every Campaign control draft that descends from the branch head (chains and racing siblings, each its own event plus a settlement); republication counts the drafts chained below; the planner checks the recorded reserve on every plan (D83) | None in code. The Explorer never leaves a draft: each publication's graph admission and branch update are staged together on a cloned `ControlSession`, adopted only after the journal segment commits, and replayed together on recovery. `HostCapacity::of` still reads `reserved_control_events()` and `graph_room()`, and `host_capacity_takes_campaigns_exposed_reserve` passes unchanged (a pending selection draft still gives `(3, 2)`). The planner comment in `round.rs` and the `HostCapacity` field docs now name D83. |
| A held projection refuses to advance onto a graph with fewer events than it recorded, or one lacking a draft it recorded as pending: `AdapterError::StaleGraph` (D84) | None in code. `ControlSession::seal` always advances over the session's own graph, which only grows (control publications and moment admissions), and a failed stage drops the whole cloned session. |
| Nested keyed vectors (`declared_costs`, `reserved_costs`, `BudgetExhausted` dimensions) must be strictly increasing, else `StaleState` (D90) | None: every ChaosControl candidate declares exactly one cost dimension, and the shell builds states only through Campaign publication and replay. |
| Conformance: `receipts.recorded_stop_overclaim`, the held matrix across every holder kind and length, and the `SharedControlGraph` and `StaleGraph` denial classes (FixConformance9) | New pinned fixture count (79) and digests. `ReceiptOverclaim` also fails `receipts.recorded_stop_overclaim`; the other expectations hold unchanged. |

New typed errors in the stop and refusal mapping: `SharedControlGraph` and `StaleGraph` are not capacity refusals (`is_capacity_refusal` is unchanged). If either ever reached the backend it would propagate as `FrontierError::Adapter(Choregraph(..))` and halt, like every other history error. Neither is reachable from the Explorer, for the reasons in the table; `a_round_fits_only_beside_the_exposed_reserve_and_within_the_graph_room` now asserts that both are not capacity refusals.

Tenancy and freshness evidence:

- `every_commit_presents_the_current_graph_and_leaves_no_control_draft` (backend, two rounds, one retaining a child moment): after genesis, after planning, after the selection segment, and after each settlement, the held projection's `graph_events()` equals the session graph's length, its state equals the session's applied state, its lineage holds every control payload, and `reserved_control_events()` and `reserved_graph_events()` both equal the outstanding selections plus the stop. With any draft they would be larger.
- `the_explorers_graph_admits_no_second_campaign_root`: after a settled round, a second `admit_root` is refused with `Sequence`. Another seed's admission on the Explorer's graph gets `SharedControlGraph` naming the first event of the Explorer's lineage in identity order, and a retry of the Explorer's own root gets `SharedControlGraph` naming the first of its later lineage events.
- `advancing_onto_a_graph_older_than_the_held_projection_is_stale` (adapter): after another writer's event grows the graph, `advance_projection` onto the older graph is `StaleGraph`; a cold load of the older graph still validates.
- Mutation check (reverted): making `ControlSession::seal` keep the previous projection fails both backend tests (graph length 5 against held `graph_events` 2).

Gates run on the tree at `905c2eb`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets` plus `--doc`: 85 test binaries, 725 passed, 0 failed, 1 ignored (was 722 at `f42381c`). The `chaoscontrol-explore` library has 396 passed and 1 ignored (was 393; the three new tests are above). The `chaoscontrol-evidence` library has 161 passed.
- Campaign conformance (`cargo test --offline --locked -p chaoscontrol-explore --lib campaign_adapter::tests::conformance`, 7 passed): all 79 fixtures pass. The default digest `31bf9eb2…730f` and the reordered digest `b3273e4f…b16b6c` equal the pilot's and Campaign's announced values. All 7 broken wrappers fail their named fixtures.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings`, `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check`, and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps -p chaoscontrol-explore -p chaoscontrol-evidence` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .` (21 families), and `check-architecture-boundaries .` (4 pure cores) exit 0.
- `check-frontier-parity` on the committed fixtures: the valid pair exits 0 with `decision_fields_equal` and `cutover_ready` true. A flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`. An existing receipt exits 2. The VM-free parity corpus (`frontier_backend::parity`, 26 tests) passes.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, `Errors: 0`. Only warn-level style findings remain (1156 and 888).
- Cairn (build `f4d97a5`, which loads the vendored policy): `validate --root .` reports only the stale assurance review (`assurance-review is blocked`, `assurance-plan-review` not approved). `gate proposal` and `gate design` pass; `gate tasks` fails on the same stale review. The archive dry run is blocked by the two open tasks, 41 (release re-pin) and 42 (final gate), and the same stale review, and mutates nothing.
- Nix: `nix flake update campaign-src` locked `9f1f33a6` from the local Nix Git cache while the forge answered HTTP 530 (warning: using expired cached ref `refs/heads/main`). The lock has narHash `sha256-lJTLYYcF4OrAyctuBQSqQjuI44ijyddM/ndHTvDm0t4=`, revCount 71, and lastModified 1790443239. `nix flake prefetch` of the locked URL gives the same hash, and `nix hash path` of the store path reproduces it. `nix eval .#packages.x86_64-linux.default.drvPath` succeeds without further cache seeding (`chaoscontrol-0.1.0.drv`), and `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run on this tree.

### Re-pin to Campaign `1f8e1a74` (`ccc6ff2`)

Cargo (`campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency) and the Nix `campaign-src` input moved together from `9f1f33a6` to `1f8e1a74b7506900a01bbe68ecea5a5579f6c3db`, which carries Campaign's tenth-audit repairs (D91 to D96). `Cargo.lock` changed only the three package source lines; Choregraph history stays at `b3e08e19`. `docs/campaign-source-acquisition.md` names the new revision.

API and behaviour migrations:

| Change (Campaign decision) | ChaosControl migration |
| --- | --- |
| No-pipelining host obligation, `r[campaign.publication_fence.durable_base]` (D91): a host plans and publishes a control event only through a projection validated at a durably committed branch value, never through the branch value a publication plans before its compare-and-set is durable. Recovery guarantees cover a partial commit and racing siblings, not a pipelined chain; a chained non-selection draft published from the durable head is refused with `StaleState`, and republishing a pipelined chain can be refused at a bound | None in code; the session module doc now names the obligation. The Explorer's graph and branch live only in its `ControlSession`, and `ControlSession::publish` (`session.rs`) sets `self.graph = publication.graph_after_admission()` and `self.branch = publication.branch_plan().after` in the same call that plans the draft, so its graph never holds a draft whose branch update is pending, and no draft is ever chained below another. Publications stage on a clone of the committed session (`publish` in `campaign.rs`); `commit` in `campaign/round.rs` seals the clone, appends the journal segment, and adopts the clone only after the segment is durable, so a segment's drafts and branch moves become durable together or not at all. A segment that fails to commit sets `halted` (unknown durability also poisons the journal), and `ensure_live` then refuses every later plan and publication; a selection-sequence collision replans from the committed session. Recovery (`close_recovered_epoch`) publishes only on the session replayed from committed segments. `plan_control_event_publication` always receives `previous` = the projection sealed at the last committed segment. |
| Admission currency (D94): `assert_state_identity_current` also re-derives and validates the state's admission, so a caller-built state with an edited admission is `StaleState` | None. No Explorer path builds a `FrontierState` by hand: the session takes its first state from `load_projection(..).state()` (Campaign reconstruction) in `ControlSession::genesis`, advances it only with `campaign_core::apply_control_event` in `ControlSession::publish`, and `ControlSession::seal` requires it to equal the state of the projection Campaign re-validates; recovery replays through the same session. Outside the Explorer, only the conformance adapter and the adapter tests hold states, built by `campaign_core::reconstruct`. |
| Claims fixtures present every receipt of their session in every later state, running and stopped (D92) | New pinned digests. The adapter's shell refuses every claim on each receipt, so every fixture passes. `ReceiptOverclaim` now fails `receipts.{non_selection_overclaim, planner_overclaim, recorded_stop_overclaim}` at the first receipt presented, "a campaign admission event receipt carried a promotion claim"; it still fails exactly its four fixtures, and every other wrapper's expectations hold unchanged. |
| Infra checker: exact `cfg(test)` rule, library-root inner cfg, and pinned consumer tests must call `run_suite` (D93); spec and doc wording (D95, D96) | None. Both pinned conformance tests call `run_suite` directly. |

Durable-base evidence:

- `an_uncommitted_segment_leaves_no_draft_and_nothing_publishes_over_it` (backend): after the selection segment commits, a settlement segment whose payload write fails (it stages a child moment, then the result, candidate, and guidance publications, each on the branch value the one before it planned) returns `FrontierError::Journal` without poisoning. The committed session keeps its graph length, lineage, and state, its reserve is only the outstanding settlement and the stop (`assert_current_graph_without_drafts`), and `begin_round` and `record_stop` return `Halted`.
- Mutation check (reverted): adopting the staged session before the journal append in `commit` fails the test (graph length 11 against the committed 6).
- `every_commit_presents_the_current_graph_and_leaves_no_control_draft` and `every_failed_selection_publication_blocks_the_round` still pass.

Gates run on the tree at `ccc6ff2`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets` plus `--doc`: 85 test binaries, 726 passed, 0 failed, 1 ignored (was 725 at `905c2eb`). The `chaoscontrol-explore` library has 397 passed and 1 ignored (was 396; the new test is above). The `chaoscontrol-evidence` library has 161 passed.
- Campaign conformance (`cargo test --offline --locked -p chaoscontrol-explore --lib campaign_adapter::tests::conformance`, 7 passed): all 79 fixtures pass. The default digest `ffebdd683ae698d41f65b4b74670af41837d5e05d48b432eefae9c2bd7bd8ffc` and the reordered digest `b94eedaed98ba3373a8f2fdfe07623cedff587012ed0ecaa75e0089277c20e1c` equal the pilot's and Campaign's announced values. All 7 broken wrappers fail their named fixtures.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings`, `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check`, and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps -p chaoscontrol-explore -p chaoscontrol-evidence` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .` (21 families), and `check-architecture-boundaries .` (4 pure cores) exit 0.
- `check-frontier-parity` on the committed fixtures: the valid pair exits 0 with `decision_fields_equal` and `cutover_ready` true. A flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`. An existing receipt exits 2. The VM-free parity corpus (`frontier_backend::parity`, 26 tests) passes.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, `Errors: 0`. Only warn-level style findings remain (1156 and 888).
- Cairn (build `f4d97a5`, which loads the vendored policy): `validate --root .` reports only the stale assurance review (`assurance-review is blocked`, `assurance-plan-review` not approved). `gate proposal` and `gate design` pass; `gate tasks` fails on the same stale review. Tasks 41 (release re-pin) and 42 (final gate) stay open.
- Nix: `nix flake update campaign-src` locked `1f8e1a74` from the local Nix Git cache while the forge answered HTTP 530 (warning: using expired cached ref `refs/heads/main`). The lock has narHash `sha256-ANOdUOoVnjCautRQ71nFaHTWlwTjXyTGexXhmdt+LXA=`, revCount 78, and lastModified 1790447989. `nix flake prefetch` of the locked URL gives the same hash (store path `pxck49gzqyy2j0mpbnv0h3lvxzirgji3-source`). `nix eval .#packages.x86_64-linux.default.drvPath` succeeds (`chaoscontrol-0.1.0.drv`), and `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run on this tree.

### Re-pin to Campaign `65eee6e7` (`f45a40f`)

Cargo (`campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency) and the Nix `campaign-src` input moved together from `1f8e1a74` to `65eee6e70a7151cad3032dde0f242f57bb03938b`, the Campaign revision under final review. Since `1f8e1a74` Campaign changed only `crates/campaign-conformance` (plus its documents and provenance); `campaign-core` and `campaign-choregraph` are byte-identical. `Cargo.lock` changed only the three package source lines; Choregraph history stays at `b3e08e19`. `docs/campaign-source-acquisition.md` names the new revision.

API and behaviour migrations:

| Change (Campaign decision) | ChaosControl migration |
| --- | --- |
| New negative fixture `history.state_coverage` (D97, 80 fixtures): 167 canonical recipes, one per state class of a bounded 4-way covering, replayed through the adapter (3 candidates; batches with coalesced duplicates; guidance discovery, improvement, equal, and revision; ranked and exploratory selections; results with and without a child and a finding; closures; consumer and policy retirements; explicit prunes; consumer, host-capacity, policy, and planner stops), with loads, caches, and claims presented at each reached state | None in code. The adapter passes the fixture on both profiles: its loads and caches refuse every shape Campaign refuses (including a held lineage on an older graph, `StaleGraph`), and its shell refuses every claim on every receipt. New pinned fixture count and digests. `StaleHistory`, `HistoryRewrite`, and `ReceiptOverclaim` each also fail the new fixture (see the wrapper list above). |
| Gate matrices mutate two evidence fields self-consistently and repeat gate questions without a publication or evidence after the host loads the publication (D98) | None. `execution_authority` grants only on a durable publication of the exact selection; `ExecutionBeforePublication` still fails exactly its seven fence fixtures. |
| Roles sample a result with a child and a finding (D98) | None. `role_identities` keeps candidate, work item, and result identities distinct; `RoleCollapse` still fails only `roles.collapsed_identity`. |
| Guidance revision after a coalesced duplicate update (D98) | None. |
| An entropy ticket's counter must equal the next selection ordinal, `EntropyContext::counter` (D98) | None. The conformance adapter derives the ticket from `EntropyContext::counter`, and the Explorer's `selection_request` uses `ticket_counter(state)`, `selection_count + 1`, for both the ticket and the request identity. |

Gates run on the tree at `f45a40f`, offline, with the committed `Cargo.lock`:

- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets` plus `--doc`: 85 test binaries, 726 passed, 0 failed, 1 ignored (unchanged from `ccc6ff2`). The `chaoscontrol-explore` library has 397 passed and 1 ignored. The `chaoscontrol-evidence` library has 161 passed.
- Campaign conformance (`cargo test --offline --locked -p chaoscontrol-explore --lib campaign_adapter::tests::conformance`, 7 passed, 38.0 s test time in a debug build; the healthy-adapter test alone takes 22.0 s): all 80 fixtures pass. The default digest `cbad4c2a51cfe5514fdf115425496ce3b54eb8d29f5e4f275a40a0ee812e9835` and the reordered digest `199cef7e172250c274c478658adffe598ab707d9e93e80ef34ee4215c67f0889` equal the pilot's and Campaign's announced values. All 7 broken wrappers fail their named fixtures.
- `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings`, `cargo fmt -p chaoscontrol-explore -p chaoscontrol-evidence --check`, and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps -p chaoscontrol-explore -p chaoscontrol-evidence` pass.
- `check-evidence-contracts --root .`, `check-contract-registry .` (21 families), and `check-architecture-boundaries .` (4 pure cores) exit 0.
- `check-frontier-parity` on the committed fixtures: the valid pair exits 0 with `decision_fields_equal` and `cutover_ready` true. A flipped selection, prune, or stop exits 1 with `[SelectedEntry, ParentMoment]`, `[Pruned]`, and `[StopClass]`. An existing receipt exits 2. The VM-free parity corpus (`frontier_backend::parity`, 26 tests) passes.
- Focused Octet, `cargo-octet check -p chaoscontrol-explore -- --lib` and `-p chaoscontrol-evidence -- --lib`: exit 0, `Errors: 0`. Only warn-level style findings remain (1156 and 888).
- Cairn (build `f4d97a5`, which loads the vendored policy): `validate --root .` reports only the stale assurance review (`assurance-review is blocked`, `assurance-plan-review` not approved). `gate proposal` and `gate design` pass; `gate tasks` fails on the same stale review. Tasks 41 (release re-pin) and 42 (final gate) stay open.
- Nix: `nix flake update campaign-src` locked `65eee6e7` from the local Nix Git cache while the forge answered HTTP 530 (warning: using expired cached ref `refs/heads/main`). The lock has narHash `sha256-aLtsvKbWsr0d4+RId/wODF8mWQce7s0j/RJ5pNazV9w=`, revCount 85, and lastModified 1790458477. `nix flake prefetch` of the locked URL gives the same hash (store path `yjs5nk2zkplhnflaqd35w7xnrjnkz8h9-source`). The first `nix eval .#packages.x86_64-linux.default.drvPath` failed to fetch the Campaign repository over the 530; after the prefetch it succeeds, repeatably (`imfyw72hj6r7gnxhnzkd3g53h078baa6-chaoscontrol-0.1.0.drv`), and `nix-instantiate --parse flake.nix` passes. The full `nix flake check -L` did not run on this tree.

### Model parity corpus

The VM-free corpus lives in `explorer/frontier_backend/parity/` (commits `b6c9194`, `3b5cb62`, `c6c81cc`). It drives both backends through the Explorer round protocol with one script and one counted RNG, and compares every decision field after every round:

- root retained;
- round count, mode, and outcome;
- draws, selected entry, explore flag, and parent moment;
- admitted, pruned, and retained set;
- selection counts, retained scores, and selection use;
- RNG position, stop class, rounds completed, and backend failure.

All 16 positive cases report zero decision drift:

| Case | Rounds | Covers |
| --- | --- | --- |
| `near_ties` | 6 | Adjacent floating-point scores |
| `equal_scores` | 6 | Equal scores; admit and prune in one round |
| `effective_score_ties` | 4 | Decayed and undecayed ties |
| `subnormals` | 5 | Underflow to zero after one selection |
| `extremes` | 4 | `f64::MAX` and pruning of zero |
| `signed_zeros` | 4 | `+0` and `-0` |
| `prune_equal_lowest` | 9 | `max_frontier` 1, 2, and 3 |
| `pushed_then_pruned` | 2 | Child admitted and pruned in one round |
| `selected_at_retention_boundary` | 2 | Explore selects the lowest entry |
| `root_beyond_max_frontier` | 2 | Genesis prune regression (`3b5cb62`) |
| `marker_duplicates` | 4 | Identical marker metadata |
| `hybrid` | 12 | Fault-schedule and input-tree rounds |
| `explore_boundary_k` | 5 | `k` = 900719925474099 and 900719925474100, 0, `2^53 - 1`, and a rejected `gen_range` word |
| `long_run_decay` | 200 | 200 decay revisions; fields equal while measured work differs |
| `seeded_mixed` | 30 | Seed 7: ties, zeros, explore draws, and pruning |
| `stop_*` (5 cases) | 24 | Plateau, plateau on the last round, short-run finding, signal, completed |

`max_rounds_zero` (0 rounds) also agrees: both stop `Completed` with zero draws.

Negative cases block cutover readiness:

- `frontier_exhaustion_from_an_empty_root_blocks_cutover` reports exactly `[RoundCount, RoundsCompleted]`.
- `degenerate_configurations_are_rejected_and_block_cutover` reports `BackendFailure`.
- `any_single_field_drift_blocks_cutover` covers 10 fields.
- `committed_fixtures_with_a_flipped_decision_block_cutover`.
- `work_and_field_equality_are_separate_facts` keeps measured work ungated.

`check-frontier-parity` was run on the committed fixtures in `crates/chaoscontrol-explore/tests/fixtures/frontier-parity/`:

- The valid pair exits 0.
- A flipped selection exits 1 with `[SelectedEntry, ParentMoment]`.
- A flipped prune exits 1 with `[Pruned]`.
- A flipped stop exits 1 with `[StopClass]`.
- Swapped roles, an existing receipt, and a missing flag each exit 2.

### Adaptive KVM smoke

`kvm-smoke-2026-09-25.md` (commit `d0719c5`) records one bounded rust-workload exploration per policy. The runs used source `c6c81cc` with bounds `--vms 1 --rounds 5 --branches 4 --ticks 500 --seed 42 --mode fault-schedule --memory-mb 128`.

- Both runs exited 0 and stopped `completed`.
- The Campaign journal has 12 chained segments. Every round's selection segment precedes that round's settlement segment and its first logged effect. The stop is recorded as `StopAccepted{SelectionLimitReached}`.
- The parity receipt is `evidence/kvm-smoke-2026-09-25-frontier-parity-receipt.json`, BLAKE3 `128e6dab1a8a1cf9b7cbeb61e6daea6c28f30496b9438408cc22e48b80e203eb`. Both replay directions compared 5 rounds on 17 decision fields with 0 drifts. The receipt records `decision_fields_equal: true` and `cutover_ready: true`.
- Flipping round 3's selection in a copy of the live Campaign trace exits 1 with 3 drifts.
- The traces use the pre-cutover policy names `legacy` and `campaign-candidate`. Commit `420cb05` renamed them, and the committed receipt is historical.

### Cutover and diagnostic rollback

Commits `be95656` and `420cb05` add these tests, all passing in the `420cb05` run above:

- **Campaign is the default**: `run_and_campaign_default_to_campaign_without_a_flag`, `default_config_selects_the_campaign_backend`, `old_checkpoint_without_the_policy_field_loads_as_campaign`, and `resume_never_selects_legacy_diagnostic_implicitly`.
- **Diagnostic runs are bounded**: `legacy_diagnostic_without_output_dir_is_rejected_before_kvm`, `only_legacy_diagnostic_runs_write_the_release_ineligible_receipt`, `campaign_resume_of_a_diagnostic_checkpoint_stays_release_ineligible`, and `legacy_diagnostic_checkpoint_claiming_release_eligibility_is_rejected`.
- **Gates refuse diagnostic output**: `legacy_diagnostic_policy_and_ineligibility_are_refused_at_every_gate`, `output_directory_refuses_each_diagnostic_mark`, `accepted_dogfood_config_refuses_legacy_diagnostic_outputs`, and `materialize_dogfood_receipt_refuses_legacy_diagnostic_outputs` (runs the real binary). Also `matrix_rows_cannot_select_the_legacy_diagnostic_policy` and `legacy_diagnostic_row_output_blocks_release`.
- **Fixtures**: the new invalid run-config, run-profile, receipt, and KVM-matrix fixtures pass `check-evidence-contracts` and `check-kvm-release-matrix`.

The cutover slice also ran a CLI smoke of the diagnostic policy:

- A retired spelling is rejected, and the error names the replacement.
- A `legacy-diagnostic` run without `--output` exits before VM work.
- Resuming a diagnostic checkpoint without an explicit policy is refused.
- `materialize-dogfood-receipt` refuses a diagnostic checkpoint.

### Task evidence map

| Task | Evidence |
| --- | --- |
| `history` | `fb9a221` moments and wire tests; `3314e8b` recovery replay; `905c2eb` one campaign per graph, the current graph, and no control drafts; `ccc6ff2` states only from reconstruction and `apply_control_event` (D94); `f45a40f` `history.state_coverage` (D97) |
| `adapter` | `fb9a221` candidates, identity, and profile tests; `2da6277` conformance |
| `ranks` | `fb9a221` rank tests; `3314e8b` decay revisions; `near_ties` and `effective_score_ties` corpus cases |
| `entropy` | `fb9a221` entropy tests; `b6c9194` `the_ticket_decides_campaigns_explore_branch` and `explore_boundary_k` |
| `snapshot_eligibility` | `fb9a221` eligibility tests; `3314e8b` backend eligibility tests |
| `adapter.invalid` | `fb9a221` crossed, duplicate, malformed-rank, overflow, and ineligible cases; `2da6277` conformance wrappers; `b6c9194` degenerate configs |
| `frontier_parity` (mapping) | `3314e8b` settlement with decay and prune; `3b5cb62` genesis prune |
| `publication_fence` | `a60cccd` journal; `3314e8b` fence tests; `d0719c5` live segment order |
| `effects` | `3314e8b` `AuthorizedRound` is the only source of the parent snapshot; `d0719c5` live run |
| `observations` | `fb9a221` results tests; `3314e8b` settlement |
| `stop` | `fb9a221` stops; `a8d2716` product stop classes; `3314e8b` `record_stop`; `b6c9194` `every_reachable_stop_class_agrees`; `43042fa` host control capacity stop; `97788af` graph-bound round reserve; `f42381c` round capacity from Campaign's exposed reserve; `905c2eb` new history errors are not capacity refusals |
| `publication_fence.invalid` | `fb9a221` fence tests; `2da6277` `ExecutionBeforePublication`; `a60cccd` durability classes; `3314e8b` failed, stale, and foreign publication tests; `ccc6ff2` no publication over an uncommitted segment (D91) |
| `product_authority` | `3314e8b` product-computed proposals; `b6c9194` corpus (only ranks cross) |
| `progress` | `31e6a40`, `1ba05d1`, and `420cb05` checkpoint, progress, and report tests |
| `evidence` | `1c06a11` boundary; `2da6277` `ReceiptOverclaim`; `0bbf309` parity receipt classification |
| `evidence.invalid` | receipt promotion: `2da6277` `ReceiptOverclaim`, `1c06a11` promotion fixtures; stale cache: `StaleHistory`; missing snapshot: `3314e8b` `an_unrestorable_snapshot_after_publication_closes_and_retires`; false observation: `CrossedIdentities`; product-type leakage and parity receipt promotion: `0bbf309` |
| `frontier_parity` (comparison) | `3b5cb62`, `b6c9194`, and `c6c81cc` corpus and checker |
| `kvm_parity` | `d0719c5` smoke record and receipt |
| `cutover` | `420cb05` default selection after the conformance, parity, and KVM evidence above |
| `rollback` | `be95656` gates; `420cb05` diagnostic policy |

### Cairn

`cairn validate` runs with the explicit installed policy, `--policy /home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`. The vendored ChaosControl policy predates the installed Cairn CLI and does not parse.

## Current gaps

- **Final Campaign release pin.** Cargo and the Nix `campaign-src` input both pin Campaign development revision `65eee6e7` (`f45a40f`), the revision under Campaign's round-12 review. It is not the release revision. The forge answered HTTP 530 at the re-pin, so fresh acquisition from the live endpoint has not been observed for it.
- **Full gate.** `nix flake check -L` has not passed for this change.
- **Capacity stop reachability.** The host control capacity stop is unreachable with the derived bounds. The tests reach it through test-only tighter projection and graph bounds; no product configuration sets either.
- **KVM smoke coverage.** The smoke did not exercise the following:
  - explore draws or pruning (the model corpus covers both);
  - input-tree or hybrid modes, `--workers` > 1, resume, or journal recovery;
  - fault injection under KVM.
- **Live observations.** Live coverage and child-moment equality between the policies is not claimed, because repeated KVM runs differ even under one policy. `kvm_parity.valid` is met by replaying each run's recorded observations through the other policy.

## Non-claims

Passing parity proves only the selected corpus and profiles. It does not prove exhaustive search, all entropy streams, snapshot durability, KVM correctness, finding truth, or release readiness.
