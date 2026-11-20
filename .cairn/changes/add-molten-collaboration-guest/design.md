## Context

Aspen exposes the collaboration-native runtime through `molten node serve` and the `collaborationnative` integration driver. The driver's positive case is `installed_native_effect_commits_through_raft_and_returns_a_completion`; it requires `MOLTEN_TEST_SOURCE_TRUST` and refuses to simulate missing signed artifacts. ChaosControl already provides a musl guest SDK, a fixed initrd builder, KVM execution, fault schedules, and retained run outputs.

## Decisions

### 1. Reuse Aspen's exact driver case

**Choice:** The guest wrapper invokes the built `collaborationnative` test binary with `--ignored --exact installed_native_effect_commits_through_raft_and_returns_a_completion --nocapture`, with `MOLTEN_TEST_SOURCE_TRUST` selected from the cmdline.

**Rationale:** The driver already owns three signed node processes, installed-effect admission, protected delivery, completion, shutdown, and source-admitted reopen. Copying that flow into ChaosControl would fork authority semantics and make drift undetectable.

### 2. Preserve absolute Nix store identity

**Choice:** Copy the signed output, the driver binary, and the required closure paths to the same absolute paths, and set the cmdline trust path to the copied record. The current signed output's measured build closure is 21,247 paths and about 83.9 GB, so an initrd is not viable as-is. A campaign requires either a signed guest output with a bounded runtime cohort or a disk-backed Nix store.

**Rationale:** Aspen authenticates the exact output path and re-hashes every path in the measured build closure; relocating `bin/molten` or dropping build-closure paths invalidates admission. The measured bounds are facts, not packaging preferences.

### 3. One guest first, three guests later

**Choice:** The first campaign runs the three-process driver inside one guest. Only after that completes do we add a three-guest cohort with one node per VM.

**Rationale:** A single-guest campaign validates initrd assembly, process supervision, signed admission, and evidence retention without adding guest networking and cross-VM fault semantics. It is a smaller honest milestone.

### 4. Missing trust fails closed

**Choice:** The wrapper emits an unreachable assertion and preserves the driver's failure output when the trust is absent, unreadable, or rejected.

**Rationale:** A guest that silently skips signed admission would manufacture apparent evidence for a runtime that never started.

### 5. Evidence remains bounded

**Choice:** Retain kernel/initrd identities, Aspen output and trust refs, driver revision, command, campaign seed/schedule, SDK events, assertion results, and the driver's canonical observation. Classify the result as bounded VM campaign evidence.

**Rationale:** The driver's own non-claim already excludes authority, production, and global exactly-once conclusions; ChaosControl must not widen it.
