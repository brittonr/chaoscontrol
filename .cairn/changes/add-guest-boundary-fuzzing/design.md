## Decision

Two detached cargo-fuzz workspaces:

- `crates/chaoscontrol-protocol/fuzz` — `payload`, `catalog_frames`
- `crates/chaoscontrol-vmm/fuzz` — `virtio_chain`, `virtio_request`

Both are listed under `[workspace] exclude` in the root manifest and carry `[workspace]`
so they resolve independently. Each commits its own `Cargo.lock` (copied from the
root workspace so versions match) and a `corpus/` directory.

Inputs are parsed from raw bytes with a deterministic reader instead of
`arbitrary`. That keeps corpora stable and hand-craftable across compiler versions,
and it lets the harness bound shapes directly: descriptors 0–32, queue size 1–16,
regions 0–4, chain limit 1–8, aggregate/transfer limits from a `u16`, so fit and
overflow decisions are reached constantly.

Properties checked by the harness, not by the code under test:

- `payload`: `decode` then `encode` then `decode` is a fixed point.
- `catalog_frames`: `decode_catalog_begin` admits only the exact two version bytes;
  decoded catalog completion, descriptor, and event frames re-encode to the same
  bytes (no alternative canonical encoding).
- `virtio_chain` and `virtio_request`: every planned buffer fits one admitted
  region (with zero-length ranges trivially contained, matching
  `range_is_contained`), no address wraps, count and aggregate stay within limits,
  no descriptor index repeats, block transfers stay inside the device and status
  addressing matches the chain, net/entropy transfers respect their limits, and
  scratch requests never exceed their maximum.

`RUSTFLAGS` must carry sancov plus `-Zsanitizer=address`, so the build passes
`--target x86_64-unknown-linux-gnu`; otherwise the same flags would hit proc-macro
and build-script compilation.

`scripts/fuzz-smoke.sh` builds each fuzz crate and runs every target with
`-max_total_time=${FUZZ_SECONDS:-60} -timeout=5 -rss_limit_mb=1024
-malloc_limit_mb=512`. `nix run .#fuzz-smoke` provides `rust-bin.nightly` and clang
and runs the script; it is a non-default app, not part of `nix flake check`.

Two harness defects were found and fixed during the first smoke run: an undersized
re-encode buffer for event frames (66-byte binding, not 64) and a containment
reference that rejected zero-length buffers that the production helper accepts
trivially. Neither was a production defect.

## Verification

Each target was built and run for 30 seconds with cargo-fuzz: payload 2,659,570
runs, catalog_frames 3,685,474 runs, virtio_chain 927,234 runs, virtio_request
1,099,604 runs, no crashes. The committed script was then run end-to-end for five
seconds per target through both a local nightly toolchain and
`nix run .#fuzz-smoke`; both reported all four targets clean
(318,243 / 349,478 / 89,400 / 78,866 runs in the app run). Seeded corpora cover
catalog begin/complete/descriptor/event frames, payload JSON/empty/UTF-8, valid
chains, a next-pointer cycle, address overflow, out-of-region buffers, block
read/flush, net transmit, and entropy requests.

## Boundaries

A clean smoke run is bounded, non-exhaustive evidence for the recorded seeds, limits,
and toolchain. It does not prove decoder or planner correctness, coverage, or
absence of crashes outside those bounds, and it says nothing about guest behavior
or KVM execution.
