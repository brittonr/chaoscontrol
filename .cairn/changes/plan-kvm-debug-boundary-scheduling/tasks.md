# Tasks

## Phase 1: Planning foundation only

- [x] [serial] Inspect native package conventions, accepted strict SMP contract, sim-core schedule state/events/journal/snapshot APIs, VMM entry/poison/watchdog paths, and persisted recording/session/replay consumers; record the supplied IRQ counterexample without rerunning it as confirmation. Identify first-party Rust/Linux SMP campaign and persisted debugger consumers and VMM/sim-core/replay maintenance owners. r[chaoscontrol.kvm_debug_boundary.units] r[chaoscontrol.kvm_debug_boundary.scheduler]
- [x] [serial] Author proposal, design, delta requirements and implementation/evidence tasks for a separate opt-in mode; preserve accepted specs, strict tasks/dependencies and historical evidence. Define boundary units, hidden-handler atomic intervals, concrete unit/API migration, finite recovery/poison, delivery uncertainty, persisted identity and actual KVM controls. r[chaoscontrol.kvm_debug_boundary.admission] r[chaoscontrol.kvm_debug_boundary.recovery] r[chaoscontrol.kvm_debug_boundary.persistence] r[chaoscontrol.kvm_debug_boundary.evidence]
- [x] [serial] Run proposal and design gates as document checks with an explicit selected policy, retain their exact CLI cohort/verdict and non-claims here, and distinguish ambient advisory checks from verified selected-cohort closure authority. Do not run product checks or imply implementation/evidence acceptance. r[chaoscontrol.kvm_debug_boundary.evidence]

## Phase 2: Authorized implementation baseline and unit cutover

- [x] [serial] Obtain implementation authorization; run affected baseline checks and enumerate every exported configuration/state/event/CLI/codec/claim consumer before changing APIs. Retain the existing strict counterexample and current campaign/debugger blocker as baseline observations, not newly relabeled coarse evidence. r[chaoscontrol.kvm_debug_boundary.scheduler] r[chaoscontrol.kvm_debug_boundary.evidence]
- [x] [serial] Implement separate `KvmDebugBoundary` admission/profile and typed `ProgressBudget`/`ScheduleProgressState`; migrate the exact fields/events/remaining/strategy/action APIs in the design and every caller/fixture atomically. Reuse seeded policy, runnable validation, journal and identities only where sound; remove superseded names/aliases and leave strict instruction/PMU semantics distinct and fail-closed. r[chaoscontrol.kvm_debug_boundary.admission] r[chaoscontrol.kvm_debug_boundary.scheduler]
- [x] [serial] Implement the closed normalized exit table, pending-I/O settlement and guest-debug arming; record only accepted guest boundaries, explicit zero-unit runnable/injection events and observable delivery facts. Audit IRQ/NMI/exception, interrupt shadow, device timers and virtual-time inputs; reject unsupported or uncapturable profiles without invented instruction/delivery progress. r[chaoscontrol.kvm_debug_boundary.units] r[chaoscontrol.kvm_debug_boundary.persistence]
- [x] [serial] Implement finite per-entry SMP recovery and outer kill/reap bounds; admit actual recovery capability and enforce permanent VM/controller poison after unverified possible execution or post-entry failure. Keep host events abort-only and partial records diagnostic-only; fresh restore/runner is the only retry path. r[chaoscontrol.kvm_debug_boundary.recovery]

## Phase 3: Persistence and first-party consumer cutover

- [x] [serial] Version canonical schedule/trace/scheduler-snapshot domains and containing VM snapshot/recording/session schemas; bind typed mode/units/profile/table, normalized boundaries, pending settlement and available interrupt/exception/device/ledger state. Migrate every identity/codec/validator and downstream claim consumer without reinterpreting old strict bytes or adding compatibility aliases. r[chaoscontrol.kvm_debug_boundary.persistence]
- [x] [serial] Add explicit mode/boundary-budget CLI opt-in to first-party campaign/run, reproduction and persisted replay/inspect; reject missing opt-in, mixed instruction/boundary flags and retained-descriptor overrides. Use shared replay/debugger continuation checks, settled snapshots and verified coordinate publication; reject hidden targets and dispose of failed runners. Update existing operator, retention and claim-boundary docs during implementation. r[chaoscontrol.kvm_debug_boundary.admission] r[chaoscontrol.kvm_debug_boundary.persistence]
- [x] [serial] Add paired portable behavioral fixtures for typed policy, normalized settlement, unknown exits/debug causes, host-event isolation, stale/regressing/overflow/overshoot/capacity cases, delivery uncertainty, schema/identity/config rejection and poison/retry behavior; preserve strict unsupported-mode negatives. Pure fixtures must not be reported as real KVM determinism. r[chaoscontrol.kvm_debug_boundary.scheduler] r[chaoscontrol.kvm_debug_boundary.recovery] r[chaoscontrol.kvm_debug_boundary.evidence]

## Phase 4: Actual KVM qualification and retained evidence

- [x] [serial] Review a named finite Nickel-backed cohort/config with exact host/KVM/CPU/kernel/initrd/workload/artifact identities, seeds, non-default SMP topology, round-robin/randomized budgets, repetition/horizon limits, retained bytes/journal bounds, watchdog and teardown deadlines. Queue actual KVM work with pueue. r[chaoscontrol.kvm_debug_boundary.evidence]
- [ ] [serial] Run repeated first-party Rust/Linux SMP campaigns and snapshot-backed replay controls; compare complete canonical schedules/delivery/fault traces plus selected guest assertions/memory/registers/output under host delay, signal, watchdog cadence and contention perturbation. Retain all outcomes and strict-mode rejection separately; no selective exclusion of timeout/divergence cases. r[chaoscontrol.kvm_debug_boundary.evidence]
- [x] [serial] Exercise actual KVM TF-visible, POPF/IRET, IRQ handler-hidden-work/internal-exit, IF/shadow/deferred IRQ, selected nested NMI/exception/fault-collision and pending-I/O completion controls. Prove classification with state observations; leave unsupported delivery classes explicitly blocked/nonqualified. r[chaoscontrol.kvm_debug_boundary.units] r[chaoscontrol.kvm_debug_boundary.persistence] r[chaoscontrol.kvm_debug_boundary.evidence]
- [x] [serial] Exercise a no-exit nonreturning interrupt handler with another runnable vCPU, actual finite host recovery/kill/reap, host interruption during hidden work and I/O, evidence/exit/action failures and partial multi-VM rounds. Demonstrate abort-only watchdogs, no fabricated switch/progress/time/result, permanent poison and fresh-runner-only recovery. r[chaoscontrol.kvm_debug_boundary.recovery] r[chaoscontrol.kvm_debug_boundary.evidence]
- [ ] [serial] Run coarse campaign finding/reproduction/session export, exit producer, then separate replay and inspect processes; seek backward/forward across persisted checkpoints containing supported pending delivery/device state under non-default topology/clock/budget and compare complete canonical suffixes and selected memory/register observations. Reject an unobservable-region target. r[chaoscontrol.kvm_debug_boundary.persistence] r[chaoscontrol.kvm_debug_boundary.evidence]
- [ ] [serial] Through both actual CLI paths, reject isolated wrong-mode/unit/profile/schema/host/artifact/config/prefix copies, omitted/reordered/forged boundary and injection records, corrupt/truncated/missing snapshots, incomplete settlement, no-opt-in/mixed flags, impossible targets and poison reuse; prove no unverified position or restore/boot fallback escapes. r[chaoscontrol.kvm_debug_boundary.admission] r[chaoscontrol.kvm_debug_boundary.persistence] r[chaoscontrol.kvm_debug_boundary.evidence]
- [ ] [serial] Retain and validate a receipt binding exact source/executables, all configs/commands/seeds/repetitions/outcomes, host capability profile, canonical comparisons, snapshots/sessions, controls, operational failures, supported delivery classes and non-claims. Review a mode-qualified bounded same-cohort claim only after all required actual controls pass; keep failed/unavailable controls blocking. r[chaoscontrol.kvm_debug_boundary.evidence]
- [ ] [serial] After all implementation lands, run affected product checks and required native gates once under the owning integrator; do not mark implementation/evidence complete merely from document checks. Review downstream verdict migration and operator docs against observed evidence. r[chaoscontrol.kvm_debug_boundary.persistence] r[chaoscontrol.kvm_debug_boundary.evidence]

## Phase 5: Acceptance and lifecycle closure

- [ ] [serial] Obtain evidence-backed owner review of explicit coarse semantics, strict non-equivalence, exact API/caller removal, bounded delivery qualification and replay claim consumers; sync accepted delta and archive only with required authorization and all acceptance evidence. Planning delivery leaves this task and every implementation/evidence task open. r[chaoscontrol.kvm_debug_boundary.evidence]

## Planning gate observations

Executed only these document gates with the ambient executable resolved to `/nix/store/x37qabfj71nzk5661f3x735nni4m96qk-cairn-0.1.0/bin/cairn`:

```text
/nix/store/x37qabfj71nzk5661f3x735nni4m96qk-cairn-0.1.0/bin/cairn gate proposal plan-kvm-debug-boundary-scheduling --root /home/brittonr/git/OnixResearch/chaoscontrol --policy /home/brittonr/git/OnixResearch/chaoscontrol/cairn-policy/generated/cairn-policy.json
/nix/store/x37qabfj71nzk5661f3x735nni4m96qk-cairn-0.1.0/bin/cairn gate design plan-kvm-debug-boundary-scheduling --root /home/brittonr/git/OnixResearch/chaoscontrol --policy /home/brittonr/git/OnixResearch/chaoscontrol/cairn-policy/generated/cairn-policy.json
```

Both returned exit 0, `valid=true`, `verdict=PASS`, no issues and advisory evidence mode. Proposal receipt hash: `82090df6ced4afc12462b9051b587647eeb433942cba3dceeb3c5117ff59c2b9`; design receipt hash: `88bc27c92167ca22e3c04a85f4d13085da5f1e7d728f81ffafcebadcda185097`. The design report observed six substantive delta requirements and sixteen scenarios. Both selected `chaoscontrol-default` explicitly with policy hash `4c7b65ad07ce7be8fe6acbf9ac29907887138fe8291b957408b467354ecd4295`.

These are ambient structural/advisory observations, not verified selected-cohort closure authority. The selected `a1e8a0d2d0af0b8b10f63e70fe4a674f5885b127` executable binding recovered by the parent points to `/nix/store/lcvbwhsl29irl1yhw6zg57fn69anjbaa-cairn-0.1.0/bin/cairn`, which was garbage-collected and unavailable at this check. No selected-cohort compatibility, product implementation, KVM execution, determinism, evidence acceptance, sync or archive is claimed. No product tests, builds, linters or formatters ran for this planning assignment. Only the three planning boxes are checked; all implementation/evidence/closure work remains open.

## Implementation and qualification observations

Implementation landed the separate opt-in mode without touching strict instruction/PMU semantics.

- Typed policy: `contracts/evidence/scheduler-progress.ncl`, `contracts/evidence/run-config.ncl`, `contracts/evidence/registry.ncl`, and `contracts/evidence/replay-verdict.ncl` carry the boundary profile, typed budget, run-intent mode, and cross-unit rejection. `crates/chaoscontrol-vmm/src/bin/common/smp_progress.rs` is the single CLI arm.
- Runtime: `crates/chaoscontrol-vmm/src/vm/coarse_boundary.rs` (normalized exit table, settlement, per-entry TSC overwrite, filter identity, authority) and `crates/chaoscontrol-vmm/src/vm/coarse_configuration.rs` (declared profile). The VM's `coarse_clock_filter_identity: Option<[u8; 32]>` is set only after a successful filter install.
- Persistence: schedule/trace/scheduler-snapshot/VM-snapshot/recording/session schemas bind mode, budget, boundary profile, denial-filter identity, pending settlement and the required actual per-vCPU CPUID. Superseded names are removed; no aliases remain. The replay verdict schema advances to 3 because `ReplayScheduleVariant` replaces `quantum_override` with a typed `progress_budget_override` and a typed strategy. Variant-free schema 2 verdicts parse into exactly the same fields, so `validate_verdict_schema` keeps admitting them; any verdict carrying a variant requires schema 3, and the old variant bytes do not deserialize. Positive and negative fixtures cover both paths, and the four committed dogfood proofs keep promoting.
- CLI: `--smp-progress-mode kvm-debug-boundary` requires `--smp-boundary-profile kvm-debug-exit-boundary-v1` and `--smp-boundary-quantum`; missing, mixed or retained-descriptor overrides reject.
- Fixtures: `crates/chaoscontrol-vmm/src/controller/tests/coarse_boundary_fixtures.rs` plus portable paired cases in `snapshot.rs`, `scheduler/core.rs`, `session_tests.rs` and the Nickel fixtures.

Checks actually run on this host:

```text
nix develop -c cargo test -p chaoscontrol-vmm -p chaoscontrol-replay -p chaoscontrol-explore --lib
  exit 0
nix develop -c cargo test -p chaoscontrol-vmm --lib vm::runtime_readiness::coarse_boundary_kvm:: -- --ignored --test-threads=1
  12 passed
nix develop -c cargo test -p chaoscontrol-vmm --lib controller::tests::coarse_boundary_fixtures:: -- --ignored --test-threads=1
  3 passed
nix develop -c cargo test -p chaoscontrol-vm-cohort-adapter tests::kvm:: -- --ignored --test-threads=1
  3 passed
```

The actual KVM controls cover TF-visible single-step boundaries, POPF/IRET rearm, hidden IRQ handler atomicity, IF/STI-shadow deferral, pending-I/O settlement, mixed Debug/PIO/MMIO boundaries, typed NMI rejection, no-exit handler recovery with permanent poison, host-signal abort, and host-delay/watchdog continuation. Continuation comparison is the approved `ExcludeGuardedCoarseClockSamplesV2` projection: raw `KVM_GET_CLOCK` plus each vCPU's MSR data at exactly `0x10`, `0x40000010`, `0x40000020`; raw bytes stay retained and every other field is compared.

Actual guest CPUID retention was verified across CCDs on this host: a source VM pinned to CPU 0 restored its retained table into a fresh, unrun VM pinned to CPU 8 whose initially measured CPUID differed, and the restored `GET_CPUID2` equalled the retained table for both vCPUs.

## Measured qualification blocker

The first-party Linux guest does not reach guest setup under this profile, and the cause is the boundary source rather than a VMM defect:

- Coarse boundaries come only from TF single-step guest-debug, which retires one instruction or one `rep` iteration per boundary. Linux 6.19 `clear_bss` is `rep stosb` over 3,940,352 bytes, observed to consume exactly one boundary per byte (`rcx` decreased 3,000,000 for 3,000,000 boundaries).
- A 2-vCPU coarse boot reached `__sanitizer_cov_trace_pc` at 5,010,000 boundaries with zero serial output at about 10,300 boundaries/s. The reviewed bootstrap allows 1,000,000 boundaries (`10000` ticks x `100` outer exits); the 100,000-tick campaigns (10,000,000 boundaries) also ended with an empty assertion catalog.
- The same kernel and initrd under strict single-vCPU control completed setup at 120,000 exits in 9.14 s with 18,191 bytes of serial through `Run /init as init process`, so the guest artifacts are functional.
- AP bring-up is representable: after an actual INIT-SIPI the AP becomes runnable, is scheduled, executes and halts, so the second vCPU is not the blocker.

Reaching the guest supervisor needs roughly 10^8 to 10^9 instructions, i.e. tens of hours at the measured rate. No smaller admissible fix exists inside coarse mode: natural-exit-only scheduling starves compute intervals and forces the documented watchdog abort plus permanent poison, batching or hiding `rep` work would fabricate unverified progress under a new unit, and PMU/MTF batching is excluded by the contract. The two real 100,000-tick campaigns (`/tmp/cc-persisted-debugger-0tw7wr4k/coarse-live-v2`) are retained as failed outcomes with their checkpoints and empty assertion catalogs; they are not qualification evidence.

Accordingly, campaign/session/replay qualification, CLI rejection of forged coarse artifacts, and the mode-qualified receipt remain open and blocked on a coarser-than-instruction boundary source that preserves one validated, settled, committed unit per boundary. No coarse execution, determinism, replay or release claim is made.
