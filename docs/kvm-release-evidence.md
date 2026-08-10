# KVM Release Evidence

The KVM release rail gives one verdict for one source revision. It is separate from portable CI.

## Matrix authority

`contracts/kvm-release/matrix.ncl` is the human-authored source. Its contract is `contracts/kvm-release/profile.ncl`.

The checked runtime projection is `contracts/kvm-release/matrix.json`. The matrix requires these behavior rows:

- exact deterministic SMP;
- serialized snapshot continuation;
- malformed virtio MMIO input survival;
- the admitted hide-TSC drift profile;
- one fresh workload replay.

Two more rows build and bind the release binaries and the Raft guest. A PMU claim must add a required PMU row.

Each row has an exact argument vector. It also has a timeout, an artifact-count limit, and an artifact-byte limit.

## Worker command

First, check the projection and the adversarial fixtures:

```bash
nix run .#check-kvm-release-matrix -- --root .
```

Use a clean revision on an admitted x86_64 Linux worker. The worker must provide read-write KVM access.

```bash
revision="$(git rev-parse HEAD)"
nix run .#kvm-release-matrix -- \
  --root . \
  --matrix contracts/kvm-release/matrix.json \
  --out target/kvm-release \
  --expected-revision "$revision"
```

The runner does not use a command shell. It starts the program and argument vector from the matrix.

The runner records host architecture, kernel release, KVM API version, source state, commands, limits, outcomes, and BLAKE3 artifact identities. It kills a row when its timeout expires.

## Terminal classes

`release-eligible` means that all required rows passed for the exact recorded cohort. Every missing, stale, dirty, skipped, unsupported, timed-out, failed, or tampered row produces `blocked`.

The portable workflow validates code and the matrix. It does not claim KVM behavior. The separate `kvm-release` job runs only on the `chaoscontrol-kvm` worker label.

## Outputs

The worker writes:

- `release-receipt.json` for machine checks;
- `release-summary.md` for review;
- bounded stdout, stderr, and row artifacts.

The validated run for source `62af4500e16dc73948277c87b79d8c26f06e46c7` is summarized in `dogfood-results/kvm-release-evidence-20260809/validation-receipt.json`. It binds the full local receipt by BLAKE3.

Raw worker files remain under the primary worktree's `.pi` evidence area. They are not a source of product truth.

## Claim boundary

A passing receipt applies only to its exact source, matrix, worker facts, commands, limits, and artifacts.

It does not prove worker integrity, all-host equivalence, universal determinism, workload correctness, or production availability.
