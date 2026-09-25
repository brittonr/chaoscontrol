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

Every commit below is on `cairn/adopt-campaign-core-20260906`. The implementation head is `0bbf309` (Campaign `260eb9ba` re-pin `ec1625e`, evidence fixtures `0bbf309`); the counts in this subsection are from `420cb05` unless the re-pin subsection below says otherwise. Command prefixes for the development shell and target directory are omitted. All cargo commands ran offline against the committed `Cargo.lock`.

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
- **Since `ec1625e`** (Campaign `260eb9ba`): all 71 fixtures pass, including the new `budgets.declared_dimensions` and `selection.second_level`. The transcript digest equals the pilot's and Campaign's announced reference digest `94f68aec45deae859800316bd96f399848744b60b124d19f2b535b3cf7548d70`, which the test now pins as a constant together with the fixture count.
- **Profile sensitivity**: `a_changed_profile_changes_the_transcript_digest` shows that another exploration and retention order still conforms and yields a different digest. At `260eb9ba` it temporarily omits equality with the pilot under that profile: the suite's `receipts.non_selection_overclaim` retires the first retained candidate in adapter candidate-id order, so the two adapters retire different operations at transcript entry 820. Campaign owns the fix; the next re-pin restores the assertion.

Broken-wrapper negative controls fail exactly their named fixtures (expectations as of `ec1625e`):

- `ExecutionBeforePublication` fails `fencing.{unpublished_execution, non_durable_execution, stale_branch, foreign_selection, stale_state_selection, gate_matrix_ranked, gate_matrix_exploratory}`; both gate matrices report "a selection executed without a publication or evidence".
- `RoleCollapse` fails `roles.collapsed_identity`.
- `StaleHistory` fails `caches.{stale, moved_binding, validation_matrix}`; the matrix reports "a cache of a stale state was used as current state".
- `HistoryRewrite` fails `history.{rewritten_payload, altered_graph, ambiguous_lineage, load_matrix}`; the matrix reports "a rewound control branch was loaded as current history".
- `CrossedIdentities` fails `results.crossed_selection`.
- `BudgetMiscount` fails `budgets.overdraw`, and `budgets.reservation` and `budgets.declared_dimensions` report positive-fixture failures.
- `ReceiptOverclaim` fails `receipts.{promotion_overclaim, non_selection_overclaim, planner_overclaim}`; the planner fixture reports "a frontier-exhausted stop receipt carried a promotion claim".

### Adapter, journal, and fence tests

- **Adapter** (`fb9a221`, `campaign_adapter/tests/`):
  - moments and wire round trips, plus tampering;
  - candidate reuse and crossed identities;
  - ranks: grid, ties, `InvalidScore`, `SequenceOutOfRange`, `DoubleDecay`. Campaign exploit plus the explicit revisions reproduces legacy division over 4 rounds;
  - entropy: `k` boundaries, agreement on 20,000 seeded ChaCha8 draws, and missing, crossed, or stale tickets;
  - eligibility: `NotRetained`, `StaleBaseSet`, `StaleEpoch`;
  - results: `DuplicateChild`, `CrossedCampaign`, `UnknownSelection`;
  - stops: `UnmappedStop`, `CrossedIdentity`, and wait handling;
  - profile degenerate configurations: `ZeroMaxFrontier`, NaN or negative weight, `MissingRestorableRoot`, `BoundOverflow`.
- **Journal** (`a60cccd`, `explorer/journal/tests.rs`):
  - Fault injection covers every publication phase: `pre_rename_faults_are_not_committed_and_leave_nothing_visible`, `rename_uncertainty_poisons_and_recovery_admits_the_visible_segment`, `directory_sync_failure_poisons_the_journal`, and `sequence_collision_marks_the_writer_stale_until_resynchronized`.
  - Recovery rejects truncation, garbage, a broken chain, gaps, aliases, and non-regular entries.
  - Recovery leaves stage leftovers and unrelated files untouched.
- **Backend** (`3314e8b`, `explorer/frontier_backend/tests.rs`):
  - fence: `the_selection_segment_commits_before_the_round_can_execute`, `every_failed_selection_publication_blocks_the_round`, `uncertain_durability_poisons_the_journal_and_stops_every_later_step`, `a_stale_selection_sequence_replans_once_with_the_same_draws`, `a_foreign_segment_at_the_selection_sequence_is_a_stale_branch`;
  - eligibility: `eligible_parents_restore_their_own_snapshot`, `an_unrestorable_snapshot_after_publication_closes_and_retires`;
  - recovery: `recovery_closes_the_outstanding_selection_and_starts_a_new_epoch`, `an_unknown_record_kind_fails_recovery_without_touching_the_journal`, and `a_genesis_prune_replays_on_recovery` (`3b5cb62`);
  - stops: `product_stops_record_structural_classes_only`, `an_outstanding_selection_is_a_wait_never_exhaustion`;
  - config admission: `degenerate_configurations_are_rejected_before_the_journal_opens`, `campaign_policy_rejects_degenerate_configs_before_kvm`.
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
| `history` | `fb9a221` moments and wire tests; `3314e8b` recovery replay |
| `adapter` | `fb9a221` candidates, identity, and profile tests; `2da6277` conformance |
| `ranks` | `fb9a221` rank tests; `3314e8b` decay revisions; `near_ties` and `effective_score_ties` corpus cases |
| `entropy` | `fb9a221` entropy tests; `b6c9194` `the_ticket_decides_campaigns_explore_branch` and `explore_boundary_k` |
| `snapshot_eligibility` | `fb9a221` eligibility tests; `3314e8b` backend eligibility tests |
| `adapter.invalid` | `fb9a221` crossed, duplicate, malformed-rank, overflow, and ineligible cases; `2da6277` conformance wrappers; `b6c9194` degenerate configs |
| `frontier_parity` (mapping) | `3314e8b` settlement with decay and prune; `3b5cb62` genesis prune |
| `publication_fence` | `a60cccd` journal; `3314e8b` fence tests; `d0719c5` live segment order |
| `effects` | `3314e8b` `AuthorizedRound` is the only source of the parent snapshot; `d0719c5` live run |
| `observations` | `fb9a221` results tests; `3314e8b` settlement |
| `stop` | `fb9a221` stops; `a8d2716` product stop classes; `3314e8b` `record_stop`; `b6c9194` `every_reachable_stop_class_agrees` |
| `publication_fence.invalid` | `fb9a221` fence tests; `2da6277` `ExecutionBeforePublication`; `a60cccd` durability classes; `3314e8b` failed, stale, and foreign publication tests |
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

- **Final Campaign release pin.** Cargo and the Nix `campaign-src` input both pin Campaign development revision `260eb9ba` (`ec1625e`). It is not the release revision. The forge answered HTTP 530 at the re-pin, so fresh acquisition from the live endpoint has not been observed for it.
- **Full gate.** `nix flake check -L` has not run for this change.
- **Changed-profile pilot equality.** At `260eb9ba`, `a_changed_profile_changes_the_transcript_digest` temporarily omits equality with Campaign's pilot. This is a Campaign conformance defect: `retire_retained` picks by adapter candidate-id order. The next re-pin restores the assertion.
- **KVM smoke coverage.** The smoke did not exercise the following:
  - explore draws or pruning (the model corpus covers both);
  - input-tree or hybrid modes, `--workers` > 1, resume, or journal recovery;
  - fault injection under KVM.
- **Live observations.** Live coverage and child-moment equality between the policies is not claimed, because repeated KVM runs differ even under one policy. `kvm_parity.valid` is met by replaying each run's recorded observations through the other policy.

## Non-claims

Passing parity proves only the selected corpus and profiles. It does not prove exhaustive search, all entropy streams, snapshot durability, KVM correctness, finding truth, or release readiness.
