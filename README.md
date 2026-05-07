# ChaosControl — Deterministic VMM

A deterministic Virtual Machine Monitor (VMM) for x86_64 built with KVM
and the rust-vmm crate ecosystem. Designed for simulation testing of
distributed systems where reproducibility is essential.


This is just an experiment with Claude + Pi.dev. Use at your own risk

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
- **Fault schedule minimization**: Delta debugging (ddmin) to find
  the smallest schedule that triggers a bug
- **Bug reproduction**: Replay a bug report to verify it still triggers
- **Assertion catalog**: Compile-time registration of all assertion sites
  via `linkme`; reports show which assertions are exercised/unexercised
- **Per-round history**: Coverage growth curves, plateau detection,
  bug discovery timeline

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
    ├── chaoscontrol-vmm/                  # VMM implementation
    ├── chaoscontrol-explore/              # Coverage-guided exploration engine
    ├── chaoscontrol-replay/               # Recording, replay, time-travel debugger
    ├── chaoscontrol-trace/                # eBPF-based KVM tracing
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

# Run tests (827 unit + doc tests)
cargo test

# Build guest binaries (statically linked, musl)
nix build .#guest-sdk    # → result/bin/chaoscontrol-guest
nix build .#guest-raft   # → result/bin/chaoscontrol-raft-guest
nix build .#guest-net    # → result/bin/chaoscontrol-net-guest

# Build initrd images (from guest binaries)
nix build .#initrd-sdk   # → result (gzipped cpio)
nix build .#initrd-raft
nix build .#initrd-net

# Build custom kernels
nix build .#net-vmlinux       # virtio-net enabled
nix build .#kcov-vmlinux      # KCOV coverage
nix build .#kcov-net-vmlinux  # both

# Boot a kernel
cargo run --bin boot -- <kernel-path> [initrd-path]

# Snapshot demo
cargo run --release --bin snapshot_demo -- <kernel-path> <initrd-path>
```

## CLI Tools

### Quick Start (Nix)

```bash
# Run Raft exploration with one command (builds kernel + guest + initrd)
nix run .#explore-raft

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

# Finalize only targeted snapshot-backed replay candidates; filenames preserve
# their checkpoint index (for example bug_2.json), and unrelated snapshot refs
# are not validated.
cargo run --release --bin chaoscontrol-explore -- export-bugs \
  --checkpoint results/checkpoint.json --output results/ \
  --assertion-id 1806003755 --min-replay-parent-depth 1 --max-bugs 1
```

Output directory contains:
- `checkpoint.json` — resumable exploration state; checkpoint saves now persist replay parent snapshot refs for bugs when a parent snapshot is available
- `report.txt` — human-readable report with per-round history
- `assertions.json` — per-assertion verdicts and hit counts
- `bug_N.json` — bug reports (consumable by minimize/reproduce)
- `snapshots/<sha256>.snapshot.bin` — hash-addressed replay parent snapshot artifacts containing zstd-compressed bincode `SimulationSnapshot` payloads for bugs that need parent context
- `run-config.json` and `receipt.json` — contract-backed review inputs generated with `scripts/materialize-dogfood-receipt.py`
- `replay-verdict.json` — optional Rust-owned machine-readable reproduce/smoke verdict emitted by `reproduce --verdict-output`

### Contract-backed evidence

Nickel contracts live under `contracts/evidence/`. Human-authored run configs
and dogfood receipts are the review boundary; runtime-emitted bug reports,
assertion summaries, and checkpoint references remain Rust-owned JSON that is
validated at the boundary. Raw `run.log` / `reproduce.log` files are debug-only
and are intentionally excluded from the acceptance record. Replay parent snapshot
references are Rust-derived, JSON/Nickel-contractable refs (`store`, `digest`,
`codec`, `schema_version`, and confined `snapshots/...` path); the optional redb
store/index is host-side only and is not a public evidence format.

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

It runs the bounded Raft `snapshot_replay_probe` workload, finalizes checkpoint-held bugs with `export-bugs`, verifies the selected parent snapshot artifact digest, and requires standalone reproduce to write a `replay-verdict.json` with `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `replay_parent_depth > 0`, a valid snapshot ref, and `command.exit_status = 0`. Raw logs remain in the temporary build directory.

For a durable dogfood bundle outside the Nix smoke wrapper, use the bounded retry helper and then curate/commit only the concise evidence boundary plus the referenced snapshot artifact:

```bash
python scripts/accepted-snapshot-verdict-dogfood.py \
  --explore target/debug/chaoscontrol-explore \
  --kernel /nix/store/...-chaoscontrol-vmlinux/vmlinux \
  --initrd /nix/store/...-chaoscontrol-initrd-raft \
  --output dogfood-results/raft-accepted-verdict-dogfood-<timestamp>
```

The helper can also exercise a non-Raft workload by parameterizing the assertion ID, cmdline template, and optional disk image. The redb second-workload proof uses:

```bash
python scripts/accepted-snapshot-verdict-dogfood.py \
  --workload redb \
  --explore target/debug/chaoscontrol-explore \
  --kernel /nix/store/...-chaoscontrol-vmlinux/vmlinux \
  --initrd /nix/store/...-chaoscontrol-initrd-redb \
  --disk-image /nix/store/...-redb-disk-image \
  --assertion-id 2718281828 \
  --cmdline-template 'redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after={fail_after}' \
  --vms 1 --rounds 3 --branches 2 --ticks 80 --memory-mb 256 \
  --output dogfood-results/redb-accepted-verdict-dogfood-<timestamp>
```

Replay verdict classes are stable strings: `snapshot_backed_reproduced`, `snapshot_backed_not_reproduced`, `schedule_only_replay_gap`, `missing_snapshot_ref`, `missing_snapshot_artifact`, `invalid_snapshot_digest`, `no_bug_found`, and `replay_error`. Only `snapshot_backed_reproduced` is accepted as proof of the selected snapshot-backed replay rail. It does not prove global deterministic hypervisor correctness across arbitrary workloads, devices, host timing, or all replay paths.

The current accepted workload-proof coverage is tracked in `docs/replay-proof-coverage.md`, `docs/replay-readiness-status.md`, and `dogfood-results/accepted-workload-proofs.json`. New breadth/readiness claims must add a committed manifest entry plus evidence and pass the aggregate coverage and generated-readiness checks.

Validate the committed evidence bundle with:

```bash
python scripts/check-contract-registry.py
python scripts/check-evidence-contracts.py
python scripts/check-replay-proof-coverage.py
python scripts/generate-replay-readiness-report.py --check
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
```

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

### eBPF Tracing

```bash
# Live KVM trace (requires sudo)
sudo chaoscontrol-trace live --pid <VMM_PID> --output trace.json

# Verify determinism between two traces
chaoscontrol-trace verify --trace-a run1.json --trace-b run2.json
```

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
