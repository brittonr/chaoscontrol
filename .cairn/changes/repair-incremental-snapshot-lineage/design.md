# Design: Repair Incremental Snapshot Lineage

## Context

The guest memory contract for an incremental restore is: memory equals the base everywhere except the pages in `last_dirty_page_indices`. Three facts break that contract today.

1. `get_dirty_log` returns guest writes only. Host writes during restore are not logged.
2. `snapshot_incremental` reads the dirty log and captures only those pages.
3. `restore_incremental` reverts only the pages written by the previous restore.

The explorer runs sequences like: restore X, run, snapshot Y, restore X', run, snapshot Y'. Each Y misses X's pages that the branch did not rewrite. Each restore leaves the previous branch's guest writes in place. The long-lived explorer controller therefore runs on memory that neither the snapshot nor a fresh controller can reproduce.

## Decisions

### 1. Track the page set that differs from the base

The VM keeps one page set, `pages_differing_from_base`. It is the union of:

- the pages written by the last incremental restore (the restored overlay's pages), and
- the guest-dirty pages accumulated from every dirty-log read since that restore.

A full restore clears the set only when the restored memory equals the base. Otherwise the set becomes every page where the restored image differs from the base.

### 2. Capture uses the set, not only the dirty log

`snapshot_incremental` reads the dirty log, merges it into the set, and captures every page in the set. Y is then complete relative to the base regardless of lineage depth.

### 3. Restore reverts the set

`restore_incremental` first reads and merges the dirty log, reverts every page in the set to the base, writes the new overlay, and resets the set to the new overlay's pages. The dirty log is drained before the write so host writes cannot be confused with later guest writes.

### 4. Pure page-set planning

A pure function receives the current set, a dirty bitmap, and the target overlay page indices. It returns the revert list, the write list, and the next set. It checks page-index bounds against the guest size before any memory access. The shell performs the ioctls and memory writes.

### 5. Old deep evidence is diagnostic-only

Replay and readiness gates already require a valid parent snapshot. After this change they classify an older replay-parent snapshot at depth 3 or more as `blocked` when restore preflight rejects it. They do not attempt to repair old snapshots.

### 6. UART interrupt restoration preserves captured IRQ-chip state

The closure run observed an actual watchdog and exact parent-memory restoration, but fresh continuations diverged at COM1 interrupt-handler reads. `vm-superio::Serial::from_state` reasserts a pending UART interrupt; registering its new eventfd creates an additional edge even when the snapshot already contains the authoritative PIC state. Legacy irqfd delivery also depends on host work scheduling.

Keep the UART trigger as an allocation-free pending fact and deliver its IRQ synchronously at the handled guest-exit boundary. Restore the UART registers without reasserting the already captured interrupt edge. Retain strict execution, memory, and every-assertion comparisons; add a guest ISR regression that distinguishes a pending UART condition from an additional IRQ.

### 7. Inhibit the host-clock PIT timer, retain virtual-time IRQ delivery

Strict continuation diagnosis retained identical I/O payloads, assertion outcomes, faults, and rounds but different interrupt-stack addresses and final memory. Linux KVM's [`create_pit_timer`](https://github.com/torvalds/linux/blob/v6.18/arch/x86/kvm/i8254.c) arms a monotonic host timer from the reload interval; moving `count_load_time` into the future does not suppress it. Repeated `KVM_SET_PIT2` can rearm that timer.

Set `KVM_PIT_FLAGS_HPET_LEGACY` before execution and preserve it on every sync and common device restore. KVM cancels and flushes an existing PIT timer on the flag transition and refuses subsequent host-timer arming. This is the timer-inhibit interface, not an HPET device requirement. Software IRQ routing remains independent, so `DeterministicPit` continues delivering IRQ 0 at virtual-time deadlines. For compatible pre-fix snapshots, enforce the flag on a local restore copy without modifying serialized bytes or protocol identity. The behavioral regression observes no IRQ after host time advances, then requires IRQ 0 at the exact virtual deadline across fresh, retained, and incremental restoration.

### 8. Architectural HLT observation supplies deterministic idle progress

The first inhibited-PIT smoke stopped during Linux serial initialization: KVM's in-kernel LAPIC consumes guest HLT and blocks inside `KVM_RUN`, so no userspace HLT exit arrives to advance the software timer. Keep the same bounded operational timer before and after guest setup so the host regains control. An explicit `KVM_GET_MP_STATE == HALTED` observation, not the signal or elapsed host time, authorizes the single-vCPU virtual PIT deadline transition. Every non-halted single-vCPU interruption remains an operational `HostWatchdogTimeout`; do not retry it silently or label it a guest bug. Preserve the existing SMP scheduling and HLT policy.

Acceptance requires actual Linux boot with the host PIT inhibited, guest progress after architectural HLT, no virtual-time advance on a runnable timeout, and the unchanged watchdog probe's poison/replacement/memory/trace/all-assertion comparisons.

The exact-deadline guest regression rejected an initial implementation that applied the generic exit tick before observing the PIT: a 2,999,543-cycle deadline was delivered at 3,000,000. A single-vCPU architectural halt uses only the admitted virtual deadline, without first adding an unrelated exit-time quantum. The existing SMP branch retains its tick and period advancement.

The deadline transition must also bypass `VirtualTsc::advance_to`'s documented tick rounding while preserving clock suspension. The focused guest keeps exact-deadline and non-halted-timeout assertions; it does not require RIP to advance past an I/O instruction before KVM's userspace I/O completion protocol runs. A full depth-four smoke then observed the real watchdog, poisoning and replacement with exact parent-memory/recapture fidelity. Its strict execution traces, all admitted assertion outcomes, fault ledgers and round observations matched in all three continuations, but final memory did not. This remains a failed proof. On mismatch the proof retains final memory for byte-level diagnosis; the strict comparison stays unchanged.


### 9. Failed guest-kernel corrections remain unadopted

The strict proof localized the remaining final-memory mismatch to each VM's 32-byte Linux `irq_randomness.pool`: five comparisons differ in 32 bytes, one in 31. The kernel's own symbols, captured `__per_cpu_offset`, and `page_offset_base` resolve that pool to the exact differing physical address. Linux 6.19 can bypass runtime feature checks under `CONFIG_X86_TSC` in both `get_cycles()` and the separate `random_get_entropy()` hook. Disassembly confirmed unconditional `RDTSC` in `add_interrupt_randomness` under the admitted hide-TSC profile.

Attempts 18 and 20 tested repository-owned guest-kernel patches while preserving the pinned kernel and existing patch set. The first changed only `get_cycles()` and left the unconditional entropy read, so final-memory equality still failed. The second also guarded `random_get_entropy()`; its rebuilt disassembly contains the capability branch and Linux clocksource fallback, but the strict proof stopped during bootstrap with operational `HostWatchdogTimeout`. Both ten-round campaigns rejected their assertion summaries with `FatalConflict` / `IneligibleInput`; the selected probe emitted no accepted replay verdict.

The failed kernel correction was backed out within attempt 20, not adopted as a fix or counted as a passing proof. Its exact patch and integration source remain locally in `/tmp/chaoscontrol-production-readiness-closure-20261015/unadopted-kernel-corrections.tar`, BLAKE3 `253dba27cda0809035a0cb89a90250669b7e7af05b6abd9a698124b7f47a3bb9`. The retained implementation uses the unchanged reviewed guest kernel. No host kernel was changed, no entropy-security claim is made, and no memory was omitted from comparison. Full final-memory fidelity remains a blocker; every required strict comparison and declared bound remains in force.

### 10. Admit an entropy clock before collector warmup

The user authorized five additional corrections, extending the total ceiling to 25 without changing acceptance criteria. Read-only initcall tracing on the retained attempt-20 kernel localized its runnable bootstrap watchdog to `jent_mod_init`. Linux's `jent_entropy_init` allocates a collector whose warmup calls `jent_gen_entropy` before the later timer sanity checks. Under the admitted hide-TSC/jiffies clock, this warmup cannot collect a varying cycle counter; the VMM correctly retains an operational timeout rather than silently advancing a runnable guest.

Attempt 21 retained the runtime TSC capability guards and added a clock-capability preflight before jitter-entropy collector allocation. An unavailable or unchanged clock returns Linux's existing `JENT_ENOTIME` or `JENT_ECOARSETIME` result before warmup, without manufacturing entropy. A working counter retains the existing health tests and collection path. The pinned kernel, inherited patches, guest bounds, full memory comparison, and every assertion comparison are unchanged. The source-built kernel (BLAKE3 `51b571a44f01a46ba4e4e63b4e9dd26c93b301a2b6feac51ef7455111ec2fa2b`) passed the existing strict depth-four proof: actual bootstrap, real watchdog/replacement, restored and recaptured memory, final memory, strict execution identities, all admitted assertion outcomes, faults and rounds agree across all three continuations. `dogfood-results/production-readiness-followup-20261016/runtime21-observations.json` retains the actual passing report and exact commands. Paired campaign, selected replay, native readiness and clean-source release obligations remain separate; this result does not promote their non-pass reports.

### 11. Align declared Raft Linux boot clocks without extending bounds

Attempt-21 control and selected-replay runs still rejected assertion evidence. A paired diagnostic held the actual three-VM controller, seed 42, 128 MiB, quantum 100 and 10,000-tick bootstrap bound fixed. With the legacy TSC-visible profile, two guests reached Accepted 44-site catalogs but VM2 remained setup-incomplete with a Pending empty catalog at the bound; the merged report correctly rejected `IneligibleInput`. With the admitted hide-TSC profile, every guest completed setup by tick 1174 and all catalogs were Accepted. This is a bootstrap clock-path failure, not an assertion collision or a guest safety finding.

Attempt 23 exercised the existing Linux `clocksource=jiffies notsc` boot policy through the reviewed Raft workload command-line inputs. All three guests completed the actual bootstrap by tick 1175 with Accepted 44-site catalogs. Both ten-round campaigns then completed with accepted collision-safe assertion summaries, measured overlays through depth ten and no retained safety finding; the control took 142.818 seconds and planted campaign 145.071 seconds under the unchanged 600-second bound. Explicit and newly compiled implicit-default selected probes both emitted accepted snapshot-backed reproduction verdicts. This does not hide CPU features globally, change the public VM default, fabricate a completed guest, or change any declared campaign/replay bound. Retain the existing kernel correction and all identity/admission checks. Historical unavailable-initrd and actual deep-finding replay obligations remain distinct from these newly selected depth-one replays.

### 12. Require an actual deep exploration finding replay

The active `incremental_validation` requirement needs an actual finding at replay-parent depth three or more. Keep the ten-round control and `fig8_commit` campaigns and the fail-after-one release cohort unchanged. Attempt 24 uses the existing Rust guest's snapshot probe in a separately labeled campaign, with a 6000-guest-tick threshold and the same seed, topology, faults, ticks, branches, rounds and operational bound as the reviewed ten-round profile. The completed earlier campaign reached guest tick 9742 within those bounds. Export and replay every retained safety finding whose actual parent depth is at least three; require the real replay verdict, retained parent and identity checks. A shallow finding or a missing deep finding cannot close this requirement. This tests snapshot mechanics, not a discovered Raft algorithm defect.

### 13. Compose the local KVM smoke gate with actual proof verdicts

The retained local multi-hypervisor report distinguishes a Raft child killed by a generic 30-second deadline from a Rust child exiting 2. Its scheduler successfully emitted a failed report; the smoke command and Nix check accepted that process/file outcome. Attempt 25 removes the incompatible generic child deadline, keeps the existing 900-second enclosing campaign bound and all per-phase proof bounds, and requires successful bounded scheduler execution plus every validated workload report passing. Both default workloads remain. `local-kvm25-observations.json` records three passing regressions and actual failed-report rejection: smoke exit 1 despite scheduler exit 0 and a valid failed campaign. The raw scheduler observation remains distinct from the smoke verdict. Default-workload KVM execution and exact clean-source release gates remain separate requirements; the ongoing earlier clean-revision run stays unchanged.



### 14. Advance each accepted staged control prefix without replaying earlier prefixes

The retained read-only timing diagnosis isolates 234 actual control events whose
held-projection publication pattern applies 27,495 suffix events in 124.12 seconds.
A single pass applies the same 234 controls in 1.01 seconds with equal recorded
pre-states and final complete state. This is bounded diagnosis, not a production
speedup or attribution of the whole campaign timeout.

Attempt 46 advances the existing validated `ControlSession` projection after
each accepted publication using the existing `seal` validation and independent
applied-state comparison. The next publication therefore starts at the accepted
prefix instead of replaying every earlier publication in the staged segment.
Segment sealing, immutable publication planning, history/payload/identity checks,
selection fences, resource reserves, journal durability and adopt-after-commit
remain required. A staged validation failure is not a committed publication.

The user authorized this one focused attempt with “do it” after the explicit
proposal to exceed the retained 45-attempt ceiling by one. The original campaign
bounds, depth-four/overlay and retained-finding replay requirements, strict
controller proof, all release gates and ownership restrictions remain unchanged.
No sibling correction, host change or unrelated implementation is authorized.


### 15. Bound frontier multiplicity by logical marker, not repeated observations

The completed attempt46 traces retain fifty marker proposals from round one's
single probe snapshot. Original-score pruning removes every depth-three proposal
(1400 control, 1384 planted, 1092 deliberate probe); selection-count decay does
not affect that retention dimension. The declared profile permits depth eleven,
so neither the snapshot implementation nor an execution bound explains this
observed depth-three ceiling.

Attempt47 will coalesce a completed branch's repeated logical marker/owner
observations into ranked representatives of that same restorable parent. Keep
distinct logical marker identities and owners, exact selected state/position refs,
every oracle event and coverage fact, and deterministic rarity/novelty scoring.
The shared rank conversion, selection-count decay, capacity pruning, entropy,
candidate admission, publication and snapshot/replay validation remain unchanged.
Product proposal normalization is shared by both frontier backends; do not change
Campaign decisions or invent a second selection policy.

The user explicitly selected "Authorize attempt 47" after the read-only diagnosis.
This is one additional cohesive owner-local correction and verification, not
authority for extra rounds, wider timeouts, depth relabeling, sibling/host changes,
unrelated compatibility repairs or readiness promotion. The native marker and
Campaign parity contracts remain required. Run the actual unchanged campaigns
and replay every retained safety finding whose parent depth is at least three;
do not substitute new probes for the selected historical finding.

Measured outcome on clean `b5f0a4550f86cfa3f5b9f45206a57d4087225182`: the
unchanged control, planted and deliberate-probe profiles completed ten rounds and
twenty branches in 280.479, 283.577 and 307.813 seconds, reaching maximum admitted
depths 8, 8 and 4. All eight retained depth-three safety findings reproduced with
valid retained parents and `snapshot_backed_reproduced` verdicts. Three fresh
strict watchdog attempts failed only as operational `HostWatchdogTimeout` results
under host load of 34–44 on 32 CPUs; the unchanged proof still passes on its
attempt46 source `ad7e892`. The selected original planted finding remains
unavailable because its exact historical initrd is missing. Fresh native
traceability still reports 103 missing requirements. Production readiness stays
blocked; the correction itself, depth-four campaign lineage and retained-finding
replay are verified.

### 16. Keep dependency-policy denial controls aligned with the Radicle cutover

The clean-source release rerun on `29329c8` passes all seven KVM matrix rows but
fails the actual Cargo-deny negative controls: Campaign and Choregraph now resolve
through Radicle, while two historical HTTPS allowances remain. Replacing either
unused allowance cannot deny the locked graph, so the negative control correctly
fails instead of manufacturing a denial.

The explicit additional-attempt selection authorizes one cohesive correction:
remove those obsolete allowances and their no-op mutations. Keep unknown-source
denial, the four active source mutations, both license-exception mutations, exact
mutation counts, expected diagnostics and the existing 120-second control bound.
Run the real Nix dependency-policy gate, then the full Nix check and a fresh
seven-row matrix on an exact clean revision. Do not weaken a checker, skip a
control, extend bounds, alter a sibling or reconfigure a host. Historical receipts
and the missing original replay input remain separate evidence boundaries.

### 17. Three separately counted owner-local release corrections

After the attempt48 publication, the user explicitly selected "Authorize three
additional attempts". The continuing ledger grants a maximum of 51, preserving
all previous ledgers rather than retroactively changing their authority.

Attempt49 isolates assertions over process-global signal state in independent
test subprocesses. Keep production atomics and signal semantics unchanged;
retain the original failing flag test, remove the duplicate test that never ran
an explorer, and replace installation-only non-crash coverage with observable
first-signal/second-signal transitions. Do not serialize the workspace suite.

Attempt50 addresses the actual Octet Cargo client route. The failing consumer
derivation prepends a wrapper for Cargo 1.96.0-nightly, while the existing
source-client repair is scoped to Cargo 1.98.0. Any integration must retain the
exact nightly compiler and Dylint ABI and genuine policy checks. Review the
supported input/consumer API before choosing a scoped repair; no sibling edits.

The pinned helper has no Cargo injection parameter, and the packaged runner
prefixes its own Cargo path after the check's setup. Use guarded `overrideAttrs`
on the original runner's wrapper and original check's invocation: prepend the
already-tested repaired Cargo client within that runtime path, preserving every
other tool/artifact/argument. Its wrapper appends the stable rustc directory,
so the original nightly rustc remains first. Match the exact pinned wrapper and
invocation forms and fail evaluation if the integration seam changes. Exercise
the real Octet checks; do not replace them with a copied checker or fixtures.

Attempt51 first traces the explorer child around its unchanged 240-second
deadline and one-second teardown allowance. The outer 3600-second matrix row
limit is not the observed boundary. A retained frontier stop is not process
completion, and no discarded captured output or host timeout is a guest verdict.
Repair only a demonstrated cause; do not increase bounds or suppress errors.

The immutable seed-zero trace reaches round zero and a retained snapshot artifact
but misses the 240-second deadline. Its explorer unmaps 150 guest-memory-sized
buffers (about 39 GiB in total). Source inspection identifies full replay-parent
clones for each failure and even for dashboard counts, followed by repeated
encoding of the same parent for each checkpoint bug. Share each round's immutable
replay parent through `Arc`, iterate borrowed bugs for counts/checkpoint creation,
and encode each shared parent/depth pair once per checkpoint. Pointer identity is
only a checkpoint-local memo key, never a persisted identity or authority; retain
the existing canonical encoding, digest, admission and replay checks. Keep depth
in the key so references cannot alias across lineage depths. Exercise the actual
launcher under its original deadlines after the repair; the diagnostic trace is
not a passing release result.

Fresh workspace/Nix/Cairn and seven-row matrix results must bind their exact
clean source. The original planted finding still requires its original initrd,
and a supported Cairn CLI/export cohort remains an independent prerequisite.

## Risks

- Overlay size and restore cost grow with lineage depth. Validation records overlay page counts per depth for the Raft and redb workloads.
- The explorer's controller-replacement path (`isolate-branch-watchdog-timeouts`) writes the base and then restores incrementally. After this change it matches the long-lived controller exactly. Validation must compare the two directly.
