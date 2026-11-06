# Fault-mechanism fixture audit

Date: 2026-11-06.

Existing and new fixtures are mapped below. Snapshot/replay work also adds
active-state drift validation, pause-application suppression, stable throttle
overlap rejection, and exact replay of retained pause admission/control facts.

## Direction, modes, heal, bounds, throttle

| Case | Fixture |
| --- | --- |
| canonical link identity, self-link rejection | `link_identity_is_canonical_and_self_links_fail` |
| both directions apply | `both_direction_faults_apply_to_each_direction` |
| per-mode parameters | `directional_modes_require_their_parameters` |
| scoped heal | `scoped_heal_clears_only_the_selected_link_pair` and `both_direction_and_clear_heal` in `chaoscontrol-sim-core::network` |
| queue bounds | `directional_modes_require_their_parameters` (0, max, max+1) |
| throttle clamp, exhaustion, release | `cpu_throttle_clamps_budget_and_releases_at_tick` |
| host-style throttle rejection | `ThrottleTarget` admits only `Vm` or named `Process`; `malformed_pause_and_throttle_identities_are_rejected` rejects an unnamed process target, and no host target is representable |

## Ambient, adapters, pause bounds

| Case | Fixture |
| --- | --- |
| ambient command strings | `custom_effects_reject_ambient_command_strings` |
| unadmitted adapter | `custom_effect_without_adapter_is_rejected` and invalid-parameter rejection |
| malformed pause window | `pause_control_enforces_the_window_boundary`, `pause_requests_are_bounded_and_unique` |
| malformed requester identity | `malformed_pause_and_throttle_identities_are_rejected` |
| overlapping pause windows | `pause_requests_reject_overlapping_windows` |
| out-of-horizon request | `pause_control_enforces_the_window_boundary` (max window + 1) |
| request count bound | `pause_requests_enforce_the_count_boundary` |

## Snapshot and exact-replay fixtures

VMM fixtures live in
`crates/chaoscontrol-vmm/src/controller/tests/fault_snapshot_fixtures.rs`:

- `directional_modes_snapshot_replay_preserves_packets_and_observation_identity`
  covers stopped/slowed/jammed serialized snapshots and directional, parameter and
  attempt-identity drift rejection without state mutation.
- `overlapping_links_snapshot_replay_preserves_precedence_and_scoped_heal`
  covers restrictive precedence, queued packet retention and scoped heal.
- `throttle_snapshot_restores_exact_budget_and_release_and_rejects_drift`
  covers the per-round exit cap, exact release, and budget/deadline/expired-window
  drift. This is not an aggregate instruction budget across the entire window.
- `custom_and_pause_snapshot_replay_preserves_active_effects_and_suppression`
  covers custom application retained without reinvocation, active throttle
  continuation, pause suppression and malformed pause snapshot rejection.
- `selected_before_pause_snapshot_replays_application_suppression_without_effects`
  covers attempts selected before a pause and suppressed after restore.
- `custom_snapshot_continuation_rejects_missing_adapter_without_invocation`
  covers adapter admission drift on snapshot continuation.

Exact-replay fixtures live in
`crates/chaoscontrol-replay/src/replay/tests/mechanism_fixtures.rs`:

- `every_new_mechanism_saved_trace_replays_exactly_and_rejects_plan_drift`
  covers saved/loaded exact stage traces for each directional mode, scoped heal,
  throttle and custom effect, including plan identity/version drift.
- `overlapping_mechanism_windows_replay_with_exact_order_and_checkpoint_prefix`
  covers overlap ordering and checkpoint prefix drift.
- `pause_replay_dispatches_recorded_admission_and_preserves_mid_window_suppression`
  covers real fault-engine admission, duplicate rejection, suppression and
  debugger navigation from a retained mid-window checkpoint.
- `pause_exact_replay_rejects_missing_changed_and_reordered_control_facts`
  covers control evidence divergence and a checkpoint with identical stage
  events but missing mid-window suppression evidence.
- `exact_replay_fails_closed_when_rejected_pause_request_raw_identity_is_absent`
  covers the deliberate absence of oversized rejected requester text.

The VMM fixtures use real controller snapshots and installed Rust adapters.
Replay stage fixtures use the existing trace runner and do not prove device
execution; pause fixtures use the real fault-engine admission path. Exact replay
compares cumulative controls at their tick and stage boundary. Bounded retained
pause requests are dispatched through normal admission, including typed
rejection. Digest-only evidence for an oversized rejected requester is not
authority to invent replacement raw input: that case fails closed. Arbitrary
mid-round external calls are not synthesized. Missing or differently versioned
custom adapters are not replaced by fallback adapters; admission or exact stage
comparison rejects divergence.

## Boundaries

The authorized fixture repair ran:

```sh
cargo test -p chaoscontrol-vmm --lib controller::tests::fault_snapshot_fixtures && cargo test -p chaoscontrol-replay --lib replay::tests::mechanism_fixtures
```

The first attempt compiled after correcting fault-input nanosecond delays, then
failed all six VMM fixtures because JSON cannot serialize the engine's typed map
keys; the chained replay command did not run. Fixtures now use the production
snapshot CBOR convention through `ciborium` dev-dependencies, with 16 MiB guest
memory per fixture VM. Recording JSON remains metadata-only: the pause fixture
separately persists and reloads checkpoint identity, tick and snapshot as CBOR
artifacts rather than assuming JSON retained them.

The repaired command passed all six focused VMM fixtures and all five focused
replay fixtures. No formatter ran in the repair worker. Integration owns broader
Cargo, formatting/lint and KVM campaign evidence; these focused passes are not
replay promotion or release eligibility. Ledger accounting and the separate
bounded KVM interaction campaign retain their own evidence owners.

## Integration verification

Maintainers: ChaosControl fault, VMM and replay owners. Current consumer: the
four-mechanism KVM interaction campaign. Durable contribution: replay-validated
bounded control facts and seed/profile-bound mechanism receipts, including exact
targets, stage counts, precedence decisions and due-work suppression opportunities.
Empty polls are not selected faults; custom adapter invocation does not establish
an ongoing effect lifetime.

- `cargo test -p chaoscontrol-fault --lib`: 155 passed.
- `cargo test -p chaoscontrol-replay --lib`: 168 passed.
- `cargo test -p chaoscontrol-sim-core -p chaoscontrol-sdk -p chaoscontrol-evidence --lib`: 477 passed.
- `cargo clippy -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay --all-targets -- -D warnings`: passed after fixing the snapshot match guard.
- `cargo test -p chaoscontrol-vmm --lib controller::tests::overlapping_cpu_throttle_is_rejected`: passed.
- `cairn validate --root .`: valid, zero issues; tasks gate: PASS (structural only).
- `cargo octet check`: integration-failure; nightly Cargo panicked at
  `cargo-util-schemas/src/core/package_id_spec.rs:244`, alongside two existing SDK build-script warnings.
- `nix flake check --no-build`: failed evaluation at the missing store derivation
  `/nix/store/2xcwjdylcxvzcvnby1zjcl73rz1fdpk8-mantle-spacewasm-bundler-source.drv`;
  earlier Radicle fetches returned HTTP 504. No Nix build pass is claimed.
- `cairn traceability coverage --root . --json`: failed, 555/555 accepted
  requirements referenced with six dangling `chaoscontrol.fault_mechanisms.*`
  markers whose definitions remain in this active change rather than accepted specs.
  The incomplete campaign is not grounds to sync/archive or suppress those markers.

Pueue task 31 ran the SDK initrd on two VMs, seed 42, hidden TSC,
100 exits per round, 10,000 bootstrap ticks and a 1,800-second outer deadline.
It timed out with exit 124 before guest setup; final serial output remained in
kernel serial-driver initialization. This is operational failure, not campaign
or replay evidence. Task 30 was explicitly killed when changing the clock profile.
The subsequent smoke binds watchdog timers to the executing test thread.
Pueue task 36 also timed out at its 180-second bound, but both guests reached
their workload and printed completion/idling. The next correction uses
`begin_counterfactual_fault_run`: unlike `start_fault_run_at`, it preserves setup
admission after bootstrap. Task 37 exercises that changed path with explicit
phase diagnostics. Source base: `ada2eb33e02da1aa1d9bff87f0011b5ac1fec3c8`
plus the working-tree implementation; no clean-revision campaign claim.
Task 37 timed out with exit 124 as well. Guest workload completion/idling was
observed, but the all-VM setup phase diagnostic was never emitted. Thus no
admitted all-VM setup boundary, mechanism application, suppression/release proof,
or success receipt was observed. The retained prebuilt guest's printed progress
is not evidence of host-side setup admission. Required next diagnostic: reconcile
the retained SDK initrd's setup transport/catalog protocol with current VMM
admission before another interaction campaign; do not extend timeouts blindly.

## Setup diagnosis correction and successful campaign

The earlier setup-admission interpretation above was based on truncated Pueue
output and is superseded. The complete locally retained diagnostic trace
(`/tmp/cc-fault/admission-boundary.log`) shows both catalog completions and both
setup hypercalls returning status 0. Both engines have accepted catalogs and
setup=true at controller tick 1170. No SDK/catalog compatibility change is justified.
Temporary VM hypercall and per-round tracing was removed after diagnosis.

The finite SDK demo completes its workload and enters idle handling during the
interaction schedule. A sustained Raft guest passes the same interaction assertions
and the unchanged 180-second deadline. This establishes a suitable campaign
consumer, not a fix or performance guarantee for idle handling.

Pueue task 45 passed with diagnostic instrumentation. Task 46 repeated the clean
final harness and passed: 1 test, 0 failures, 64.87 seconds reported by the test.
Source base: `120feff8408df9c85af381ca27c2359daf641ab1`. The harness BLAKE3 at
execution was `ae9a1d53dcd96d5900d9447dddcd9c85ec373f1c29704cb1c904e8d5cd2bdd22`.
The run uses two single-vCPU VMs, hidden TSC, seed 42, quantum 100, random faults
disabled and 10,000 bootstrap ticks. Setup completed at tick 1171; the retained
receipt describes state at tick 1200, inside pause [1191,1201). The test then
executes three further rounds and asserts the deferred fourth selection occurs
after release. The receipt is written only after all assertions pass.

```sh
CC_KERNEL=/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux \
CC_INITRD=/nix/store/ymfxhvyv4zr3la0q4k6h6famb9avz8y9-chaoscontrol-initrd-raft \
CC_RECEIPT=/tmp/cc-fault/interaction-raft-final.json \
timeout 180 cargo test -p chaoscontrol-vmm --test fault_interaction_kvm -- --ignored --nocapture
```

| Artifact | BLAKE3 |
| --- | --- |
| Kernel above | `f2abae8a52c83c975ed15de504535445e2ac159cc0198f8028867b0c18cdf67c` |
| Raft initrd above | `2580c3a76bb4f25d2622e7a489e44acff544201a13d79c145a35502a863812af` |
| Retained `2026-11-06-interaction-campaign.json` | `e92d62cf698cc9860de2054e3e4190b8703cfd3883fb5a85e7889139964e3d5d` |

The runtime receipt was generated and revalidated by the Rust mechanism receipt
API during the test. It records three selected/applicable/applied attempts, zero
rejections/application failures, two observations, one custom invocation and
three suppression opportunities for the pending throttle. Those opportunities
are repeated due-work polls, not three distinct faults. Network packets sent=0:
directional installation is exercised, packet-level directional impact is not.
This is bounded KVM execution evidence, not snapshot-backed replay, universal
determinism, protocol correctness or release eligibility. Owner: ChaosControl
VMM/fault maintainers; repeatable consumer: the explicit-artifact ignored KVM
interaction test. SDK setup-positive and inactive-run-negative engine tests and
the final campaign's denied-warning Clippy check also pass. Octet/Nix blockers
above remain unchanged; no archive or release promotion is authorized by this run.




