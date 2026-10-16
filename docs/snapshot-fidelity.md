# Snapshot fidelity and compatibility

ChaosControl snapshots use the `exact-x86-kvm-v1` completeness profile.
The current state schema version is `2`.

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

Capture fails if a device operation is in progress, a device is failed, schedule evidence is pending, the VM is poisoned, or a backend has no exact snapshot adapter. Capture also fails if KVM cannot return every required vCPU state group.

## Incremental lineage

An incremental image is a complete overlay relative to its retained base, not just the latest KVM dirty log. Capture includes inherited overlay pages and subsequent guest writes. Restore reverts both the previous overlay and guest writes made since the last capture, then installs the selected overlay. A sibling therefore cannot inherit uncaptured writes from another branch.

`DeterministicVm::get_dirty_bitmap` requires mutable access because observing the KVM dirty log consumes it. The VM retains those observed pages for later incremental capture and restore. Full restore of an overlay installs its base as well as its dirty pages. A later change of incremental base reconciles the current memory against that base before using the page set.

Page-set planning is pure and bounded by guest memory size. Invalid bitmap shapes and page indices reject before page access. Tracking is conservative: a page can remain tracked after its bytes return to the base value. The persisted snapshot codec and schema remain unchanged.

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

Single-vCPU bootstrap and admitted execution both have the existing bounded operational watchdog. After an interrupt, only KVM's architectural `HALTED` state authorizes idle progress to the exact next virtual PIT deadline; a runnable timeout does not advance time or become a guest bug. SMP scheduling remains separate. A strict continuation mismatch retains BLAKE3-bound final VM memory under each local continuation directory for byte-level diagnosis; none of that memory is omitted from the equality decision.

## Restore rules

Restore first runs a read-only preflight. Preflight checks the schema, profile, inventory, topology, memory shape, vCPU state, scheduler state, serial bounds, fault state, and every virtio transport and backend adapter.

A preflight failure does not change VM state. A failure after restore starts permanently poisons the VM. A simulation restore failure after mutation starts also poisons the controller. The caller must discard a poisoned VM or controller.

## Compatibility policy

Schema `2` restores only into the same exact profile and matching VM topology. The snapshot records the sorted `KVM_GET_MSR_INDEX_LIST` inventory. The restore host must expose that same inventory and support all required KVM state operations.

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

