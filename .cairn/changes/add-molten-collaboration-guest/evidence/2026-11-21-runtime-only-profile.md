# Runtime-only guest profile (2026-11-21)

Aspen added the distinct `molten-nix-guest-runtime-binding-v1` profile.

## Signed guest output

```
/nix/store/qjb341yh9f1p9jzw4m3s49vx64g5x6pg-molten-guest-runtime-binding-v1
```

- 10 measured runtime paths
- 109,233,552-byte NAR
- input-addressed and signed by the experimental operator key
- full-profile admission remains unchanged

## Host driver

The real three-process collaboration driver passed against the guest output:

```
baseline observations=blake3:48ab2650cc296f88684f026d89265d6f5b0c253d40ab1b9251b6e21c605d9ddc
```

## Guest image

`scripts/build-molten-guest-store-initrd` copies the signed output, the runtime
cohort, and the Nix verifier into a chroot store with signatures and packages it
as a 414 MB uncompressed initrd. The guest boots `/init` and starts the driver,
but the host single-vCPU watchdog still interrupts a long no-exit stretch before
the baseline completes. No guest campaign evidence is claimed.

## Boundary

Runtime-only admission proves the measured runtime cohort and executing binary,
not the omitted build/compiler/gate closures.

## Streaming guest attempt

The wrapper now streams the driver output to the serial console instead of
buffering it until exit. The guest reached `running 1 test` and the baseline test
remained active for over eight minutes without completing; the run was stopped
rather than claimed. The host run of the same signed output and driver completed
in about six minutes.

## Loopback and guest bottleneck

The guest starts with `lo` down (`flags 0x8`). The wrapper now raises it
(`0x49`), after which all three nodes complete setup and start. The driver then
spends minutes per `molten node service-host` invocation because each process
re-authenticates source through the guest Nix daemon. `scope::select` alone
issues multiple preview and operation processes per node per round. The guest
does not finish inside the bounded run; no campaign evidence is claimed.

## Hardened run: the driver reached its terminal result

With fresh source authentication still enabled, pueue task 51 ran one VM, one vCPU, one round/branch, seed 42, no random faults, 2 GiB, quantum 100, bootstrap budget 120,000, and a 300,000-tick limit. After seven unsuccessful scope rounds it observed currentness in round eight, completed the installed work and reopen path, and the real driver reported one passing test. It printed baseline reference `blake3:6723ae7c751f38ce99ad25f12fc69eca30a574d72c15dba412228bec07386cbe`.

The outer 2,200-second limit then sent termination and eventually killed the explorer (task exit 137); the log BLAKE3 is `882d29afd2f8d3670a0ad190b769b6b29a846d4f65a8ab258a477b9ac7c4d905`. Unlike the 50,000- and 100,000-tick attempts, this run reached real quorum and driver completion. It did not retain the packed observation, a terminal assertion report, or a replay verdict. The wrapper must keep yielding after the driver returns and explicitly export the artifact before this milestone is a complete campaign result.
