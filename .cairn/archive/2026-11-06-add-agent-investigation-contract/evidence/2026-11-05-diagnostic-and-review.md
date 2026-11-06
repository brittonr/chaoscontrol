# Guest diagnostic operation and reviewer receipt

Date: 2026-11-05.

## Guest diagnostic operation

Artifacts:

- kernel: `/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux`
- supervisor initrd: `result-initrd-multiprocess`
- executable: `/init`, `blake3:c732a09bb6c6e5d272288ee408f1362c846480104547ed6a05eda0cc6f6c22ca`

```text
chaoscontrol-explore diagnose \
  --kernel <kcov-vmlinux> --initrd <initrd-multiprocess> \
  --vms 1 --seed 42 --bootstrap-budget 3000 --ticks 100 \
  --executable /init --executable-digest blake3:c732a09b…22ca \
  --arg=--fixture-worker --timeout-ms 500 \
  --receipt /tmp/cc-diag-session4/session.json
```

Result: one canonical guest receipt, zero pending requests:

- status `completed`, exit code 1, stdout 0 bytes, stderr 75 bytes, no truncation
- request digest `blake3:37dfe34d…9664`
- executable digest `blake3:c732a09b…22ca`
- receipt digest `blake3:931374c4…a2ca`
- `counterfactual: true`

Two guest-supervisor defects were found and fixed by running the real guest:

1. The kernel forwards unknown command-line tokens to `/init`; the supervisor
   treated `nokaslr` as a relative manifest path and exited. Only an absolute
   first argument is now accepted as a manifest path.
2. The supervisor loop slept on the host clock after setup. Guest time advances
   from VMM exits, so an idle sleep stalled deterministic progress. The loop now
   yields and relies on its hypercalls for progress.

## Candidate review

- Positive candidate: same kernel/initrd with `raft_bug=premature_commit`,
  6 rounds × 1 branch × 1000 ticks, seed 42: `completed=true`, `bugs_found=1`.
- Negative candidate: same artifacts with `raft_bug=none`, same bounds:
  `completed=true`, `bugs_found=0`.
- Original failure: `premature_commit` checkpoint bug 0, tick 1477.
- Branch identity from the live probe: `b3:5e4fa949…fa2`.
- Replay verdict: `snapshot_backed_reproduced`, reproduced, snapshot `valid`.

`review-candidate` emitted schema `chaoscontrol.candidate_review.v1` with verdict
`ready` and review digest `blake3:2423f194…06dd`. The receipt binds the original
failure, replay facts, branch identity, both candidate receipt digests, their
candidate artifact identities, recorded cmdlines, completion, bug counts, and
fresh checkpoint identities.

## Boundaries

The diagnostic operation reports bounded execution facts, not executable trust or
output correctness. Candidate receipts describe one fresh run each; the reviewer
verdict is not a fix, replay-fidelity proof, correctness proof, or release
eligibility.
