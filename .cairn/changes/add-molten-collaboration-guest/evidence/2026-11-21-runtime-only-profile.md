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

## Rebuild of the capture wrapper

The wrapper unit test and strict Clippy passed, and Nix built the static guest replacement. Task 55 nevertheless exited 1 during assembly: its selected signed output had disappeared from the host Nix store. It never started the explorer (build/assembly log BLAKE3 `cc9bbd82bf510215258f822041db841783886b958b87799c05e50630ac163628`). The earlier initrd still contains the byte-identical package, driver, trust and Nix database. A recovery attempt uses those original signed bytes with only `/init` replaced; it neither generates a new signer nor weakens startup admission.

## Serial export was stopped by a separate idle budget

The repacked initrd kept the original signed cohort and changed only the wrapper. At both 300,000 and 360,000 branch ticks, the driver again passed with the same observation reference. The two logs retained the same 19,891 bytes of a declared 22,827-byte frame, no terminal marker, and the same accepted-catalog result: the required completion assertion missed. The extractor denied each partial stream. The greater tick budget did not change the stopping point or the branch digest.

`DeterministicVm::run_bounded` has a separate 50,000-exit post-setup idle threshold, and it counts UART exits as ordinary non-SDK exits. The wrapper stopped making SDK calls while writing a long serial frame, so extending the campaign tick horizon cannot address the idle classification. The wrapper now flushes each bounded data record and reports SDK progress, without changing VMM limits, fault selection, trust, or the extractor's completion checks. A fresh fixed-wrapper execution and terminal readback are required before promotion.

## Completed bounded campaign

<!-- r[verify chaoscontrol.molten_guest.campaign] -->

With the cooperative export wrapper, task 69 passed `cargo test --locked -p chaoscontrol-molten-guest` and denied-warning Clippy. Remote realization stopped before KVM; task 71 repeated realization on the local Nix builder without changing any source pin or trust input. The new wrapper (`4e2e944bc22ba22a134c09bbaeaa2dc3b327fbe1494f4eb268c8036beed2a20a`) replaced only `/init` of the recovered signed carrier (repacked initrd `55e53a18cf54b43b8b13ca8cca29c8b206c39add57ddf98c741b20fb69392279`). Explorer `e3f2386e0193a25d4a51545300f603c553e3f00cdb8f7a5b825df467d4f3be76`, kernel `f2abae8a52c83c975ed15de504535445e2ac159cc0198f8028867b0c18cdf67c`, signed Aspen executable `d63c4a4987143d12312922eb943f73aa5d5a139125266471c4aa26be345e34c5`, driver `6e0b403627f52a69b17951ef1e9520bf76f70081e0693844dc406332183d2cc7`, trust `c74a2b7fdde52d9efde338760028dba89400986d7f8cb85459158b7a635aa017`, runtime closure `78fb99e2aa4342485b7a8b433be8cd3433660ec511faadff1eb86f7519077359`.

Task 71 retained one VM/vCPU, 2 GiB, one round and branch, quantum 100, seed 42, 300,000 ticks, 120,000 bootstrap budget, `input-tree`, and random faults disabled. It finished within the 2,200-second execution bound. SDK terminal event `molten_driver_done` records `success=true` and exit 0. The accepted catalog's completion assertion `2565890805` had one true hit and no false hits. The full 22,827-byte observation passed the strict extractor and matched `blake3:6723ae7c751f38ce99ad25f12fc69eca30a574d72c15dba412228bec07386cbe`. Assertion catalog `ed77066c065a72f785da8bc985a59cc0e1be6457eef1c339f9c9e17e6939af4e`, checkpoint `988ac838cd1981e73d130fbe90040581c3adc2ed9191136ab9602b75fc31a3c0`, frontier trace `dd0368eb4aeca2db1d3570e943716c3fa8581f53981cd1ff4519967e8c2efe87` (BLAKE3). Task-owned originals include the journal, console, signed runtime and kernel/carrier; Aspen retains the packed observation, assertions, SDK events and frontier trace.

This is one bounded no-fault campaign, not saved-snapshot replay, independently admitted three-guest execution, storage-fault qualification, or production/release approval. No related cohort gate is marked complete.
