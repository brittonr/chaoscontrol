# ChaosControl — Deterministic VMM

A deterministic Virtual Machine Monitor (VMM) for x86_64 built with KVM
and the rust-vmm crate ecosystem. Designed for simulation testing of
distributed systems where reproducibility is essential.


This is just an experiment with Claude + Pi.dev. Use at your own risk

<!-- product-scope-facts:start -->
> **Product scope:** 4 supported, 6 experimental, 1 deferred, 1 blocked, and 3 non-goal capabilities.
>
> The workspace has 21 crates from `Cargo.toml`. The replay manifest has 4 historical workload rows.
>
> The selected Cargo command owns the test inventory. This projection does not copy a test count. The authority is `cargo test --workspace --all-targets -- --list`.
>
> Generated facts do not prove correctness, release eligibility, hosted support, or universal determinism.
<!-- product-scope-facts:end -->

## Features

### Deterministic Execution
- **CPUID filtering**: Comprehensive filtering removes RDRAND, RDSEED,
  RDTSCP, optionally AVX2/AVX-512, and hides hypervisor presence
- **Pinned TSC**: Fixed time stamp counter frequency (default 3.0 GHz)
  for reproducible timing across hosts
- **Virtual TSC**: Software TSC counter that advances only on VM exits,
  enabling fully deterministic time progression
- **Fixed processor identity**: Optional model/family/stepping override
  for cross-host reproducibility
- **SMP support**: Multi-vCPU VMs with serialized execution (Antithesis-style),
  deterministic round-robin or randomized scheduling

### Deterministic SMP Progress

ChaosControl uses `ProgressMode::ExactSingleStep` for SMP by default. KVM guest debug returns control after each guest instruction.
The scheduler switches vCPUs only at the declared instruction quantum.

`ProgressMode::PmuAccelerated` is an explicit opt-in mode. It uses a guest-only instruction counter, then single-steps the exact remainder.
Startup fails if the PMU, overflow delivery, or exact single-step capability is unavailable. The VMM does not use timer-only fallback.
PMU overflow signals target the execution thread with `F_SETOWN_EX` and `F_OWNER_TID`.

`SIGALRM`, unrelated `VcpuExit::Intr`, and unrelated `EINTR` have no scheduling authority.
A watchdog timeout is an operational result. It does not prove a guest crash, deadlock, or deterministic replay result.

`VmConfig::smp_schedule_journal_limit` sets the in-memory evidence bound. The value cannot exceed `DEFAULT_SCHEDULE_JOURNAL_LIMIT`.
Each accepted record contains canonical pre-state and post-state BLAKE3 identities.
Recordings preserve these traces by VM and simulation tick. Exact replay rejects missing, forged, reordered, or divergent traces.

The VM becomes permanently poisoned if guest progress can occur without exact evidence or post-commit exit handling fails.
A failed controller round also creates a permanent controller poison after mutation starts.
The controller then rejects execution, mutation, snapshots, restores, recording output, and success results.
Partial VM journals remain available only as diagnostics. The controller never publishes the failed round as complete.

`KVM_EXIT_HLT` leaves the userspace MP state runnable on the tested KVM host.
ChaosControl therefore keeps a replay-stable HLT latch and clears it only after an explicit deterministic wake.

The portable claim is bounded to KVM hosts that support exact guest debug single-step.
PMU acceleration has a narrower, host-specific capability profile.

### Runtime capacity

ChaosControl admits selected runtime capacity before VM activation. The plan covers schedule records, virtio scratch buffers, retained TX packet slots, and TX queue metadata.

The pure core validates the plan and its BLAKE3 identity. The VM reports bounded startup, usage, high-water, exhaustion, release, and leak observations.

This evidence does not claim deterministic latency, complete process-wide allocation removal, zero-copy I/O, or host memory guarantees. See [deterministic runtime capacity](docs/runtime-capacity.md).

Run the focused KVM evidence test with:

```bash
nix develop -c cargo test -p chaoscontrol-vmm --test deterministic_smp_kvm
```

### VM Infrastructure
- **x86_64 boot**: Full long mode setup with GDT, identity-mapped page
  tables (1 GB via 2 MB pages), and Linux boot protocol support
- **In-kernel IRQ chip**: PIC, IOAPIC, and LAPIC via KVM
- **Serial console**: COM1 with interrupt-driven I/O and output capture
- **Linux kernel support**: Loads ELF kernels via linux-loader
- **ACPI tables**: RSDP/RSDT/MADT for SMP CPU topology

### Snapshot / Restore
- **Complete state capture**: CPU registers, FPU, debug registers, LAPIC,
  XCRs, IRQ chip (PIC master/slave, IOAPIC), PIT, KVM clock, and full
  guest memory
- **Instant restore**: Resume execution from any captured checkpoint
- **Fork support**: Create divergent execution paths from a single
  snapshot point
- **Copy-on-write block device**: Snapshots share the base disk image
  via `Arc`; only dirty 4 KB pages are cloned — a 512 MB disk with 1 MB
  of writes costs ~1 MB per snapshot, not 512 MB

### Shared cohort mechanics

ChaosControl selects VM Cohort for retained initialized bases, private overlays, clone lifecycle, KVM descriptors, and cleanup.

ChaosControl keeps snapshot, fault, scheduler, assertion, coverage, exploration, replay, guest, evidence, and release meaning.

The old duplicate path is diagnostic rollback only. It is not an automatic or release fallback.

See [the VM Cohort adoption boundary](docs/vm-cohort-adoption.md) for pins, mapping, parity, verification, and non-claims.

### Deterministic Devices
- **Entropy**: Seeded ChaCha20 PRNG replacing hardware RNG, with
  snapshot/restore and reseed for exploration
- **Block**: Copy-on-write block device with optional disk image file
  backing (`--disk-image`). Supports fault injection (read errors,
  write errors, torn writes, corruption)
- **Network**: Simulated network with RX/TX queues, latency, jitter,
  bandwidth limiting, packet loss/corruption/reorder/duplication for
  fully controlled packet delivery between VMs

### Exploration & Bug Finding
- **Coverage-guided exploration**: AFL-style edge coverage bitmaps,
  fork-from-snapshot branching, frontier-based search
- **Three exploration modes**: fault-schedule mutation, input-tree
  branching at `random_choice()` points, or hybrid
- **Deterministic schedule diversity**: SMP branches can carry explicit seeded scheduler variants. Bug and replay evidence binds the exact policy. See [`docs/schedule-diversity.md`](docs/schedule-diversity.md).
- **Fault schedule minimization**: Delta debugging (ddmin) to find
  the smallest schedule that triggers a bug
- **Bug reproduction**: Replay a bug report to verify it still triggers
- **Assertion catalog**: Compile-time registration of all assertion sites
  via `linkme`; reports show which assertions are exercised/unexercised
- **Per-round history**: Coverage growth curves, plateau detection,
  bug discovery timeline

### Pure Simulation Core
- **Machine-independent decisions**: `chaoscontrol-sim-core` owns scheduling, virtual time, network transitions, fault selection, and canonical round traces.
- **Typed shell boundary**: The core emits commands and validates observations. `chaoscontrol-vmm` retains KVM, device, filesystem, and snapshot effects.
- **Bound identities**: Round traces bind the seed, deterministic configuration, and exact BLAKE3 guest artifact identities.

### Determinism Logging
- **Binary dlog format**: Per-exit event log for diagnosing non-determinism
- **Structural diff**: Compare two runs ignoring data payloads
- **Register dumps**: Periodic full-register snapshots in dlog
- **Memory hashing**: CRC32 page hashes at snapshot boundaries

## Project Structure

```
chaoscontrol/
├── flake.nix                              # Nix development environment
├── Cargo.toml                             # Workspace root
└── crates/
    ├── chaoscontrol-protocol/             # SDK ↔ VMM wire protocol (no_std)
    ├── chaoscontrol-sdk/                  # Guest-side SDK (Antithesis-style)
    ├── chaoscontrol-fault/                # Host-side fault injection engine
    ├── chaoscontrol-sim-core/             # Pure simulation decisions and traces
    ├── chaoscontrol-vmm/                  # KVM and machine-effect shell
    ├── chaoscontrol-vm-cohort-adapter/    # VM Cohort consumer and exact restore adapter
    ├── chaoscontrol-dashboard/            # Web dashboard backend and static UI
    ├── chaoscontrol-explore/              # Coverage-guided exploration engine
    ├── chaoscontrol-replay/               # Recording, replay, time-travel debugger
    ├── chaoscontrol-snapshot-descriptor/  # Pure portable snapshot identity and preflight
    ├── chaoscontrol-evidence/             # Typed evidence/readiness models and gates
    ├── chaoscontrol-trace/                # eBPF/ftrace tracing
    ├── chaoscontrol-guest/                # Minimal SDK-instrumented guest binary
    ├── chaoscontrol-raft-guest/           # 3-node Raft consensus guest (35 assertions)
    ├── chaoscontrol-guest-net/            # Network guest library (smoltcp)
    └── chaoscontrol-net-guest/            # Network demo guest binary
```

### Kernel Coverage (KCOV)

When the guest kernel is built with `CONFIG_KCOV=y`, the SDK
automatically collects kernel code coverage and merges it into the
same AFL-style bitmap used by userspace SanCov.  This gives the
explorer visibility into kernel code paths exercised by different
fault schedules — filesystem error handling, network stack branches,
scheduler decisions, etc.

```bash
# Build KCOV-enabled kernel (first time takes ~20 min)
nix build .#kcov-vmlinux -o result-kcov

# Run exploration with kernel coverage
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel result-kcov/vmlinux --initrd guest/initrd-raft.gz \
  --vms 3 --rounds 200 --branches 16

# Guest SDK auto-detects KCOV — no code changes needed
```

On a standard kernel (without `CONFIG_KCOV`), the SDK gracefully
falls back to userspace-only coverage — no crash, no error.

## Building

```bash
# Enter development environment
nix develop

# Build VMM + tools
cargo build

# Run the current test inventory
cargo test

# Build guest binaries (statically linked, musl)
nix build .#guest-sdk            # → result/bin/chaoscontrol-guest
nix build .#guest-raft           # → result/bin/chaoscontrol-raft-guest
nix build .#guest-net            # → result/bin/chaoscontrol-net-guest
nix build .#guest-rust-workload  # → result/bin/chaoscontrol-rust-workload-guest

# Build initrd images (from guest binaries)
nix build .#initrd-sdk   # → result (gzipped cpio)
nix build .#initrd-raft
nix build .#initrd-net
nix build .#initrd-rust-workload

# Build custom kernels
nix build .#net-vmlinux       # virtio-net enabled
nix build .#kcov-vmlinux      # KCOV coverage
nix build .#kcov-net-vmlinux  # both

# Boot a kernel
cargo run --bin boot -- <kernel-path> [initrd-path]

# Snapshot demo
cargo run --release --bin snapshot_demo -- <kernel-path> <initrd-path>
```

## Kernel-bundle compatibility smoke

ChaosControl has an opt-in, bounded KVM rail for one exact admitted Onix/Mantle
kernel-bundle cohort. It builds a repo-owned initrd, verifies expected versus
measured kernel/initrd BLAKE3 identities before launch, and records structured
boot, module, BPF, and cleanup observations. Transcript-only markers cannot pass
this behavior rail. Bounded Tree observes and revalidates initrd source trees,
while ChaosControl retains Newc and evidence semantics. See
[the kernel-bundle validation runbook](docs/kernel-bundle-validation.md) and
[the Bounded Tree adoption boundary](docs/bounded-tree-adoption.md) for
supported inputs, reproduction, retention, rollback, and non-claims.

## CLI Tools

### Quick Start (Nix)

```bash
# Run Raft exploration with one command (builds kernel + guest + initrd)
nix run .#explore-raft

# Run the Rust workload harness as a local instrumentation dry-run.
# This writes sdk.jsonl plus report.json with registered-vs-observed
# per-assertion coverage and does not claim replay proof.
nix run .#rust-workload-local-report -- /tmp/cc-rust-workload-local

# Run the Rust workload harness in a bounded VM campaign.
# This writes campaign output plus evidence-classification.json.
nix run .#explore-rust-workload -- /tmp/cc-rust-workload-vm

# Run with non-duplicate custom args (the wrapper already sets rounds/branches/ticks)
nix run .#explore-raft -- --output results/ --extra-cmdline 'raft_bug=fig8_commit'

# To override wrapper defaults, call the explorer directly with explicit kernel/initrd paths.
```

### Exploration

```bash
# Coverage-guided exploration
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --vms 3 --rounds 200 --branches 16 --output results/

# With persistent disk image
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --disk-image <path-to-ext4.img> \
  --vms 3 --rounds 200 --branches 16 --output results/

# Input-tree mode (branch at random_choice() points)
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --mode input-tree --output results/

# Resume from checkpoint
cargo run --release --bin chaoscontrol-explore -- resume \
  --corpus results/ --rounds 500

# Finalize an interrupted checkpoint that already contains bugs but no bug_N.json files
cargo run --release --bin chaoscontrol-explore -- export-bugs \
  --checkpoint results/checkpoint.json --output results/

# Finalize only targeted snapshot-backed replay candidates. The checkpoint and
# every bug identity are validated before filtering or writing any bug file.
cargo run --release --bin chaoscontrol-explore -- export-bugs \
  --checkpoint results/checkpoint.json --output results/ \
  --assertion-id <current-v2-compatibility-id> --min-replay-parent-depth 1 --max-bugs 1
```

Output directory contains:
- `checkpoint.json` — resumable exploration state; checkpoint saves now persist replay parent snapshot refs for bugs when a parent snapshot is available
- `report.txt` — human-readable report with per-round history
- `assertions.json` — a closed v2 summary with exact descriptors, fingerprints, catalog tokens, verdicts, and counters
- `bug_N.json` — catalog-bound bug reports for minimize/reproduce; ID-only historical bugs are diagnostic-only
- `snapshots/<sha256>.snapshot.bin` — bounded, hash-addressed, zstd-compressed CBOR v2 `SimulationSnapshot` artifacts; live replay rejects legacy codecs
- `run-config.json` and `receipt.json` — contract-backed review inputs generated with `materialize-dogfood-receipt`
- `replay-verdict.json` — optional Rust-owned machine-readable reproduce/smoke verdict emitted by `reproduce --verdict-output`

### Assertion identity and migration

Automatic assertion macros use the namespace `build:<package>:<version>`.
Their logical keys include the exact source file, line, and column. A package
version change or source move creates a new automatic identity.

Use the `*_stable` assertion macros when an assertion needs an explicit
namespace and logical key. The namespace and key remain the logical identity.
Changes to the kind, message, source site, guest, or category change the full
fingerprint. Two different descriptors for one logical identity are a fatal
catalog conflict.

The old public `u32` assertion APIs and compatibility wire aliases are removed.
Use automatic macros or explicit stable namespace/key macros. Old compiled
clients and unbound assertion events are unsupported and fail closed. Removed
commands `0x05` and `0x07` return the unknown-command error.

Bounded readers can still identify `legacy_u32` in historical serialized input.
They quarantine or reject that input. A legacy identity cannot enter strict
catalogs, counters, readiness, replay, merges, or accepted v2 summaries.

The unbound guidance API and command `0x07` are also removed. Future guidance
must bind to an exact catalog token and descriptor fingerprint.

The live oracle has no integer recording path. It updates counters only for
catalog-bound fingerprints. Integer aliases can select one record only after
the complete structured report and its collision-safe claim validate.

Runtime restore accepts only pristine pre-catalog state or validated structured
state. Diagnostic legacy and fatal snapshots remain readable but not restorable.

ChaosControl uses BLAKE3 fingerprints to bind canonical descriptor bytes. This
binding detects known mismatches and collisions. It does not prove that a hash
collision is impossible.

### Contract-backed evidence

Nickel contracts live under `contracts/evidence/` and `contracts/kvm-release/`. Nickel owns human-authored VM run, simulator, campaign, fault-schedule, SMR workload, eBPF capture, and KVM release profiles. Rust revalidates external projections and owns runtime records, outcomes, reports, receipts, execution, and replay.

Use `check-profile-projections --root .` to check projection freshness. Use `--write` only during the explicit preparation workflow. The receipt binds source, imports, contract, evaluator, profile, and projection identities with BLAKE3. Nickel is not invoked in simulator, campaign, or replay hot paths. See `docs/simulator-campaign-profile-boundary.md` for the field inventory and non-claims.

Raw `run.log` and `reproduce.log` files are debug-only. They are excluded from the acceptance record. Replay parent snapshot references are Rust-derived refs with a store, digest, codec, schema version, and confined path. The optional redb store or index is host-side only. It is not a public evidence format.

Acceptance statuses are:
- `accepted` — the receipt and replay evidence are complete and the reported bug reproduces.
- `snapshot-backed-proof` — a retained bug has `replay_parent_depth > 0`, a valid `replay_parent_snapshot_ref`, and reproduce/minimize evidence loaded the persisted parent snapshot.
- `schedule-only-gap` — exported bugs have `replay_parent_depth = 0`; useful as replay-gap evidence but not proof of snapshot-backed replay.
- `no-bug-coverage-gap` — the workload ran without bugs; useful only when paired with bounded run configuration and assertion coverage.
- `missing-artifact-skip` — reproduce/minimize was skipped or failed early because a required snapshot artifact was absent or invalid.
- `partial` — the run is useful but does not satisfy every acceptance condition.
- `known-gap` — the run exposed a documented product/evidence gap; the Raft dogfood receipt uses this for the non-replaying `bug_0.json`.
- `invalid` — the receipt or artifacts fail contract validation.
- `raw-log-only` — only debug logs exist; this is not acceptance evidence.

The repeatable KVM smoke gate for this rail is:

```bash
nix build .#checks.x86_64-linux.snapshot-replay-smoke --no-link -L
```

It runs one bounded Raft `snapshot_replay_probe` branch. It validates all checkpoint bugs before filtered export. A Rust validator checks exact bug identity, schema-v2 verdict semantics, current snapshot format, artifact hashes, and replay linkage. Raw logs remain temporary.

### Required KVM release matrix

Portable CI does not establish KVM behavior. The separate KVM release lane runs the typed matrix in `contracts/kvm-release/matrix.ncl` on an admitted worker.

```bash
revision="$(git rev-parse HEAD)"
nix run .#kvm-release-matrix -- \
  --root . \
  --matrix contracts/kvm-release/matrix.json \
  --out target/kvm-release \
  --expected-revision "$revision"
```

Every required row must pass. Missing, stale, dirty, skipped, unsupported, timed-out, failed, or tampered evidence blocks the verdict. The validated seven-row run is summarized in `dogfood-results/kvm-release-evidence-20260809/validation-receipt.json`. See [the KVM release evidence guide](docs/kvm-release-evidence.md).

### State-machine property lanes

Portable CI runs bounded generated command sequences against independent scheduler, snapshot, fault, assertion, virtio, and evidence models. A scheduled deep lane uses larger finite limits. Both lanes retain typed receipts and stable minimized regression fixtures.

```bash
nix build .#checks.x86_64-linux.property-coverage -L
```

These lanes report only bounded model agreement. They do not prove complete correctness or KVM behavior. See [the state-machine property guide](docs/state-machine-property-coverage.md).

For a single operator-facing readiness button, run:

<!-- replay-readiness-status:start -->
> **Replay readiness checks:** `replay-readiness status=passed exit=0 static_gates=12/12 failed_gates=none dogfood=skipped failed_phase=none scope=bounded`
>
> This status reports bounded static gate execution. Historical workload rows remain blocked until fresh admitted v2 KVM evidence exists. A passed status does not promote a workload. It is not a claim of universal determinism.
<!-- replay-readiness-status:end -->

```bash
nix run .#replay-readiness
```

This checks the contract registry, evidence contracts, historical proof manifest, current promotion classifications, consistency fixtures, generated reports, artifact limits, and dogfood wrapper configuration. CI and dashboards can request a machine-readable operator receipt:

The consistency-checker fixture gate validates typed operation histories and bounded semantic reports under `dogfood-results/consistency-checker-fixtures/`; these reports are semantic workload evidence only and explicitly do not imply deterministic replay proof, assertion-readiness coverage, or hosted-product parity.

```bash
nix run .#replay-readiness -- --receipt "$PWD/target/replay-readiness-receipt.json"
```

The receipt records final status, static gate outcomes, optional selected dogfood workload, dogfood output path plus compact post-run summary when available, the selected workload expectation and expectation match status, failure phase when applicable, and the scoped anti-claim that this is bounded committed replay/evidence readiness rather than universal determinism or hosted-product parity. To emit a one-line CI/dashboard summary from a saved receipt, run:

```bash
nix run .#replay-readiness-summary -- "$PWD/target/replay-readiness-receipt.json"
```

It prints a stable line such as `replay-readiness status=passed exit=0 static_gates=12/12 failed_gates=none dogfood=skipped failed_phase=none scope=bounded` and fails closed on malformed receipts. The README status block above is generated from the same receipt summary and can be refreshed with:

```bash
nix run .#replay-readiness-readme-status -- "$PWD/target/replay-readiness-receipt.json" --readme README.md
```

When a dogfood run emitted wrapper summaries, the `dogfood=` token expands with accepted/seed/fail-after/replay-class/depth fields so operators can triage the KVM proof without opening raw attempt dirs. To render the same receipt as a static operator dashboard, run:

```bash
nix run .#replay-readiness-dashboard -- "$PWD/target/replay-readiness-receipt.json" \
  --output "$PWD/target/replay-readiness-dashboard.html"
```

The dashboard is a self-contained HTML artifact that shows final status, static gates, selected dogfood expectation/replay-class details, raw receipt JSON, and the bounded-readiness scope string. To render the same receipt into a local operator triage runbook that opens committed bug/replay artifacts, gives reproduce/minimize commands, and records decisions without raw-log scraping, run:

```bash
nix run .#replay-readiness-triage -- "$PWD/target/replay-readiness-receipt.json" \
  --root "$PWD" \
  --output "$PWD/target/operator-triage-runbook.md"
```

The committed baseline runbook lives at `docs/operator-triage-runbook.md` and can be checked with `cargo run -p chaoscontrol-evidence --bin replay-readiness-triage -- --root . --sample-receipt --check docs/operator-triage-runbook.md`. For multi-run artifact review, render a static fleet triage index from one or more saved replay-readiness receipts without promoting it to hosted UI or shared decision-store status:

```bash
nix run .#replay-readiness-fleet-index -- --output "$PWD/target/fleet-triage-index.html" \
  "$PWD/target/replay-readiness-receipt.json"
```


To persist a bounded local operator decision next to that artifact, write and validate a decision receipt. This is a review artifact format, not a hosted/shared decision store:

```bash
nix run .#replay-readiness-decision-receipt -- --sample \
  --output "$PWD/target/replay-readiness-decision-receipt.json"
nix run .#replay-readiness-decision-receipt -- --check \
  "$PWD/target/replay-readiness-decision-receipt.json"
```

The CI/check surface packages the receipt, summary, dashboard, triage runbook, fleet index, and local decision receipt artifacts with:

```bash
nix build .#checks.x86_64-linux.replay-readiness --no-link -L
```

For real local KVM multi-hypervisor evidence, run the packaged smoke rail:

```bash
nix run .#local-multi-hypervisor-kvm-smoke
```

That rail drives at least two replay-readiness dogfood workloads through the bounded local multi-hypervisor campaign runner, persists queue state and per-run receipts, and validates the campaign receipt. It covers local multi-hypervisor KVM proof only; it is not a hosted service, shared remote queue, cross-machine scheduler, or Antithesis parity claim.

Replay-readiness scheduler plans now use typed executable, argument, environment, input, limit, and teardown facts. Rust passes the executable and arguments directly to the pinned `bounded-exec` mechanism. Legacy command text remains diagnostic-only and cannot execute. See [`docs/typed-operator-commands.md`](docs/typed-operator-commands.md).

Compiled Rust tools also own dogfood, receipt, summary, audit, scaffold, and local KVM product automation. See [`docs/rust-product-automation.md`](docs/rust-product-automation.md).

GitHub Actions builds that check, prints the saved summary line, and uploads `replay-readiness-receipt.json`, `replay-readiness-summary.txt`, `replay-readiness-dashboard.html`, `operator-triage-runbook.md`, `fleet-triage-index.html`, `decision-receipt.json`, and `decision-receipt-summary.txt` as the `replay-readiness-receipt` artifact.

For the VM drift gate specifically, run the bounded hide-TSC operator profile:

```bash
nix run .#vm-determinism-drift -- --out "$PWD/dogfood-results/vm-determinism-drift-latest" --runs 5
```

This is the current passing DST VM confidence rail for the selected single-VM and controller cases. It emits `receipt.json` plus dlogs and remains a bounded drift check; it does not promote arbitrary guest/device/timing determinism. For legacy A/B diagnosis, call `determinism_stress` directly with `--single-clock-profile tsc`.

To run exactly one slow KVM accepted-verdict proof rail after static checks pass, select a workload explicitly and pass any dogfood wrapper args after `--`:

```bash
nix run .#replay-readiness -- --receipt "$PWD/target/replay-readiness-raft.json" --dogfood raft -- \
  --output dogfood-results/raft-accepted-verdict-dogfood-<timestamp>
```

Selected dogfood may build kernel/initrd/runtime artifacts if they are not cached; checks-only is the default, and receipt emission does not curate or promote dogfood evidence. If `--output` is omitted, `replay-readiness` supplies a timestamped `dogfood-results/replay-readiness-<workload>-<timestamp>` output under the caller's working directory before invoking the selected dogfood wrapper. Wrapper defaults such as `--fail-after-values` and `--max-attempts` derive from `dogfood-results/accepted-dogfood-expectations.json`, so changing the live proof probe requires updating the lockfile and passing the static drift gate first.

For durable dogfood bundles outside the Nix smoke wrapper, use the packaged bounded retry apps and then curate/commit only the concise evidence boundary plus the referenced snapshot artifact:

```bash
nix run .#raft-accepted-verdict-dogfood -- \
  --output dogfood-results/raft-accepted-verdict-dogfood-<timestamp>
```

The helper can also exercise non-Raft workloads by parameterizing the assertion ID, cmdline template, and optional disk image. The redb proof uses:

```bash
nix run .#redb-accepted-verdict-dogfood -- \
  --output dogfood-results/redb-accepted-verdict-dogfood-<timestamp>
```

The net proof uses the virtio-net kernel/initrd and no disk image:

```bash
nix run .#net-accepted-verdict-dogfood -- \
  --output dogfood-results/net-accepted-verdict-dogfood-<timestamp>
```

The Rust workload proof attempt uses the packaged initrd, a KCOV-enabled kernel, one VM, and the explicit harness assertion identity. This slow rail can build a Linux kernel when no cached kernel exists:

```bash
nix run .#rust-workload-accepted-verdict-dogfood -- \
  --output dogfood-results/rust-workload-accepted-verdict-dogfood-<timestamp>
```

Replay verdict classes are stable strings: `snapshot_backed_reproduced`, `snapshot_backed_not_reproduced`, `schedule_only_replay_gap`, `missing_snapshot_ref`, `missing_snapshot_artifact`, `invalid_snapshot_digest`, `no_bug_found`, and `replay_error`. Only `snapshot_backed_reproduced` is accepted as proof of the selected snapshot-backed replay rail. It does not prove global deterministic hypervisor correctness across arbitrary workloads, devices, host timing, or all replay paths. See [`docs/snapshot-fidelity.md`](docs/snapshot-fidelity.md) for the exact state boundary, restore rules, and compatibility policy.

External consumers can use the stable exact-cohort descriptor in [`docs/portable-snapshot-descriptors.md`](docs/portable-snapshot-descriptors.md). The descriptor excludes host locators and grants no restore authority.

Historical workload-proof inventory and current promotion status are tracked in `docs/replay-proof-coverage.md`, `docs/replay-readiness-status.md`, `docs/assertion-readiness-status.md`, and `dogfood-results/accepted-workload-proofs.json`. Legacy schema-v1 rows are historical input, not current promotion authority. New claims require the typed cohort in `contracts/fresh-workload-proofs/cohort.ncl`, admitted v2 identity, committed evidence, and passing coverage and readiness checks. Oversized snapshots can use a chunks manifest plus ordered parts. The coverage gate verifies the stream against its logical digest.

Rust workload authoring is `supported-bounded-rust-cohort` for the admitted downstream-shaped cohort. Run `nix run .#fresh-rust-workload-proof -- --scaffold /tmp/my-service --output /tmp/my-service-proof --name my-service` for this bounded onboarding flow. It builds the scaffold and runs the KVM replay cohort. Its result is cohort-scoped. It is not proof for arbitrary scaffold code.

The assertion-readiness report may show zero ordinary assertion blockers after local harness coverage and replay-probe signal separation. Read that as an instrumentation-readiness signal only: it does not establish hosted-product parity, universal determinism, workload onboarding completeness, or operator triage UX readiness without the separate replay/readiness gates above.

Validate the committed evidence bundle with:

```bash
check-contract-registry .
check-evidence-contracts --root .
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --check-doc .
cargo run -p chaoscontrol-evidence --bin materialize-snapshot-chunks -- --selftest
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --check .
cargo run -p chaoscontrol-evidence --bin check-assertion-readiness-promotion-gate -- .
cargo run -p chaoscontrol-evidence --bin check-dogfood-artifact-sizes --
cargo run -p chaoscontrol-evidence --bin check-accepted-dogfood-config -- \
  --config $(nix build .#accepted-verdict-dogfood-config --print-out-paths --no-link) \
  --expectations dogfood-results/accepted-dogfood-expectations.json
nix build .#checks.x86_64-linux.dependency-audit --no-link -L
nix build .#checks.x86_64-linux.dependency-policy --no-link -L
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
```

The dependency audit fails on vulnerabilities and on any untriaged cargo-audit warning. Current warning dispositions live in `audits/cargo-audit-warning-allowlist.json`, and the Nix check copies both the raw audit JSON and the allowlist into its output for review. The dependency policy check runs `cargo-deny` offline over license, ban, and source provenance rules from `deny.toml`.

### Bug Workflow

```bash
# 1. Explore — find bugs
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel vmlinux --initrd initrd.gz \
  --vms 3 --rounds 100 --output results/

# 2. Minimize — shrink the fault schedule
cargo run --release --bin chaoscontrol-explore -- minimize \
  --kernel vmlinux --initrd initrd.gz \
  --bug results/bug_0.json --output minimized.json

# 3. Reproduce — verify the bug
cargo run --release --bin chaoscontrol-explore -- reproduce \
  --kernel vmlinux --initrd initrd.gz \
  --bug minimized.json --serial \
  --verdict-output replay-verdict.json
```

### Replay & Debugging

```bash
# Replay a recorded session
cargo run --release --bin chaoscontrol-replay -- replay \
  --recording session.json --ticks 5000

# Triage — generate bug report from recording
cargo run --release --bin chaoscontrol-replay -- triage \
  --recording session.json --bug-id 1 --format markdown

# Show recording metadata
cargo run --release --bin chaoscontrol-replay -- info \
  --recording session.json

# Determinism log tools
cargo run --release --bin chaoscontrol-replay -- dlog diff a.dlog b.dlog
cargo run --release --bin chaoscontrol-replay -- dlog dump run.dlog
cargo run --release --bin chaoscontrol-replay -- dlog stats run.dlog
```

### Live Dashboard

```bash
# Run exploration with live web dashboard
cargo run --release --bin chaoscontrol-explore --features dashboard -- run \
  --kernel vmlinux --initrd initrd.gz \
  --vms 3 --rounds 100 --dashboard

# Custom dashboard port
cargo run --release --bin chaoscontrol-explore --features dashboard -- run \
  --kernel vmlinux --initrd initrd.gz \
  --dashboard --dashboard-port 9090

# Review past results (standalone mode)
cargo run --release --bin chaoscontrol-dashboard -- serve --corpus results/
```

The dashboard shows:
- Coverage growth chart with bug discovery markers
- Per-assertion status table (failed/passed/unexercised)
- Round-by-round progress table
- Network fabric statistics
- Live updates via Server-Sent Events

Open `http://localhost:8080` in a browser while exploration runs.

### eBPF tracing

```console
# Collect a legacy debug trace. This is not complete eBPF evidence.
sudo chaoscontrol-trace live --pid <VMM_PID> --output trace.json

# Run pure evidence fixtures.
cargo run -q -p chaoscontrol-trace --bin ebpf-trace-evidence-selftest

# Run the strict privileged attachment smoke against an existing VMM TGID.
sudo cargo run -q -p chaoscontrol-trace \
  --bin ebpf-trace-evidence-selftest -- \
  --privileged-smoke-pid <VMM_PID> --require-privileged

# Generate real KVM exit and IRQ trace traffic in one target process.
sudo cargo test -p chaoscontrol-trace \
  --test ebpf_kvm_smoke -- --ignored --nocapture
```

Complete evidence requires an admitted Nickel profile, exact runtime cohort,
producer and userspace accounting, stable target identity, and cleanup evidence.
See [`docs/ebpf-trace-evidence.md`](docs/ebpf-trace-evidence.md).

### SpaceWasm MVP differential evidence

`chaoscontrol-wasm-differential` remeasures the exact Mantle reference bundle. It then compares bounded SpaceWasm and Wasmtime observations for the admitted WebAssembly 1.0 core-module corpus.

```bash
nix build .#checks.x86_64-linux.spacewasm-mvp-differential -L
```

See [the SpaceWasm MVP differential guide](docs/spacewasm-mvp-differential.md) for the profile, command, evidence fields, and non-claims.

## Architecture

### VM Setup (`vm.rs`)

`DeterministicVm` is the main entry point, configured via `VmConfig`:

```rust
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use chaoscontrol_vmm::cpu::CpuConfig;

let config = VmConfig {
    memory_size: 256 * 1024 * 1024,
    cpu: CpuConfig {
        tsc_khz: 3_000_000,
        seed: 42,
        ..CpuConfig::default()
    },
    ..VmConfig::default()
};

let mut vm = DeterministicVm::new(config)?;
vm.load_kernel("vmlinux", Some("initrd.gz"))?;
vm.run()?;
```

### CPU Determinism (`cpu.rs`)

Comprehensive CPUID filtering:

| CPUID Leaf | What's Filtered | Why |
|------------|----------------|-----|
| 0x1 | RDRAND, TSC-Deadline, hypervisor bit | Hardware RNG, timer jitter |
| 0x7 | RDSEED, AVX2, AVX-512 | Hardware RNG, ISA variation |
| 0x15 | TSC frequency info | Fixed crystal clock ratio |
| 0x16 | Processor frequency | Consistent MHz reporting |
| 0x40000000+ | KVM paravirt leaves | Hide hypervisor presence |
| 0x80000001 | RDTSCP | Bypasses MSR-trap path |
| 0x80000007 | Invariant TSC | Guest shouldn't assume host TSC |

Virtual TSC for fully deterministic time:

```rust
use chaoscontrol_vmm::cpu::VirtualTsc;

let mut vtsc = VirtualTsc::new(3_000_000, 1_000);
vtsc.tick();                    // Advance by 1000 counts
let ns = vtsc.elapsed_ns();    // Convert to nanoseconds
let snap = vtsc.snapshot();    // Serialize for checkpoints
```

### Guest SDK (Antithesis-style)

The `chaoscontrol-sdk` crate provides a guest-side testing API inspired by
[Antithesis](https://antithesis.com). Guest code uses these to annotate
properties and receive guided random values:

```rust
use chaoscontrol_sdk::prelude::*;

chaoscontrol_init();

// Signal setup complete — faults may begin
lifecycle::setup_complete(&[("nodes", "3")]);

// Safety property: must always hold
cc_assert_always!(leader < num_nodes, "valid leader");

// Liveness property: must hold at least once across all runs
cc_assert_sometimes!(write_ok, "write succeeded");

// Reachability
cc_assert_reachable!("leader elected");
cc_assert_unreachable!("split brain");

// Guided random choice for exploration
let action = random::random_choice(3);
```

All assertion sites are registered at compile time via `linkme` and
reported to the VMM at startup. The exploration report shows which
assertions were exercised, passed, failed, or never reached.

For Rust projects that need a repeatable setup/scenario shape, see
[`docs/rust-workload-harness.md`](docs/rust-workload-harness.md). The harness
keeps the existing SDK APIs intact while adding local dry-run reports for
setup-complete, assertion exercise, sometimes/reachable progress, and guided
random-choice observations. The in-tree Nix rail packages that harness shape as
`.#guest-rust-workload` / `.#initrd-rust-workload`; `nix run
.#rust-workload-local-report` produces instrumentation-only JSON, and `nix run
.#explore-rust-workload` runs a bounded VM campaign with a separate evidence
classification file.

### Fault Injection Engine

```rust
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_fault::faults::Fault;

let schedule = FaultScheduleBuilder::new()
    .at_ns(1_000_000_000, Fault::NetworkPartition {
        side_a: vec![0],
        side_b: vec![1, 2],
    })
    .at_ns(5_000_000_000, Fault::NetworkHeal)
    .at_ns(8_000_000_000, Fault::ProcessKill { target: 1 })
    .at_ns(10_000_000_000, Fault::InjectInterrupt { target: 0, irq: 5 })
    .build();
```

**27 fault types** across 6 categories: network (partition, latency,
jitter, bandwidth, loss, corruption, reorder, duplication, heal), disk
(I/O errors, torn writes, corruption, full), process (kill, pause,
restart), clock (skew, jump), resource (memory pressure), interrupt
(IRQ injection, NMI).

Fault evidence uses six distinct stages:

1. `Selected` means that the schedule or random source chose the fault.
2. `Applicable` means that the pure planner accepted the fault and produced an exact effect plan.
3. `Rejected` means that planning failed before an effect occurred.
4. `Applied` means that the adapter completed an immediate effect or armed a reachable mechanism.
5. `ApplicationFailed` means that the adapter did not complete the plan.
6. `Observed` means that a real execution or data path consumed the effect.

Selection does not prove application. Application does not prove workload impact.
An armed mechanism can remain applied and unobserved.

`faults_injected`, `faults_fired`, and `FaultFired` are legacy selected-only projections.
New acceptance logic must use the stage ledger and counters.
The default campaign policy records a rejection and continues.
Set `rejection_is_fatal` to stop the campaign after a rejection.

Memory pressure, CPU stall, clock freeze, and clock jitter currently return explicit unsupported-capability rejections.
Other invalid targets, parameters, ranges, devices, and state transitions return typed rejection or application-failure records.

Snapshots preserve the stage ledger, pending mechanisms, operation ordering, and exact attempt attribution.
Exact replay rejects malformed, mixed-run, or out-of-horizon evidence.
Counterfactual replay starts a new run group and retains valid prefix attribution.

### Run Loop

The VM run loop handles exits and advances the virtual TSC deterministically:

- **IoIn/IoOut**: Serial port I/O, device access, SDK hypercalls
- **Hlt**: VM halted — fast-forward TSC + inject timer IRQ
- **MmioRead/MmioWrite**: Virtio MMIO, HPET, ACPI PM timer
- **Hypercall**: VMCALL-based SDK transport (preferred over port I/O)
- Every exit increments the virtual TSC by a fixed amount

Execution modes:
- `run()` — run until halt/shutdown
- `run_until(pattern)` — run until serial output matches
- `run_bounded(max_exits)` — run for N exits (deterministic scheduling)

## Dependencies

```toml
kvm-ioctls = "0.19"       # KVM API
kvm-bindings = "0.10"     # KVM structures
vm-memory = "0.17"        # Guest memory management
linux-loader = "0.13"     # Kernel loading (ELF)
vm-superio = "0.8"        # Serial port emulation
vmm-sys-util = "0.12"     # EventFd, utilities
rand_chacha = "0.3"       # Seeded PRNG
linkme = "0.3"            # Compile-time assertion catalog
snafu = "0.8"             # Error handling
```

## Roadmap

Current baseline: replay artifacts for `raft`, `redb`, `net`, and `rust-workload` are historical diagnostics. Fresh admitted v2 KVM evidence is still required. The product target remains Rust-only workload support on one machine with multiple local ChaosControl hypervisors.

### Current missing features

**SDK / workload surface**

- [ ] Promote new Rust workload authoring from experimental to supported: scaffold → local dry-run → assertion-quality gate → VM dogfood → accepted verdict/manifest/snapshot curation should be a repeatable product path per workload.
- [ ] Add more first-party Rust adapter/checker examples for common service shapes (storage state machines, RPC/queue systems, and consistency histories) without raw-log scraping.
- [ ] Tighten simulator-to-VM bridge automation so SDK workload metadata, simulator-local receipts, and VM snapshot replay receipts can be compared per workload without merging evidence classes.
- [ ] Improve local artifact hygiene for SDK users: bounded retention, chunking, promotion receipts, and clear failure diagnostics when a workload is not promotable.

**Hypervisor / local control plane**

- [x] Promote the local multi-hypervisor control plane beyond the KVM smoke rail: durable worker state, resource budgets, artifact roots/indexes, queue transitions, and follow-up reproduce/minimize jobs as one supported local workflow.
- [ ] Broaden bounded determinism matrix coverage for named product profiles while preserving visible failing/unsupported rows and rejecting universal guest/device/timing claims.
- [ ] Expand structured deterministic fault coverage across the local campaign rail with per-workload observed/not-observed/unsupported fault-class evidence.
- [ ] Keep hosted UI, SaaS, cross-machine scheduling, non-Rust SDKs, and full Antithesis parity as non-goals unless product scope is explicitly reopened.

### Completed baseline

- [x] Boot Linux kernel in single-vCPU KVM VM
- [x] CPUID filtering (RDRAND, RDSEED, RDTSCP, AVX, hypervisor)
- [x] TSC pinning + virtual TSC tracking
- [x] Complete snapshot/restore (CPU + memory + devices)
- [x] Deterministic entropy (seeded ChaCha20)
- [x] Deterministic block device with fault injection
- [x] Deterministic network (simulated queues)
- [x] Guest SDK (Antithesis-style assertions + guided randomness)
- [x] Fault injection engine (network, disk, process, clock faults)
- [x] Property oracle (cross-run assertion tracking + verdicts)
- [x] VMM ↔ SDK hypercall integration (VMCALL + port I/O fallback)
- [x] Virtio transport layer (MMIO-based, blk + net + rng)
- [x] Multi-VM simulation controller with network fabric
- [x] Deterministic scheduling across VMs
- [x] SMP — multi-vCPU with serialized execution
- [x] Coverage-guided exploration (AFL-style edge bitmaps)
- [x] Input tree exploration — branch at random_choice() decision points
- [x] Network simulation fidelity (jitter, bandwidth, duplication)
- [x] Kernel coverage (KCOV) — kernel code path visibility
- [x] Assertion catalog — compile-time registration via linkme
- [x] Fault schedule minimization — delta debugging
- [x] Bug reproduction from JSON reports
- [x] Determinism logging (dlog) — binary event log + diff + stats
- [x] Time-travel debugger with counterfactual analysis
- [x] Per-round exploration history and plateau detection
- [x] Per-assertion detail reports with JSON export
- [x] Multi-VM networking (virtio-net + smoltcp TCP/IP)
- [x] Interrupt injection faults (IRQ + NMI)
- [x] Core pinning for reduced scheduling jitter
- [x] Nix-native build pipeline (guest packages, initrd builder, kernel composer)
- [x] Declarative simulation tests via `mkChaosTest`

## Using ChaosControl from Your Flake

Add ChaosControl as a flake input and define simulation tests for your
own guest binaries:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    chaoscontrol.url = "github:user/chaoscontrol";
  };

  outputs = { self, nixpkgs, chaoscontrol, ... }:
    let
      system = "x86_64-linux";
      cc = chaoscontrol.lib.${system};
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      # Define a simulation test as a flake check
      checks.${system}.my-consensus-test = cc.mkChaosTest {
        name = "my-consensus";
        kernel = cc.mkChaosKernel { virtioNet = true; };
        initrd = cc.mkChaosInitrd {
          init = self.packages.${system}.my-guest;
        };
        vms = 3;
        rounds = 100;
        branches = 8;
        seed = 42;
      };

      # Use pre-built kernels to skip kernel compilation
      checks.${system}.quick-test = cc.mkChaosTest {
        name = "quick";
        kernel = chaoscontrol.packages.${system}.net-vmlinux;
        initrd = cc.mkChaosInitrd {
          init = self.packages.${system}.my-guest;
        };
        rounds = 10;
      };
    };
}
```

Run with `nix flake check` (requires `system-features = kvm` in
`nix.conf` for the builder).

## License

Future revisions of repository-owned ChaosControl crates, tools, templates, configuration, lifecycle material, and documentation are `AGPL-3.0-or-later`. Each package and copyable template includes the complete license text.

Earlier Apache-2.0 grants remain valid for revisions published before this policy. Third-party and upstream-derived material retains its governing terms. Running a workload does not relicense unrelated workload source or output. See [LICENSES](LICENSES) and [docs/licensing.md](docs/licensing.md) for the exact path and prior-grant map.

## References

- [VM Cohort](rad:z2QJLUqyAZnnHPiZQ1BFjLsX9ush3) at `ab123e3673b6dd616b3df5d044026b5e85755149` — product-neutral retained-base, private-overlay, KVM clone, lifecycle, cleanup, and conformance mechanics.
- [NASA SpaceWasm](https://github.com/nasa/spacewasm) at `e24cf09355a90497148eb5029fdb8e3400bd63e3` — exact experimental core-MVP interpreter used by the bounded diagnostic rail.
- [`../mantle/`](../mantle/) at `a141fcbaafe41f9a413a81275a33fe915bfca370` — producer of the remeasured SpaceWasm source, runner, fixture, toolchain, report, and non-claim bundle.
- [Wasmtime](https://github.com/bytecodealliance/wasmtime) — independent reference engine for normalized bounded observations; a match does not prove engine equivalence.
- [Antithesis documentation index](docs/references/antithesis-documentation.md) — design reference for deterministic simulation, fault injection, assertions, exploration, replay, and debugging.
- [WalTier](https://github.com/danthegoodman1/waltier/tree/d5dda89fb176d590d03c7812d047ced2712bba94) — bounded object-store DST reference for seeded faults, crash cycles, and oracle invariants.
- [antithesishq/antithesis-skills](https://github.com/antithesishq/antithesis-skills) — workflow-design prior art for staged agent research, workload onboarding, launch gates, and receipt-first triage. ChaosControl does not depend on Antithesis, Snouty, Docker Compose, Kubernetes, or hosted services.
- [Cilium](https://github.com/cilium/cilium) at `8c0423e970e62706bcd5dd3a57e1ffaee697439c` — connectivity-matrix, identity-aware policy, and loss-aware Hubble flow design reference. ChaosControl retains simulation, fault, replay, trace, and evidence semantics.
