# Repeatability gate runner

Date: 2026-11-05. Built guests:

- kernel: `/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux` (`.#kcov-vmlinux`)
- initrd: `/nix/store/ymfxhvyv4zr3la0q4k6h6famb9avz8y9-chaoscontrol-initrd-raft` (`.#initrd-raft`)

## Implementation

`chaoscontrol_explore::repeatability_gate` runs one recorded configuration twice
through the explorer executable as separate processes, loads both checkpoints, and
compares `round_history[].branch_digests` with
`chaoscontrol_explore::repeatability::compare`. It records each run's checkpoint
BLAKE3 identity, rounds, branches, operational rounds, and unavailable rounds;
refuses to overwrite an existing receipt; and maps verdicts to exit status 0
(match), 1 (mismatch), 2 (operational), 3 (unavailable).

`chaoscontrol-explore repeatability-gate` exposes it. Unit tests cover run facts,
verdict status mapping, workload-scoped outputs, and receipt round-trip plus
overwrite refusal.

## Observed run

```text
chaoscontrol-explore repeatability-gate \
  --kernel <kcov-vmlinux> --initrd <initrd-raft> \
  --workload none --seed 42 --rounds 3 --branches 4 --ticks 1000 \
  --bootstrap-budget 5000 --vms 1 \
  --output-root /tmp/cc-gate/control \
  --receipt /tmp/cc-gate/control-receipt.json
```

Result: `Mismatch { round: 1, branch: Some(0), reason: BranchDigest }`, exit 1.
Both runs recorded 3 rounds and 12 branches with zero operational and zero
unavailable rounds:

- first checkpoint `b3:acdd27425ef9f1c2685da02a53fb2ee9e4354ff0feb05a2c9dceb1b4f41b382e`
- second checkpoint `b3:a5a14b7b9d44fa236f1b9060229c8ffecc8a0d1757c3920e58cf43b96f7e1096`

This reproduces the defect the change targets: identical seed, configuration,
kernel, initrd, and explorer build produce different branch digests in round 1.
The first divergent branch is now named, which is the input the cause tasks need.

## Boundaries

The receipt compares two bounded runs of one configuration. It does not prove
determinism for other seeds, hosts, kernels, or workloads. No cause is diagnosed
here; the cause tasks depend on `repair-incremental-snapshot-lineage`.
