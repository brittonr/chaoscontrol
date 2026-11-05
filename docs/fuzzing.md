# Guest-boundary fuzzing

Two guest-controlled boundaries are fuzzed with libFuzzer: the hypercall protocol
frames (`chaoscontrol-protocol`) and the virtio descriptor-chain and request
planners (`chaoscontrol-vmm`). Both are pure functions over arguments; no KVM,
guest, or device state is involved.

## Targets

| Crate | Target | Properties |
|---|---|---|
| `chaoscontrol-protocol` | `payload` | decode → encode → decode is a fixed point (payloads are not canonical: trailing bytes and lossy UTF-8 are dropped) |
| `chaoscontrol-protocol` | `catalog_frames` | catalog begin admits only the two version bytes; catalog completion, descriptor, and event frames re-encode to the same bytes (no alternative canonical encoding) |
| `chaoscontrol-vmm` | `virtio_chain` | accepted plans keep every buffer inside one admitted region, never wrap, stay within chain/aggregate limits, and never repeat a descriptor index |
| `chaoscontrol-vmm` | `virtio_request` | block transfers stay inside the device and address the status buffer, net/entropy transfers respect their limits, scratch requests stay within their maximum |

Inputs are parsed from raw bytes by a small deterministic reader rather than
`arbitrary`, so corpora stay stable across toolchain versions. Shapes are bounded:
descriptors 0–32, queue size 1–16, regions 0–4, chain limit 1–8, and byte limits
taken from a `u16` so fit/overflow decisions are reached constantly. Zero-length
buffers are treated as trivially contained, matching `range_is_contained`.

## Running

```bash
# Bounded smoke (default 60 s per target); needs a nightly toolchain and network.
nix run .#fuzz-smoke

# Shorter local run.
FUZZ_SECONDS=10 nix run .#fuzz-smoke
```

The script `scripts/fuzz-smoke.sh` builds each fuzz crate with
`--target x86_64-unknown-linux-gnu` (so proc macros are not instrumented) and runs
each target with `-timeout=5 -rss_limit_mb=1024 -malloc_limit_mb=512`. The fuzz
workspaces are excluded from the root workspace and are not part of
`nix flake check`.

`nix run .#fuzz-smoke` is an app, so it fetches crates on first use and uses the
repository at the current directory (override with `FUZZ_ROOT`). To run the script
outside Nix, put a nightly `cargo`/`rustc` on `PATH`; the flags require
`-Zsanitizer=address` and sancov coverage.

## Corpora

Committed seeds live in `crates/chaoscontrol-protocol/fuzz/corpus/` and
`crates/chaoscontrol-vmm/fuzz/corpus/`. Protocol seeds are generated from the real
encoders by `crates/chaoscontrol-protocol/fuzz/examples/gen_seeds.rs`; virtio seeds
are hand-crafted raw inputs covering valid chains, a next-pointer cycle, address
overflow, an out-of-region buffer, block read/flush, net transmit, and entropy.

## Findings

A crash becomes a regression test beside the existing adversarial tests
(`crates/chaoscontrol-protocol/tests/`, `crates/chaoscontrol-vmm/tests/`) and its
input stays in the corpus. Minimized crashers are not committed as standalone
artifacts.

## Non-claims

A clean smoke run is bounded, non-exhaustive evidence for the recorded seeds,
limits, toolchain, and time budget. It does not prove decoder or planner
correctness, coverage, or absence of crashes outside those bounds, and says nothing
about guest behavior, KVM execution, determinism, or release readiness.
