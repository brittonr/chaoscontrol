# Design: Snapshot replay smoke check

## Scope

The check is a KVM smoke gate, not part of sandbox-safe flake-wide tests. It should be runnable directly as:

```bash
nix build .#checks.x86_64-linux.snapshot-replay-smoke --no-link -L
```

## Workload

Use the targeted Raft mode:

```text
raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=15
```

with bounded parameters proven locally to finish report generation quickly:

```text
vms=3 rounds=2 branches=2 ticks=60 seed=42 mode=hybrid bootstrap_budget=10000 memory_mb=128
```

The explorer may be killed by the timeout after writing report/checkpoint/bug artifacts because some VMM cleanup paths can outlive the bounded evidence point. The check must treat that as acceptable only when the expected artifacts exist and downstream export/reproduce succeeds.

## Assertions

The smoke script must:

1. Run the workload in a scratch output directory.
2. Export checkpoint-held bugs in-place with `export-bugs`.
3. Select a bug with `replay_parent_depth > 0` and `replay_parent_snapshot_ref != null`.
4. Validate the referenced `snapshots/<digest>.snapshot.bin` path is confined to the run dir.
5. Validate the SHA-256 digest in the reference matches artifact bytes.
6. Run standalone `reproduce` against the selected bug with matching kernel/initrd/memory/cmdline.
7. Require the reproduce output to contain `BUG REPRODUCED`.
8. Write only a concise success marker to `$out`; raw logs stay inside the temporary build directory.

## Nix wiring

Expose the script through a KVM-required check named `snapshot-replay-smoke`. The derivation should depend on the built `chaoscontrol` package, `net-vmlinux`, and `initrd-raft`, and use `coreutils`/`python3` for timeout and artifact checks.
