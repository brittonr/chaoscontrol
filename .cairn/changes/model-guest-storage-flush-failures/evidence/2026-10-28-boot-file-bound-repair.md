# VMM boot-file bound repair — 2026-10-28

## Observed

After the kernel artifact leaf became a regular file, `snapshot-replay-smoke`
failed with:

```text
Exploration failed: Vm { source: Io { source: Custom { kind: InvalidData,
  error: "boot file is not a bounded regular file" } } }
```

The external boot-file guard bounded the kernel by `memory_ceiling_bytes`
(256 MB for the smoke), while the kernel image is 410 MB. A kernel file is not
a guest-memory resident; linux-loader already validates each segment against
guest memory.

## Repair

`crates/chaoscontrol-vmm/src/vm.rs` now bounds the kernel file itself at
`MAX_KERNEL_FILE_BYTES = 1 GiB` and keeps the guest-memory validation in the
loader. Initrd reads keep their existing bound.

## Verification and limit

`cargo check -p chaoscontrol-vmm` passes. `snapshot-replay-smoke` now reaches
kernel boot (`Freeing unused kernel image …`) and fails later with
`HostWatchdogTimeout { vcpu: 0 }` under the concurrent VMM hardening changes.
The complete rail was last green at `77a5d696`; it is not claimed green at the
current external HEAD while that hardening work is in flight.
