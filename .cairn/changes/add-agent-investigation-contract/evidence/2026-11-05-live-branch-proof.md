# Live branch and intervention proof

Date: 2026-11-05. Artifacts:

- kernel: `/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux`
- initrd: `/nix/store/ymfxhvyv4zr3la0q4k6h6famb9avz8y9-chaoscontrol-initrd-raft`
- checkpoint: `/tmp/cc-campaign/premature_commit/checkpoint.json` (fresh `premature_commit` campaign, one VM)

## Probe

```text
chaoscontrol-explore restore-probe \
  --checkpoint /tmp/cc-campaign/premature_commit/checkpoint.json \
  --index 0 --rounds 0 \
  --set-register rax=0x2a --expect-register rax=0x2a \
  --output /tmp/cc-probe/probe.json
```

Observed `chaoscontrol.restore_probe.v1` facts:

- `branch_id: b3:5e4fa949…fa2` (deterministic disposable-branch identity)
- `checkpoint_index: 0`, `bug_id: 0`, `replay_parent_depth: 1`
- `snapshot_digest: sha256:5c62937c…5031`
- `simulation_tick: 1477`, `restored_tick: 1174`
- `interventions: [{vm_index: 0, vcpu: 0, register: rax, previous: 117, value: 42, boundary: after_restore_before_seek}]`
- `counterfactual: true`
- `expectation: {expected: 42, actual: 42, matched: true}`

## Original immutability

The checkpoint file SHA-256 was `db6079f7c013b40f23d722720d4df4a7a5976ae61b29d524713a8ae61155e7ad`
before and after the probe. A new regression,
`read_only_investigation_never_rewrites_the_checkpoint`, also asserts that
listing, inspection, moment binding, and restore planning leave the checkpoint
bytes unchanged.

## Boundaries

The probe restores the recorded parent in fresh VMs and reports read-only state;
it is not replay evidence, a correctness verdict, or a claim that the original
failure was reproduced. The intervention is explicitly counterfactual.
