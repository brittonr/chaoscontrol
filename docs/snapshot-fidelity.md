# Snapshot fidelity and compatibility

ChaosControl snapshots use the `exact-x86-kvm-v1` completeness profile.
The current VMM state schema version is `3`. The outer persisted codec and replay-reference schema remain version `2`; they are separate compatibility boundaries.

## Exact state boundary

The profile includes these state groups:

- guest memory;
- KVM IRQ chip, PIT, and clock state;
- general, special, debug, LAPIC, MP, XCR, XSAVE, event, and required MSR state for each vCPU;
- deterministic time, entropy, timer, panic detection, counters, coverage, fault, and scheduler state;
- serial controller state;
- virtio-mmio transport registers and queue progress;
- block, network, and entropy backend state.

Each snapshot contains an inventory of these components. It also contains the vCPU count and stable virtio identities. A virtio identity contains the MMIO base address, IRQ, and device type. Restore does not use vector position as device identity.

Host handles are not in the deterministic state boundary. These handles include file descriptors, timer handles, performance counter handles, log writers, and output sinks. Restore creates new handles or configures them from restored deterministic state.

## Capture rules

Before capture, the VMM completes pending KVM userspace exits with `immediate_exit` set. This step completes prior I/O or MMIO emulation without running another guest instruction.

Capture rejects unserializable completion faults, pending schedule evidence, poisoned VMs, unsupported backend adapters, and failures to collect required vCPU state. Defined transport failures, pending completion state, and the last request outcome are serialized. Restoring a failed post-effect request must not repeat its backend effect.

Runtime regressions compare the same meaningful guest-exit horizon under real host signals; an unrelated no-progress `EINTR` does not consume that horizon. The backend round-trip scenario compares actual guest continuation, registers, schedule, and virtual time rather than temporal equality of raw hardware `IA32_TSC` samples. Those samples advance with the host clock even at a quiescent guest boundary. Complete snapshot serialization still round-trips the captured MSR inventory without field loss. This is not proof of deterministic direct guest access to the hardware TSC.

## Incremental lineage

An incremental image is a complete overlay relative to its retained base, not just the latest KVM dirty log. Capture includes inherited overlay pages and subsequent guest writes. Restore reverts both the previous overlay and guest writes made since the last capture, then installs the selected overlay. A sibling therefore cannot inherit uncaptured writes from another branch.

`DeterministicVm::get_dirty_bitmap` requires mutable access because observing the KVM dirty log consumes it. The VM retains those observed pages for later incremental capture and restore. Full restore of an overlay installs its base as well as its dirty pages. A later change of incremental base reconciles the current memory against that base before using the page set.

Page-set planning is pure and bounded by guest memory size. Invalid bitmap shapes and page indices reject before page access. Tracking is conservative: a page can remain tracked after its bytes return to the base value. The lineage repair does not change the outer persisted codec or replay-reference schema.

### Repeatable lineage validation

Maintenance owner: `chaoscontrol-vmm`; current consumers are explorer frontier branching and fresh-controller restoration. The `chaoscontrol-explore` example `raft_lineage_proof` compares full and incremental memory at depths one through four, persists and reloads the depth-four parent, and compares long-lived, fresh-full, and watchdog-replacement continuations with an attributed CPU-stall fault. It also checks full-restore-to-incremental recapture.

Build `net-vmlinux` and `initrd-raft` with Nix. Set `KERNEL` to the kernel output's `vmlinux`, `INITRD` to the initrd output file, and `OUTPUT` to a new directory:

```bash
nix develop -c cargo run --offline --locked -p chaoscontrol-explore \
  --example raft_lineage_proof -- \
  --kernel "$KERNEL" --initrd "$INITRD" --output "$OUTPUT"
```

The source records the finite profile: seed 42, three single-vCPU VMs, 128 MiB each, quantum 100, four 500-tick lineage levels, and three 500-tick continuations under the admitted hide-TSC clock profile. Intermediate levels retain full-memory comparison facts; only the replay parent is persisted. Success requires equal initial and final full-memory identities, strict dlog execution digests, every admitted assertion outcome, attributed fault ledgers, round observations, and passing safety invariants. A partial directory or a non-passing `runtime-proof.json` is not proof.

This is bounded continuation evidence, not a `snapshot_backed_reproduced` bug verdict. The control guest has no planted probe. Its dlog digest uses the existing strict determinism fields; it is not a full instruction/register trace or `CanonicalTrace::identity`. Unexercised catalog sites remain visible and are not claimed covered.

The separate watchdog-only probe executes `cli; jmp` until the existing 100 ms VMM timer interrupts it. `watchdog-event.json` must record the real `SI_TIMER` signal, `HostWatchdogTimeout`, rejection of further poisoned-controller operations, disposal, and production `RecoverySnapshot::rebuild_controller` replacement. The replacement restores the bootstrap base and retained parent; full memory and immediate incremental recapture must match. Probe state is discarded and never becomes a guest bug or admitted fault.

UART interrupts are committed synchronously at handled guest-exit boundaries. Restoring a pending UART condition does not inject a second edge over captured PIC state. KVM's host-clock PIT timer is inhibited using `KVM_PIT_FLAGS_HPET_LEGACY`; software `DeterministicPit` still delivers IRQ 0 at virtual-time deadlines. Compatible snapshots retain their serialized bytes while destination PIT restore enforces the same timer inhibition. The focused ISR and PIT regressions cover retained/fresh restore and the positive virtual-time IRQ path.

The Raft guest's `Node::restart` resets volatile protocol state without rolling back its already-applied application prefix. A retained prefix index and the persistent log provide the consumer readback while `commit_index` catches up; uncommitted tail entries remain excluded. Repeated restart and catch-up preserve the same chain, while truncating or changing applied entries still fails the adapter's rollback or divergence checks. This guest model is not a general filesystem-durability guarantee.

Single-vCPU bootstrap and admitted execution both have the existing bounded operational watchdog. After an interrupt, only KVM's architectural `HALTED` state authorizes idle progress to the exact next virtual PIT deadline; a runnable timeout does not advance time or become a guest bug. SMP scheduling remains separate. A strict continuation mismatch retains BLAKE3-bound final VM memory under each local continuation directory for byte-level diagnosis; none of that memory is omitted from the equality decision.

## Restore rules

Restore first runs a read-only preflight. Preflight checks the schema, profile, inventory, topology, memory shape, vCPU state, scheduler state, serial bounds, fault state, and every virtio transport and backend adapter.

A preflight failure does not change VM state. A failure after restore starts permanently poisons the VM. A simulation restore failure after mutation starts also poisons the controller. The caller must discard a poisoned VM or controller.

## Compatibility policy

Schema `3` restores only into the same exact profile and matching VM topology. It requires explicit transport/queue failure, pending completion, and last-request fields. State-schema `2` is not silently upgraded or filled with defaults. The snapshot records the sorted `KVM_GET_MSR_INDEX_LIST` inventory; the restore host must expose that same inventory and support all required KVM state operations.

Legacy snapshots that omit XSAVE, vCPU events, required MSRs, metadata, or the complete inventory are incompatible. Decode or preflight rejects them. ChaosControl does not silently fill missing execution state with defaults.

A newer reader can accept an older snapshot only through an explicit migration that produces a complete current inventory. No such migration exists for incomplete legacy CPU or device state.

## Claim boundary

The profile records and restores the state that ChaosControl owns for deterministic continuation. It does not prove KVM correctness, guest correctness, cross-kernel ABI equivalence, or portability between different VM topologies.

Pre-repair exploration findings at replay-parent depth three or more remain diagnostic-only. Fixing capture and restore does not retroactively validate those artifacts or establish explorer seed repeatability. Fresh evidence must bind its source, exact guest artifacts, configuration, assertion coverage, and retained parent. Production-readiness observations and remaining gate blockers are recorded separately in `dogfood-results/production-readiness-20261015/receipt.ncl`.

Validate the concise operator receipt with `nix develop -c nickel export --format json dogfood-results/production-readiness-20261015/receipt.ncl`. Its runtime summary retains all admitted assertion outcomes once, plus both execution identities, attributed fault ledgers, and hashes of the complete local Rust-emitted records. Raw dlogs and the large parent remain under `target/`; this is not a newly promoted committed workload replay bundle. The required clean-source matrix and native Cairn readiness verdict remain independent of continuation equality.

## Production-readiness closure observations

`dogfood-results/production-readiness-closure-20261015/watchdog-current-artifacts-observations.json` retains the final three-controller comparison on clean source `62f97f8244fedcbb83f1d06eaea72c7dbb37e6bd` and its exact realized guest inputs. The actual watchdog timeout, poison rejection, controller replacement, restored parent memory, and immediate incremental recapture passed. Execution digests, fault ledgers, round observations, and every admitted assertion outcome matched; final guest memory did not. The strict proof remains failed. Its 44 admitted merged assertion sites contain 43 passed and one unexercised outcome; unexercised is not coverage. Earlier comparisons and the interrupt-entropy diagnosis remain separately retained.

`campaign-observations.json` in the same directory distinguishes accepted early ten-round campaigns from later rejected outputs. Early control and planted runs reached admitted depth nine under the unchanged 600-second bound, but did not log per-depth overlays. The later accepted planted cohort records overlays through depth eight; its paired control was rejected. The failed final kernel cohort completed ten structural rounds but rejected both assertion summaries. A completed checkpoint or frontier trace does not establish accepted campaign evidence. No retained safety finding at parent depth three or greater occurred in the listed fresh campaign scopes; this is not a successful deep-replay claim. The separate admitted planted probe reproduced from a depth-one parent on the unpatched cohort.

Attempts 18 and 20 tested guest-kernel TSC capability guards. The first still read unconditional TSC through the separate entropy hook. The second compiled both guards but stopped during Linux bootstrap with operational `HostWatchdogTimeout`; its selected probe was not accepted. Both kernel corrections were backed out rather than shipped as verified fixes. Their source archive, built kernels, exact commands, raw records, and failures remain local. The attempt ledger records 20 of 20 corrective attempts; unchanged-source release observations do not authorize further corrections.

The native audit repaired 106 of 226 missing links and all five dangling references through the recorded source/test evidence. It leaves 120 genuine mandatory gaps explicit. Native readiness also remains blocked on Cairn-owned downstream applicability for `mcp_agent_smoke` and `preserves_corpus_freshness`; no copied self-fixtures, suppressed failures, or skipped checks were used. Current upstream source was retrieved and inspected, but still runs those checks unconditionally. The exact prerequisite is in `cairn-policy/UPSTREAM.md` and the closure's `cairn-current-upstream-audit.json`.

The final release commands tested that exact clean revision, not a later evidence-only delivery commit. Workspace tests passed (2,129 passed, 16 declared ignored), denied-warning Clippy passed, and formatting passed. Full Nix checks failed at `snapshot-replay-smoke`; the local multi-hypervisor command also emitted a failed report despite completing successfully. The seven-row KVM matrix passed six rows but failed `fresh-workload-replay`, so its overall verdict is blocked. Native validation and policy freshness passed; traceability and readiness did not. `receipt.ncl` requires report verdicts as well as command status and computes `production_ready = false`.

The selected depth-one planted probe's historical reproduction remains recorded, but its exact initrd is no longer available. Supported store recovery failed; inspected producer-source outputs and bounded retained-memory candidates did not recover the required bytes. `historical-artifact-recovery.json` records the required BLAKE3 identity and recovery attempts. Current guest artifacts are separately identity-bound and locally rooted; they cannot substitute for the missing historical input. Re-export the closure receipt with the repository's pinned Nickel evaluator; `release-observations.json` retains the exact commands and local artifact locators. Evidence-only updates do not make a later revision release-tested.

Maintenance owner: `chaoscontrol-vmm`. Current consumers: explorer branching, retained-parent restore, controller replacement, and subsequent incremental capture. The command above and retained positive/negative KVM regressions are the repeatable mechanism; these observations do not establish production readiness or a clean-source release verdict.

## Authorized follow-up observations

The follow-up ledger in `dogfood-results/production-readiness-followup-20261016/attempts.ncl` records the user-authorized extension from 20 to 25 corrective attempts. Historical ledgers and receipts are unchanged; the extension does not relabel old failures.

Attempt 21 corrects the guest kernel's runtime TSC checks in both cycle and interrupt-entropy reads. Its jitter-entropy collector now rejects an absent or non-advancing clock before allocation warmup, using Linux's existing no-timer/coarse-timer results without manufacturing entropy. The pinned kernel and inherited patch set remain selected. `runtime21-observations.json` records the actual watchdog timeout, poison enforcement and production replacement, full restored-parent memory, immediate incremental recapture, final memory, strict execution identities, every admitted assertion outcome, faults and rounds. All six comparison fields match across long-lived, fresh-full and replacement continuations through depth four. This is scoped continuation evidence, not an entropy-security or universal determinism claim.

Attempt 23 diagnoses the separate campaign bootstrap rejection: with legacy TSC-visible boot clocks one guest remained setup-incomplete at the unchanged 10,000-tick bound, and the merged assertion report correctly rejected its Pending catalog. Explicit Linux `clocksource=jiffies notsc` admitted all three 44-site catalogs by tick 1175. The reviewed Raft campaign profiles, fresh replay cohort, default proof CLI and snapshot smoke script now select that existing boot policy. Public VM defaults, CPU feature visibility, all execution bounds and assertion/replay admission remain unchanged.

`campaign23-observations.json` binds accepted control and `fig8_commit` campaigns: each completed ten rounds and twenty branches, reached depth ten, and recorded twenty actual overlay captures with per-VM page counts and payload bytes. The control took 142.818 seconds and planted campaign 145.071 seconds under the same 600-second bound; neither retained a safety failure or operational timeout. Both explicit and newly compiled implicit-default selected probes reproduced from valid retained depth-one parents. Use `materialize-snapshot-chunks MANIFEST` for their retained `.chunks.json` sidecars before replaying the public parent locator. These selected probes validate snapshot mechanics, not a discovered Raft algorithm defect; zero deep findings does not satisfy the separate native deep-replay requirement.

Attempt 24 exercised the existing planted probe at guest tick 6000 under the unchanged ten-round campaign bounds. All six retained safety findings reproduced from valid digest-verified parents: five at depth three, one at depth four. `deep-probe24-observations.json` retains each actual verdict, exact assertion/bug identity, snapshot reference, BLAKE3 artifact binding and matched effective profile. The campaign completed in 265.820 seconds without a host watchdog timeout. Its six intentional probe failures are separate from control safety outcomes, liveness observations and operational failures. The initial runner's exit-127 command errors remain recorded; only failed export/replay commands were resumed. This supplies the required actual deep exploration replay, not a Raft algorithm-correctness claim.

Attempt 22 maintains four ChaosControl-owned Preserves inputs and passes the supported Cairn freshness check, including prior stale-hash rejection and a reviewed producer update. No upstream fixtures were copied. Native readiness still requires genuine residual requirement evidence and MCP completion support for nonempty dependency identities and passing Nix completion rows; see `cairn-policy/UPSTREAM.md`. The actual release and native report verdicts remain authoritative. No production-readiness claim follows from these runtime improvements.

Attempt 25 makes the local KVM smoke reject a failed campaign report even when its bounded scheduler exits zero. Three report regressions, the release CLI build, the actual negative CLI path and package Clippy with warnings denied passed. The negative execution returned smoke exit 1, scheduler exit 0 and campaign status `failed`. The enclosing campaign remains 900 seconds; every declared proof bound and both default workloads remain required. The diagnostic default-workload app build failed before VM launch after Nix rejected changing source following an auto-GC wait; it is not guest evidence.

The terminal follow-up receipt separates the two clean source revisions:

| Source | Observed results | Boundary |
| --- | --- | --- |
| `9078d39e8870ba141ff41f1f9f98ab6bb2d3bd9d` | Workspace tests, denied-warning Clippy, formatting, strict watchdog proof and all seven KVM matrix rows passed; matrix verdict `release-eligible`. Nix command exited zero. | The earlier local campaign report failed; process success does not pass that report. Native readiness failed. These observations do not release-test the later correction. |
| `c919a1496963fd7db82b27cf6be6b9e9adc7c65e` | Kernel built. Initrd, workspace tests, Clippy, formatting, Nix checks and matrix commands timed out operationally during Nix auto-GC/collector-lock waits. Native validation and policy freshness passed; traceability and readiness failed. | No watchdog proof ran and no Rust-owned matrix receipt was emitted. No row verdict or VM success is inferred from these host timeouts. |

`dogfood-results/production-readiness-followup-20261016/receipt.ncl` exported successfully with the pinned Nickel evaluator, identity-checked 388 retained manifest entries, and computed `production_ready = false`. It binds the exact source archive, runtime/campaign/replay records, both clean-source command sets, native reports, missing candidate reports, and the 25-attempt ledger. Raw logs and VM artifacts remain local under the recorded artifact root; historical receipts remain byte-identical. Later receipt/documentation-only delivery commits are not the tested source revision.

Remaining prerequisites: genuine evidence for 120 mandatory requirements; Cairn-owned MCP producer support for nonempty dependency identities and passing native Nix completion payloads; successful candidate-source release gates and both actual local workloads; and the exact historical initrd identified in `historical-artifact-recovery.json`. Four consumer-owned Preserves cases already pass supported freshness validation. No check is skipped, old receipt promoted, timeout reclassified as a guest bug, or additional corrective attempt authorized. Maintenance owner remains `chaoscontrol-vmm`; Cairn producer changes require their upstream owner.

## Authorized Cairn producer observations

The separate `dogfood-results/production-readiness-cairn-20261017/` receipt records the authorized extension through attempt 30. Cairn commit `35e6bd4c4d6e9d5a3639a0e70ca922faf3b94a48`, on its dedicated public change branch, implements genuine dependency-file identity observation and native Nix build-payload admission. Twelve actual local producer cases, three native Nix cases, and independent CLI/MCP positive and negative controls passed. The exit-zero failed-build MCP result remains `valid=false`; read-only MCP executes no checks. These are bounded producer-mechanism results, not the complete ChaosControl required-check contract or its `mcp_agent_smoke` input.

Cairn aggregate verification remains nonpassing: its whole workspace command exceeded 600 seconds after the producer cases passed, two native license-evidence requirements remain missing, Octet rejected noncanonical finding paths, and packaged policy freshness/Nix checks timed out on the global collector lock. Clippy with warnings denied, formatting and native validation passed. The Cairn change remains active and unmerged. The incomplete policy import probe was retained locally and removed from the consumer; its recorded compatible policy cohort and generated projection remain unchanged.

The source audit mapped all 120 remaining ChaosControl requirement IDs, with zero safe metadata-only whole-requirement closures. No blanket markers or evidence-root expansion were applied. The earlier deep Raft, watchdog and release observations above remain bound to their original exact revisions; they are not rerun, relabeled or promoted by this evidence-only delivery. Production readiness remains false. All 30 authorized corrective attempts are consumed; the receipt names the remaining owner-specific prerequisites without authorizing another repair, weakening bounds or changing host configuration.

## Authorized terminal extension

The separate `dogfood-results/production-readiness-extension-20261017/` receipt and ledger retain the prospectively authorized ceiling of 35 corrective attempts. Fresh release commands tested exactly clean implementation revision `6190343c9ffe578f39c13501bb44048c8179e712`. A later receipt/documentation-only commit is not that tested revision. Historical receipts remain unchanged.

Workspace all-target tests, Clippy with warnings denied, and formatting passed. The existing `raft_lineage_proof` also passed on the current VMM and its exact recorded guest inputs: actual timer/watchdog interruption, poisoned-controller rejection, controller replacement, retained-parent memory, incremental recapture, and equal initial/final memory, strict execution identities, every admitted assertion outcome, fault ledgers, and rounds across all three continuations. Assertion coverage remains explicit: 43 passed and one unexercised of 44 sites; no whole-catalog or universal-determinism claim follows.

All seven required matrix rows ran with their declared commands and bounds:

| Row | Terminal status |
| --- | --- |
| `build-release-binaries` | `passed` |
| `build-raft-guest` | `passed` |
| `deterministic-smp` | `passed` |
| `serialized-snapshot-replay` | `passed` |
| `virtio-malformed-input` | `passed` |
| `admitted-drift` | `timed-out` at 1,800 seconds |
| `fresh-workload-replay` | `timed-out` at 3,600 seconds |

The Rust-owned overall matrix verdict is `blocked`. Full Nix checks separately exhausted their unchanged 10,800-second bound during derivation evaluation. Their log retains an unavailable Git source and expired cached-ref warning; the timeout does not establish a deeper root cause. Neither these operational timeouts nor the queue monitor's separate broken pipe is a guest bug or a substitute gate verdict.

The completed control/planted campaigns and six deep safety replays remain bound to their earlier exact cohorts in `campaign23-observations.json` and `deep-probe24-observations.json` under the follow-up evidence directory. Both paired campaigns completed ten rounds through depth ten under 600 seconds and retain every measured overlay size. All six retained deep safety findings reproduced from valid parents: five at depth three and one at depth four. The selected depth-one planted probe also reproduced. A finding-producing explorer campaign returns 1; validated output and every actual snapshot-backed replay verdict, not process success alone, establish this criterion. These intentional probe findings do not prove a newly discovered Raft algorithm defect.

Native validation, policy freshness, and the four consumer-owned Preserves cases pass. Native traceability remains nonpassing with 118 mandatory missing requirements and zero dangling references; `mcp_agent_smoke` also fails. No blanket evidence markers, copied upstream fixtures, skipped checks, or ungated policy migration close those gaps. The current producer native gap is `cairn.octet_integration.evidence.blocked`; a fully passing immutable Cairn source cohort and the original complete downstream MCP contract remain required. Alternate Rust 1.98 diagnostics, a proven overlong diagnostic socket path, and missing `nickel-export` are retained separately from pinned producer gates.

The manifest binds current inputs with BLAKE3 and preserves historical receipt bytes. The exact historical initrd `blake3:82b2ef606827644e8d10ca98142344e9fe5bae06cc3c11f837f5734654bf646e` remains unavailable; a replacement with different bytes is not accepted. Current kernel/initrd and executable retention aliases, raw logs, snapshots, complete assertion tables, bug files, and replay attempts remain local at the recorded paths.

Use the recorded Nickel 1.17.0 cohort to evaluate the receipt:

```bash
nickel export --format json dogfood-results/production-readiness-extension-20261017/receipt.ncl
```

`observations.json` retains exact argv, environments, limits, reports, and source boundaries. `receipt-validation.json` binds the actual export and artifact identities. Ownership remains with `chaoscontrol-vmm` for runtime/evidence and Cairn CLI/core maintainers for producer admission. The final release/readiness task stays unchecked; all 35 corrective attempts are consumed. No lifecycle sync, archive, readiness promotion, host reconfiguration, or production-readiness claim follows.

## Authorized continuation implementation

The separate continuation ledger authorizes corrections through attempt 40;
earlier ledgers, receipts, campaign cohorts and verdicts remain unchanged.
The existing incremental-lineage repair and `raft_lineage_proof` are reused.

Restore preflight now checks IRQ-chip component identities and the admitted
PIT clock/profile before quiesce or memory replacement. Ready virtqueues require
accepted features, while legitimate failed-driver snapshots remain representable.
The actual KVM runtime suite exercises malformed-state isolation, complete
backend continuation, concurrent host signals/CPU work, and a required
`KVM_SET_PIT2` failure after guest-memory replacement. That last scenario repairs
the fixture descriptor and still requires permanent non-runnable restore poison;
an operational failure is not a guest panic. The focused eight-scenario suite,
workspace all-target tests, denied-warning Clippy and formatting passed before
the clean-source release run. These checks do not establish exhaustive raw
architecture-value preflight, PMU continuation, or historical engine equivalence.

Runtime assertion `process_instances` are preserved through bug extraction,
deduplication, checkpoint/resume, summaries, replay and artifact admission.
The observed set is bounded, unique and token-validated; owner removal and
substitution reject. It does not attest the supervisor or establish whole-guest
coverage. Fallback records retain their stronger independent admitted
record/process/sink scope; application details cannot supply SDK owner authority.
Nickel review contracts admit historical empty sets and the exact cardinality
and token boundaries, and reject duplicate, malformed and oversized sets.

Frontier proposals use only newly observed markers, not inherited cumulative
history. Raft emits `raft/leader-elected` at its actual Candidate-to-Leader
transition with real node/term position, without fabricating a state reference.
Reproduction applies a retained schedule variant before counterfactual execution
and still checks the independently observed policy. Source/regression checks do
not prove marker-selected retained-parent replay or a nondefault-policy campaign;
those runtime obligations remain separate until an actual consumer verdict exists.

Retained SMR admission checks client bounds, complete operation inventory and
proposal/command references before consumer effects. Process lifecycle receipts
bind role and identity to the same admitted process. The registry requires all
previously authored runtime ownership families, and its actual CLI rejects each
newly enforced family's omission. These validations do not substitute for the
standalone SMR replay shell, exact upstream kernel receipts, privileged eBPF
scenario corpus, or immutable source/baseline/adaptive-parity evidence.

Maintenance owner: `chaoscontrol-vmm` and each existing crate owner. Current
consumers are restore/recovery, explorer resume/reproduction and evidence
admission. Final clean-source Cargo, Nix, native, watchdog and seven-row matrix
verdicts remain independent requirements; no production-readiness claim follows.

## Continuation current-guest evidence

The continuation receipt under
`dogfood-results/production-readiness-continuation-20261017/` binds clean
implementation `7dace3fbd283c61e4e8ea0ed59da7e143fbc36fb`, separately from prior
clean `2aa212f87021391f03717b86089330db0c8967d8` and later evidence-only commits.
All 40 explicitly authorized corrective slots are consumed. Historical receipts
and their source cohorts remain unchanged.

Attempt 36 used its still-unstarted implementation slot after read-only Nix
diagnosis. The current Raft guest had retained its log but reset `commit_index`
on protocol restart; all three continuations observed `raft-adapter-rollback`.
The corrected consumer preserves its applied application prefix and rejects
changed or truncated applied history. The failing-before regression, all 109
Raft library tests and denied-warning Clippy are retained. This is not whole-SMR
contract completion.

On the corrected clean source, workspace all-target tests, denied-warning
Clippy and formatting passed. The rebuilt current guest passed the existing
three-controller proof: an actual `SI_TIMER` watchdog interruption, permanent
poison enforcement, production controller replacement, exact retained-parent
memory and immediate incremental recapture. Initial/final memory identities,
strict execution identities, every admitted assertion outcome, fault ledgers
and rounds matched across long-lived, fresh-full and replacement controllers.
All three tables retain 44 passed and one unexercised of 45 admitted sites, with
no safety failure. The watchdog is an operational host timeout, not a guest bug.

| Parent depth | Observed dirty pages | Overlay payload bytes |
| --- | ---: | ---: |
| 1 | 1,910 | 7,823,360 |
| 2 | 2,808 | 11,501,568 |
| 3 | 3,699 | 15,151,104 |
| 4 | 4,191 | 17,166,336 |

Payload bytes are the proof's observed page counts times 4,096, not compressed
snapshot sizes. Each depth matched a full-memory reference; only the depth-four
parent was persisted by this proof. These measurements do not stand in for a
completed explorer campaign.

The corrected release-profile control, `fig8_commit` and deep-probe campaigns
retain accepted one-round checkpoints, then fail with
`Frontier { source: Adapter(Campaign(BoundExceeded)) }`. Their unchanged
600-second bounds were not exhausted. Earlier debug-profile timeouts remain
separate operational observations. Neither valid checkpoint admission nor an
empty deep-finding inventory passes ten-round/depth-four exploration.

All six original retained deep findings were replayed again: five depth-three
parents and one depth-four parent, with exact bug/assertion identity, parent
reference and valid digest-verified `snapshot_backed_reproduced` verdicts.
The selected original depth-one probe also reproduced. These actual commands
used the separately identity-verified original executable and original inputs;
they do not prove current-reader acceptance. Current readers reject the old
parents' missing mandatory device failure state. No healthy default, substituted
snapshot or relabeled fresh finding hides that rejection. Intentional snapshot
probe failures are not discovered Raft algorithm defects.

The exact frozen-source native report remains nonpassing: 266 of 382
requirements referenced, 116 mandatory gaps, zero dangling coverage references,
and a failing `mcp_agent_smoke`. Supported consumer-owned Preserves freshness
passes. Two task-note SMR references were outside the snapshot change's scope;
the evidence-only delivery removes those references and passes actual strict
native validation without claiming either whole SMR requirement. The frozen
validation failure remains retained separately.

The unchanged owner prerequisites remain explicit: one fully passing immutable
compatible Cairn producer cohort through its required Cargo/Nix/native/Octet
gates and the original complete downstream MCP contract; genuine residual
ChaosControl requirement evidence; and the exact missing historical initrd.
The local Nix dependency diagnosis isolates a client wait after daemon
`AddToStore` for `base-orphans-0.9.3.tar.gz.drv`, while source-independent new
derivation registration succeeds. It does not establish a general collector
lock cause or authorize host reconfiguration.

Maintenance owner: `chaoscontrol-vmm` for current runtime and evidence;
Cairn CLI/core maintainers for producer admission. Exact commands, environments,
profiles, complete assertion tables, raw logs, retained snapshots and executable
aliases remain bound by the receipt's BLAKE3 manifest. No production-readiness
claim follows from these scoped passes.

### Terminal corrected-source release gates

The unchanged-bound release consumers are terminal on clean `7dace3fb`:

| Required gate or row | Observed verdict |
| --- | --- |
| Workspace all-target tests | passed |
| Clippy with warnings denied | passed |
| Formatting | passed |
| Strict current-guest watchdog proof | passed |
| Full Nix checks | operational timeout at 10,800 seconds |
| `build-release-binaries` | passed |
| `build-raft-guest` | passed |
| `deterministic-smp` | passed |
| `serialized-snapshot-replay` | passed |
| `virtio-malformed-input` | passed |
| `admitted-drift` | operational timeout at 1,800 seconds |
| `fresh-workload-replay` | operational timeout at 3,600 seconds |

The Rust-owned overall matrix verdict is `blocked`, not `release-eligible`.
Its generated Markdown retains unconditional passed-matrix boilerplate; that
sentence does not override the typed blocked verdict and two timed-out rows.
No historical or dirty-source result replaces any corrected-source failure.

Evaluate the separate continuation receipt with the recorded Nickel cohort:

```bash
nickel export --format json dogfood-results/production-readiness-continuation-20261017/receipt.ncl
```

The receipt requires every original criterion, complete gate/attempt inventories
and passing report verdicts. `receipt-validation.json` records its actual export,
artifact identity checks and rejection of an unsupported readiness claim.
Production readiness remains false; the exact upstream, current-campaign,
requirement-evidence, host-owned Nix and historical-retention prerequisites stay
open. No lifecycle sync, archive or additional corrective slot is authorized.


## Architectural and virtio corrections through attempt45

The separate `dogfood-results/production-readiness-attempts41-45-20261018/`
receipt owns this corrective cohort; earlier receipts keep their original
source, artifact and verdict identities.

CPU preflight now checks standard XSAVE header representation, MXCSR reserved
bits, architectural RFLAGS, CR8 and reserved debug-register flags before raw or
complete-VM restore effects. Pending NMIs are a count: two are valid while
unmasked; they are not rejected as a non-boolean value. PIC, IOAPIC, PIT and
clock payload admission preserves actual KVM reset and partial-word states.
These facts-only checks do not establish every host-dependent CPUID, XSTATE or
MSR value constraint; those broader requirements remain open.

The raw-vCPU continuation fixture enables the selected SSE state and writes its
active XSAVE component, rather than merely changing legacy FPU storage. A guest
IRET leaves the masked-NMI region. The exact control and restored traces must
observe pending NMI delivery, the required MSR, the nondefault XMM byte and both
subsequent outputs. The initial duplicate-NMI and zero-XMM control failures stay
in the new attempt ledger; expected outputs were not relaxed.

Virtio queue lifecycle, completion, scratch sizing and block-error decisions
remain allocation-free pure plans. Cursor commit still follows successful
used-index publication. Generated bounded-shell cases require actual request
family reachability, allocator observations, exact backend/guest output and
prevalidation isolation. The malicious-guest fixture executes production MMIO
dispatch against the same memory mapped into KVM, including malformed queue
configuration and descriptors; a manually forwarded MMIO fixture is not its
substitute.

Only requirement clauses supported by those implementation paths and behavioral
oracles acquire links. Every remaining native requirement, downstream Cairn
producer prerequisite, strict watchdog comparison and clean-source release gate
remains mandatory. Maintenance owner: `chaoscontrol-vmm`.

The integrated precommit workspace, denied-warning Clippy, formatting and
strict watchdog proof passed. The actual control campaign no longer exits with
`Frontier BoundExceeded`: it retained seven rounds and fourteen branches before
the unchanged 600-second operational timeout. Marker-only frontier regressions
also pass in the complete workspace run; their earlier focused timeout remains
recorded. None of those observations closes the ten-round/depth-four campaign
or clean-source release requirements. All45 authorized corrective slots are
consumed; further gates and evidence collection are not authority for another
product correction.

### Clean-source campaign, replay and watchdog observations

The tested implementation is clean
`b51beafc4507cab1fa32bb7a7a3b83d24ed543fc`, not the later evidence-only delivery
commit. Workspace tests, denied-warning Clippy, formatting and the strict
real-watchdog `raft_lineage_proof` passed on that source.

The unchanged 600-second control and planted campaigns each retained eight
rounds and sixteen branches, with no retained findings, before an operational
timeout. Neither completes the required ten-round/depth-four campaign. The
separate deliberate deep probe completed ten rounds and twenty branches. Its
eight retained safety findings all have recorded replay-parent depth three;
every one produced a current-reader `snapshot_backed_reproduced` verdict with
matching bug/assertion/parent identity and a validated snapshot. The admitted
frontier entry depth reached three, not four. The exact campaign profile did
not emit per-capture dirty-page diagnostics, so no campaign overlay sizes are
inferred from missing diagnostics or compressed file lengths.

An initial debug export timed out at its unchanged 300-second bound and
partially overwrote in-place ordinal bug filenames, yielding duplicate bug
identities. Those failed command records remain retained. The complete
checkpoint and a successful fresh-directory release export retain all eight
unique identities; the completed replays use that export, not the partial one.
The archived failed helper is diagnostic history, not the reproduction recipe.

The six historical deep findings and selected original planted finding were
also attempted by the current reader. Their snapshots were rejected with CBOR
`missing field failure`; historical original-engine successes remain in their
original receipt and are not current-source passes. Fresh deliberate probes
do not replace that selected original finding or establish a discovered Raft
`fig8` defect.

The strict controller proof separately observed a real watchdog timer signal,
poison enforcement and controller replacement. Restored parent memory and
immediate incremental recapture matched. Long-lived incremental, fresh full
and watchdog-replacement incremental continuations matched every admitted
assertion outcome, initial/final memory identity, strict execution identity,
fault ledger and round observation. Each reported 44 passing, zero failing
and one unexercised site in the 45-site catalog. The host timeout was classified
as operational, never as a guest bug.

The Rust-emitted controller proof records these overlay payload sizes; these
are dirty pages times 4096, not compressed store sizes or campaign captures:

| Depth | Dirty pages | Overlay payload bytes |
| --- | ---: | ---: |
| 1 | 2012 | 8241152 |
| 2 | 2582 | 10575872 |
| 3 | 3466 | 14196736 |
| 4 | 4088 | 16744448 |

Frozen-source native traceability references 278 of 382 requirements: 104
mandatory gaps remain, with zero dangling coverage references. Consumer-owned
Preserves freshness passes; MCP smoke does not. Five task-metadata issues are
corrected only in the evidence delivery tree; its passing validation does not
replace the frozen-source native failure. Every original criterion remains
mandatory, and all 45 authorized corrective slots remain consumed.

### Terminal clean-source release verdict

All declared commands are terminal for clean `b51beaf`. Workspace output
reports 2203 passed, zero failed and 16 ignored tests. Clippy with warnings
denied, formatting and the strict watchdog proof passed. Required Nix checks
timed out at the unchanged 10800-second bound. Evaluation last reported
`packages.x86_64-linux.raft-accepted-verdict-dogfood`; the recorded Radicle
fetch warning did not stop earlier derivation evaluation. This observation
does not prove a particular daemon, dependency or repository root cause.

The Rust release matrix emitted an overall `blocked` verdict on the same
clean revision. Every required row ran:

| Required row | Terminal status |
| --- | --- |
| `build-release-binaries` | passed |
| `build-raft-guest` | passed |
| `deterministic-smp` | passed |
| `serialized-snapshot-replay` | passed |
| `virtio-malformed-input` | passed |
| `admitted-drift` | timed out at 1800 seconds |
| `fresh-workload-replay` | timed out at 3600 seconds |

The two timed-out rows emitted no stdout/stderr. Their operational deadlines
are not guest safety findings, skipped checks or passing evidence. No declared
bound, required row, original acceptance criterion or strict comparison was
relaxed. No previous dirty-source receipt is promoted.

The separate attempts41–45 receipt retains commands, environments, bounded
profiles, BLAKE3 identities, assertion/fault/snapshot records, all current and
historical replay attempts, and passing/nonpassing report verdicts. Validate it
with the pinned Nickel cohort:

```bash
nickel export --format json dogfood-results/production-readiness-attempts41-45-20261018/receipt.ncl
```

Successful export validates the evidence and its computed blocked outcome,
not production readiness. The exact older missing historical initrd remains a
retention blocker; a different current initrd is not its replacement. Cairn
maintainers must supply an immutable compatible cohort passing the unchanged
Octet integration and full producer Cargo/Nix/native gates before the supported
policy update and original full MCP completion contract can be admitted.
Remaining ChaosControl requirement/campaign/compatibility work and fresh
passing Nix/matrix verdicts remain independent prerequisites. All 45 corrective
slots are consumed; this evidence closure authorizes no further correction,
sibling edit, host reconfiguration, lifecycle archive or release claim.

`receipt-validation.json` records successful Nickel export, independent
BLAKE3/length checks of all 559 retained manifest artifacts with zero identity
mismatches, six checked helper-source versions, unchanged historical receipt
bytes, and rejection of an unsupported `production_ready = true` claim.
The computed outcome is `blocked`: historical artifact retention and the
native, campaign/replay and release criteria remain nonpassing. This delivery
adds repeatable owner-local runtime and exact-source evidence capability, not
production-readiness authority.

## Focused staged-prefix correction (attempt46)

`ControlSession::publish` now advances its validated projection after each
accepted staged control through the existing `seal` path. The next publication
starts at that accepted prefix rather than replaying all earlier staged controls.
Independent applied-state comparison, publication fences, resource reserves,
final segment sealing, journal durability and adopt-after-commit remain intact.
The regression compares every staged prefix with a cold reconstruction under
the same projection bound and verifies stale-control rejection without mutation.
Maintenance owner and current consumer: `chaoscontrol-vmm` and the Campaign
frontier's existing publication/recovery path.

The final implementation is clean `ad7e892ad6838872432cc227cb1c88bb6f0ae303`.
All three original campaign profiles completed ten rounds and twenty branches
under their unchanged 600-second bounds:

| Campaign | Wall seconds | Maximum admitted depth | Retained findings |
| --- | ---: | ---: | ---: |
| Control (`raft_bug=none`) | 245.729 | 3 | 0 |
| Planted (`raft_bug=fig8_commit`) | 246.746 | 3 | 0 |
| Deliberate deep snapshot probe | 319.544 | 3 | 8 safety findings |

Control and planted catalogs were Accepted with 44 passing, zero failing and one
unexercised site of 45. The deliberate probe reported its expected safety
assertion failures; its exit 1 is distinct from an operational timeout. Every
retained probe finding has replay-parent depth three. Existing info-level VM
capture diagnostics provide dirty-page counts without a product logging change.

Ten completed rounds do **not** establish depth four: the admitted traces reach
only depth three. The separate controller proof's depth-four snapshot does not
replace that campaign obligation. Original historical replay/retention, native
readiness, Nix and clean-source release-matrix obligations also remain mandatory.
The separate [attempt46 receipt](../dogfood-results/production-readiness-attempt46-20261018/receipt.ncl)
binds each observed verdict and the unchanged acceptance boundary; earlier
receipts and the initial pre-format control observation retain their own sources.

All eight current retained depth-three probe findings reproduced with
`snapshot_backed_reproduced`, matching bug/assertion/parent identities and valid,
present, digest-verified retained snapshots. The actual Rust checkpoint export
admitted and matched every retained identity before replay. Control and planted
retained no findings; neither liveness failure nor operational timeout was
reclassified as a safety finding. These current deliberate-probe successes do
not replace the selected original historical planted finding.

Each campaign has twenty aligned captures: two at depth one, two at depth two
and sixteen at depth three. Summed per-VM dirty pages times 4096 give these
overlay payload ranges; compressed snapshot lengths are not substituted:

| Depth | Control bytes | Planted bytes | Deliberate probe bytes |
| --- | ---: | ---: | ---: |
| 1 | 7782400–8069120 | 7843840–7856128 | 7540736–7565312 |
| 2 | 10035200–10391552 | 9674752–10018816 | 9613312 |
| 3 | 12800000–13811712 | 12918784–13824000 | 11096064–11669504 |
| 4 | not observed | not observed | not observed |

The same clean implementation passed workspace all-target tests (2204 passed,
zero failed, 16 ignored), workspace Clippy with warnings denied, and formatting.
Its existing `raft_lineage_proof` observed one real `SI_TIMER` watchdog signal,
poison enforcement and production controller replacement. Restored parent memory
and immediate incremental recapture matched. Long-lived incremental, fresh full
and replacement incremental continuations matched initial/final memory identities,
strict execution records, every admitted assertion outcome, fault ledgers and
rounds. Each retained 44 passing, zero failing and one unexercised site of 45.
The watchdog remained an operational timeout, not a guest bug.

The controller proof independently measured overlay payload bytes of 7983104,
10350592, 14024704 and 16015360 at depths one through four. These are **controller
proof** observations, not missing depth-four campaign measurements.

The required Nix/full native/matrix gates are not rerun or promoted by this
scoped correction; their previous nonpassing observations retain their original
source. No new clean-source matrix verdict is manufactured. Refreshed Cairn
refs still identify main `d2c1dd7` and producer `35e6bd4`; the same
[failed producer job](https://github.com/OnixResearch/cairn/actions/runs/39011460673/job/117334691926)
stops fetching `ssh://git@github.com/OnixResearch/onix-artifact.git`.
An immutable compatible producer cohort must pass its original complete
Cargo/Nix/native/Octet and MCP integration contracts before consumer migration.
The exact historical initrd remains unavailable. All 46 authorized attempts are
consumed; no further product correction or host change is authorized.

Validate this scoped receipt with `nickel export --format json
dogfood-results/production-readiness-attempt46-20261018/receipt.ncl`.
The adjacent `receipt-validation.json` retains independent BLAKE3/length checks,
the unsupported-readiness rejection and strict delivery-metadata verdict.
Successful export validates the computed **blocked** outcome, not readiness;
later evidence/documentation commits are not the clean implementation tested.

## Focused logical-marker correction (attempt47)

`Explorer::entry_proposals` now emits one ranked proposal per distinct logical
marker/owner for a completed branch snapshot. Every observed instance still
updates hit counts and rarity; the first, highest-novelty instance supplies the
retained context. Campaign and legacy-diagnostic backends share this
normalization. Rank conversion, candidate admission, capacity pruning,
publication, entropy and declared bounds are unchanged.

The bounded-frontier regression failed before the correction: Campaign retained
depth one instead of a distinct depth-two child under repeated marker aliases.
After normalization, both new regressions and the complete explorer library
passed (418 passed, zero failed, one ignored), including the existing
Campaign/legacy parity corpus.

The final implementation is clean
`b5f0a4550f86cfa3f5b9f45206a57d4087225182`. All three original profiles
completed ten rounds and twenty branches under their unchanged 600-second bounds:

| Campaign | Wall seconds | Maximum admitted depth | Retained findings |
| --- | ---: | ---: | ---: |
| Control (`raft_bug=none`) | 280.479 | 8 | 0 |
| Planted (`raft_bug=fig8_commit`) | 283.577 | 8 | 0 |
| Deliberate deep snapshot probe | 307.813 | 4 | 8 safety findings |

Control and planted catalogs were Accepted with 44 passing, zero failing and one
unexercised site of 45. The deliberate probe reported its expected safety
assertion failure; its exit 1 is distinct from an operational timeout. Each
campaign has twenty aligned per-VM dirty-page captures. This cohort therefore
measures campaign snapshot lineage beyond the required depth four:

| Depth | Control bytes | Planted bytes | Deliberate probe bytes |
| --- | ---: | ---: | ---: |
| 1 | 7757824–7831552 | 7852032–7901184 | 7548928 |
| 2 | 10125312–10133504 | 9752576–10104832 | 9601024–9773056 |
| 3 | 12677120–13729792 | 12795904–13676544 | 11075584–11153408 |
| 4 | 15130624–18477056 | 15679488–16932864 | 11153408–11161600 |
| 5 | 16740352–16891904 | 17502208–17784832 | not observed |
| 6 | 19554304–20373504 | 19791872–20131840 | not observed |
| 7 | 22450176–23547904 | 23220224–23269376 | not observed |
| 8 | 24530944–24760320 | 25116672–25153536 | not observed |

All eight retained depth-three safety findings reproduced with
`snapshot_backed_reproduced`, matching bug, assertion and parent identities and
valid, present, digest-verified retained snapshots. Control and planted retained
no findings; neither liveness failure nor operational timeout was reclassified
as a safety finding.

Three fresh strict watchdog attempts exited 1 with
`HostWatchdogTimeout { vcpu: 0 }` while host load averaged 34–44 on 32 CPUs.
Those are operational host timeouts, not guest findings. The unchanged proof
passed in attempt46 on `ad7e892`; that remains separate prior evidence for an
unchanged code path, not a fresh `b5f0a45` pass.

The same clean implementation passed workspace all-target tests (2206 passed,
zero failed, 16 ignored), denied-warning workspace Clippy and formatting. Fresh
exact-source native traceability still reports 103 missing requirements, zero
dangling references and verdict `fail`. Nix and the seven-row KVM matrix were not
rerun; their prior nonpassing observations keep their original source. The
selected original planted finding still lacks an accepted current-source
snapshot-backed replay because its exact historical initrd remains unavailable.
All 47 authorized attempts are consumed; no further product correction, sibling
edit or host change is authorized.

Validate this scoped receipt with `nickel export --format json
dogfood-results/production-readiness-attempt47-20261018/receipt.ncl`.
The adjacent `receipt-validation.json` retains independent BLAKE3/length checks,
the unsupported-readiness rejection and strict delivery-metadata verdict.
Successful export validates the computed **blocked** outcome, not readiness;
later evidence/documentation commits are not the clean implementation tested.

## Shared replay-parent retention (attempt51)

`BugReport::snapshot` now holds an `Arc<SimulationSnapshot>`. Failures from the
same round share its immutable parent; final reports and minimization retain that
ownership rather than copying guest RAM. Count-only and checkpoint consumers
iterate borrowed bug reports. Checkpoint and final-output persistence encode a
shared parent/depth pair only once per operation. These local pointer keys are
not serialized identities, and lineage depth remains part of each cache key.
The snapshot format, digest, parent validation, assertion admission and configured
execution/teardown bounds are unchanged.

The retained immutable pre-fix launcher reached round zero but exceeded its
240-second exploration bound. Its syscall trace records 150 guest-memory-sized
unmaps, about 39 GiB in total. Source inspection found repeated full-parent
copies for bug retention and counts, followed by per-bug encoding of the same
parent. The corrected actual fresh-probe launcher completes in 200.1 seconds
and emits `snapshot_backed_reproduced` with a valid digest-verified depth-one
parent. This initial proof uses the rebuilt guest artifact and is not evidence
that the exact original planted finding reproduced. The separate receipt records
the controlled original-input run, clean-source gates and remaining prerequisites.

The controlled run retaining the original immutable kernel, initrd, cohort, seed
and bounds also accepts snapshot-backed reproduction, in 201.835 seconds. The
unchanged strict watchdog proof completes in 355.474 seconds, observing one real
SI_TIMER timeout, permanent poison rejection and production replacement. All
three continuations match parent/recaptured memory, execution identities, every
admitted assertion outcome, fault ledgers and rounds: 44 passing, zero failing,
one unexercised site of 45 each. Controller overlay payloads at depths one through
four are 7827456, 10162176, 14548992 and 16588800 bytes. The deliberate watchdog
timeout remains operational, never a guest finding.

ChaosControl's VMM/explorer maintainers own this shared-parent retention path.
Reproduce through `nix run .#raft-accepted-verdict-dogfood -- --output NEW_DIR
--max-attempts 1 --start-seed 0`; the wrapper retains its declared inner bounds.
See `dogfood-results/production-readiness-attempts49-51-20261020/` for commands,
artifact/configuration identities, trace facts and independent report verdicts.
No historical receipt or failed matrix row is reclassified by this correction.

The final clean release source is
`ab006a79ce6de78ca40cbdde60d35efce07513a9`. Workspace all-target tests report
2223 passed, zero failed and 16 declared ignored; Clippy with warnings denied
and formatting pass. Native coverage is 382/382 with zero missing or dangling
references. All seven KVM rows and the independent checker report
`release-eligible` on that exact clean source, with all 138 row artifacts
BLAKE3/length verified.

Full Nix remains failed at `local-multi-hypervisor-kvm-smoke`. Raft passes;
Rust-workload exhausts its six declared seeds with bootstrap
`HostWatchdogTimeout`, no candidate, and no replay verdict. These are operational
failures, not safety findings. The focused Octet report is accepted warning-only
with 3294 warnings and zero errors; both deny-all reports are clean. Their
nightly compiler/Dylint cohort is unchanged. Native readiness separately reports
`fail` at MCP smoke, despite command exit zero.

The receipt reuses the already-completed `b5f0a45` control/planted campaigns and
all eight depth-three replay verdicts with their original source and inputs.
It does not relabel them as `ab006a7` execution or substitute a fresh probe for
the selected original planted finding. All 37 historical receipts remain
byte-identical. Readable failed-Nix reports/logs are retained locally; 16
builder-owned 0600 payloads in the unregistered failed output are explicitly
inaccessible, not assigned fabricated digests. The standard Nix daemon archive
attempt rejects that unregistered path. No permissions or host configuration
were changed. The required-evidence-retention and production-readiness verdicts
remain false. All 51 corrective slots are consumed; no additional correction,
lifecycle archive or readiness promotion is implied.

## Kernel-only Rust-workload bootstrap correction (attempt52)

The user explicitly authorized exactly one further corrective attempt. The
continuing `dogfood-results/production-readiness-attempt52-20261020/attempts.ncl`
ledger preserves every original acceptance criterion and raises the ceiling to
52; no attempt53 is implied.

Actual guest-register samples from the failing profile interrupted KCOV hooks
called by `get_symbol_offset`. Linux 6.19's ftrace boot worker validates every
function record with `kallsyms_lookup`, which expands an unused symbol name.
The repository-owned `nix/linux-ftrace-offset-validation.patch` uses the existing
offset-only lookup for built-in addresses. Both paths use `get_symbol_pos`;
non-built-in addresses keep the original lookup and its module/BPF/ftrace
fallbacks. Lookup failure and the weak-function offset rejection are unchanged.
KCOV full instrumentation/comparisons and ftrace remain enabled. The complete
313532-byte kernel configuration is byte-identical before and after.

The controlled original Rust-workload command changes only the kernel artifact
and output directory. Its original executable, initrd, seed 42, 128 MiB memory,
single vCPU, workload inputs, 10000 bootstrap ticks, 240-second run envelope and
100000-microsecond KVM_RUN watchdog remain unchanged. It boots, retains two
intended probe findings, and completes in 32.660 seconds. Exit 1 denotes those
findings, not process success or a timeout. All six admitted sites are exercised:
five pass and only deliberate assertion 3143219316 fails. Both findings then
produce `snapshot_backed_reproduced` with a valid digest-verified depth-one
parent, in about 2.868 seconds each.

`correction-evidence.json` retains the exact commands, input comparison, assertion
outcomes, findings, parent artifact and both replay verdicts. Its BLAKE3 bindings
include the unchanged original executable/initrd. A four-core kernel build hit
its one-hour deadline during module compilation; the same derivation completed
with 24 compile cores under that same deadline. Both results remain recorded.

ChaosControl guest-kernel/VMM maintainers own the common kernel patch. Repeat
the packaged path with `nix run .#rust-workload-accepted-verdict-dogfood --
--output NEW_DIR`; preserve the cohort's declared bounds. These controlled
results remain distinct from the independent clean-source release gates below
and do not reproduce the original selected Raft finding.

The complete release gate run binds clean source
`19d2ecd42160eefc2bca0abb2d30c70f490bdea5`, not a later evidence-only commit.
Workspace all-target tests report 2223 passed, zero failed and 16 declared
ignored; denied-warning Clippy and formatting pass. Full `nix flake check -L`
passes, including both actual local multi-hypervisor workloads. Each workload
has a passing typed readiness report, all 13 static gates, an accepted
snapshot-backed reproduction and a matched expectation. The seven-row KVM
matrix and independent checker both report `release-eligible`; all 138 row
artifacts match their recorded BLAKE3 identities and lengths. Native coverage
is 382/382 with no missing or dangling links. Native readiness still reports
`fail` at `mcp_agent_smoke`; command exit zero does not make that gate pass.

The strict current-kernel watchdog proof completes in 296.974 seconds. A real
100-millisecond SI_TIMER interruption poisons the controller, replacement
restores the retained parent, and immediate incremental recapture matches.
Initial/final memory identities, strict execution records, every admitted
assertion outcome, fault ledgers and rounds match across long-lived, fresh-full
and replacement continuations. Each reports 44 passing, zero failing and one
unexercised site of 45. Depth-one through depth-four overlay payloads are
7991296, 10420224, 14163968 and 16244736 bytes. This remains an operational
watchdog event, not a guest finding or a new deep campaign.

The Nickel receipt in `production-readiness-attempt52-20261020/` binds these
commands, current inputs, complete outcomes and locally retained artifacts.
Independent verification checks all 239 current manifest entries and all 40
historical receipt files without changes. The receipt reuses the completed
`b5f0a45` depth-four campaigns and eight deep replay verdicts under their original
identities; no newly retained finding has parent depth three or greater. A
negative Nickel check rejects forcing `production_ready=true`.

Production readiness remains false. Required prerequisites are a compatible
fully gated Cairn CLI/export cohort admitted through the repository-owned
four-import migration and unchanged full MCP contract; the exact historical
Raft initrd `blake3:82b2ef606827644e8d10ca98142344e9fe5bae06cc3c11f837f5734654bf646e`
and accepted current-source reproduction of the original selected finding; and
the 16 inaccessible builder-owned failed-Nix payloads. Fresh probes do not
substitute for that finding. No permissions, acceptance criteria, historical
receipts or failed verdicts are changed. All 52 authorized corrective attempts
are consumed; no attempt53, lifecycle archive or readiness promotion is implied.


## Approved replacement planted-finding cohort (2026-10-25)

The operator selected "Reselect a current-source finding" and directed that the
sixteen inaccessible historical failed-build payloads be recorded as
unattainable. That approval covers campaign and replay execution plus
evidence-only updates. It does not authorize another product correction, and the
53-slot implementation ledger is unchanged. The historical original finding is
still unrecovered: the exact initrd remains absent and the current reader still
rejects its legacy parents. Neither result is rewritten here.

The cohort ran on clean `ef778be7ecc0467f176857dac779a262b9b741d1` with the
immutable release explorer built from that revision and the store paths
`kmykw8w23hlf752b098yv6phmv7cryzf-chaoscontrol-vmlinux` and
`nfv89frhg5fz8hfsshp0acw6j4zz6clb-chaoscontrol-initrd-raft`. The used kernel
resolves to `hsypp38yxmfrf49a0f5a24r0j50xxsiq-linux-6.19-dev/vmlinux`
(`blake3:0541ff0e…1d07ca`, 410793888 bytes); the initrd is
`blake3:30411f41…845048` (477079 bytes). Seed 42,
three VMs, one vCPU, 128 MiB, ten rounds, two branches, 500 ticks, quantum 100,
frontier 50, 10000 bootstrap ticks and the unchanged 600-second campaign
envelope are identical to the attempt47 declarations. Campaign outputs, boot
files and the explorer were copied to content-bound local paths before analysis.

| Campaign | Exit | Rounds | Branches | Depth | Retained | Replays | Wall (s) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `raft_bug=none` | 0 | 10 | 20 | 8 | 0 | — | 345.7 |
| `raft_bug=fig8_commit` | 0 | 10 | 20 | 8 | 0 | — | 381.9 |
| `raft_bug=snapshot_replay_probe` | 1 | 10 | 20 | 4 | 8 safety | 8 | 313.5 |

Exit 1 is the deliberate failure from the admitted probe; it is not a timeout.
All three advisory bounds completed well inside 600 seconds, no round reported a
host watchdog timeout, and no capture was left uncommitted. The control and
`fig8_commit` campaigns retained no finding, so neither liveness failure nor
operational timeout was reclassified as a safety result. Their deeper frontier
(8) does not pass or replace the ten-round/depth-four exploration requirement.

Observed dirty pages times 4096 give these overlay payloads; compressed snapshot
lengths are not substituted:

| Depth | Control bytes (captures) | Planted bytes (captures) | Probe bytes (captures) |
| --- | --- | --- | --- |
| 1 | 7815168–8122368 (2) | 7979008–8118272 (2) | 7753728–8245248 (2) |
| 2 | 10059776–10506240 (2) | 9822208–9928704 (2) | 9658368–10149888 (6) |
| 3 | 13041664–14143488 (4) | 13152256–14561280 (4) | 11132928–11644928 (10) |
| 4 | 15056896–17354752 (4) | 14979072–17596416 (4) | 11255808–11259904 (2) |

Each campaign produced twenty aligned captures; every trace-selected parent
resolved to the recorded child depth and no capture remained unmatched. The
assertion catalog is accepted and collision-safe for all three campaigns at 45
sites: control and planted report 44 passing, zero failing, one unexercised;
the probe reports 44 passing and the single deliberate failure at assertion
`3463273124` (`snapshot-replay-probe.v1`).

All eight retained probe findings have parent depth three and reproduced with
`schema_version` 2 `snapshot_backed_reproduced` verdicts. Each verdict matches
the exported bug, assertion, identity and parent-depth fields and names a
present, digest-verified parent snapshot. The selected replacement finding is
bug 0: assertion `3463273124`, parent depth 3, snapshot
`sha256:6855c3684836345a6fe76670815fa397bad545713be0ad260c570c38ea453245`
(`simulation-snapshot-cbor-zstd-v2`). Its diagnostic is
"BUG REPRODUCED — assertion 3463273124 failed".

This is a deliberate snapshot replay probe that trips only after restored parent
context. It is not a discovered Raft consensus defect, and it does not stand in
for the historical finding it replaces. Validate the receipt with `nickel export
--format json dogfood-results/production-readiness-planted-20261025/receipt.ncl`;
the computed `approved_review_ready` verdict covers only this reselected
evidence plus the already-retained `ef778be` release gates. `production_ready`,
`historical_original_reproduced` and `historical_failed_payloads_recovered` stay
false.
