# Verification: Preallocate deterministic runtime capacity

## Baseline

The baseline ran before core changes at revision `fa69c65`.

| Command | Result |
|---|---|
| `nix develop -c cargo test -p chaoscontrol-sim-core` | Pass: 49 tests. |
| `nix develop -c cargo test -p chaoscontrol-vmm --lib` | Pass: 468 tests and 9 ignored tests. |

The baseline covered scheduler transitions, journal bounds, virtio buffers, network retention, snapshots, poison states, and deterministic replay helpers.

## Implementation evidence

The implementation adds these checked surfaces:

- A pure runtime-capacity plan with checked arithmetic and a domain-separated BLAKE3 identity.
- Startup allocation for schedule records, virtio scratch slots, network TX packet slots, and queue metadata.
- Move-only generation-bound scratch leases with clearing before use and after release.
- Packet-slot FIFO retention with pre-commit packet, byte, frame, and free-slot checks.
- Bounded capacity observations and an overclaim validator.

Positive tests cover valid plans, exact limits, reuse, FIFO order, stable identities, and unchanged poison behavior.

Negative tests cover zero limits, one-past-cap limits, contradictions, arithmetic overflow, startup allocation errors, exhaustion, stale leases, duplicate releases, forged capacities, leaks, and overclaims.

## Focused results

| Command | Result |
|---|---|
| `nix develop -c cargo test -p chaoscontrol-sim-core -p chaoscontrol-vmm --lib` | Pass: 55 simulation-core tests and 475 VMM tests. |
| `nix develop -c cargo test -p chaoscontrol-vmm --test virtio_block_paths --test virtio_net_entropy_paths --test virtio_net_retention --test virtio_post_progress` | Pass: 22 tests. |
| `nix develop -c cargo clippy -p chaoscontrol-sim-core -p chaoscontrol-vmm --all-targets -- -D warnings` | Pass. |
| `nix develop -c cargo test -p chaoscontrol-vmm --test deterministic_smp_kvm` | Pass: 3 KVM tests. |

## Broad results

| Command | Result |
|---|---|
| `nix develop -c cargo fmt --check` | Pass. |
| `nix develop -c cargo test --workspace` | Pass. |
| `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` | Pass. |
| Cairn validate and proposal, design, and tasks gates | Pass. |
| `nix build .#checks.x86_64-linux.dependency-policy --no-link -L` | Pass after the stale named license and source entries were restored. |
| `nix build .#checks.x86_64-linux.dependency-audit --no-link -L` | Pass: zero vulnerabilities. |
| `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L` | Pass: 1,869 advisory findings and zero errors. |
| `nix flake check -L` | Pass: all checks passed. |

The first Tiger Style run exposed denied findings in macro-generated Serde defaults, generated libbpf code, and one negative predicate name. The repair adds explicit default constructors, scopes the generated-code exception, and uses a positive boundary name. It does not weaken the global lint profile.

## Allocation probe

The deterministic probe compares retained capacities before and after selected operations.

- Journal reserve, commit, drain, reset, restore, and policy reconfiguration retain startup record capacity.
- Scratch acquire, operation error, release, and reuse retain startup buffers.
- Network enqueue uses startup packet slots and queue metadata. Snapshot and drain publication remain outside the selected no-growth path.

## Claim boundary

The evidence does not claim deterministic latency, complete process-wide allocation removal, zero-copy I/O, or guaranteed host memory.

Snapshot encoding, trace publication, queue drain output, exploration state, and reports can still allocate.
