## 1. Guest binaries as Nix packages

- [x] 1.1 Add musl crane toolchain and cargoArtifacts for `x86_64-unknown-linux-musl` target in flake.nix
- [x] 1.2 Add `guest-sdk` package: crane buildPackage for `chaoscontrol-guest` crate with musl target
- [x] 1.3 Add `guest-raft` package: crane buildPackage for `chaoscontrol-raft-guest` crate with musl target
- [x] 1.4 Add `guest-net` package: crane buildPackage for `chaoscontrol-net-guest` crate with musl target
- [x] 1.5 Verify all three guest binaries are statically linked (`file` / `ldd` check)
- [x] 1.6 Delete `scripts/build-guest.sh`, `scripts/build-raft-guest.sh`, `scripts/build-net-guest.sh`

## 2. mkChaosInitrd

- [x] 2.1 Implement `mkChaosInitrd` function in flake.nix (runCommand with cpio + gzip)
- [x] 2.2 Add `initrd-sdk` package using `mkChaosInitrd { init = guest-sdk; }`
- [x] 2.3 Add `initrd-raft` package using `mkChaosInitrd { init = guest-raft; }`
- [x] 2.4 Add `initrd-net` package using `mkChaosInitrd { init = guest-net; }`
- [x] 2.5 Remove `guest/*.gz` from git and add `guest/*.gz` to `.gitignore`

## 3. mkChaosKernel

- [x] 3.1 Implement `mkChaosKernel` function with `virtioNet`, `kcov`, and `extraConfig` parameters
- [x] 3.2 Redefine `net-vmlinux` package as `mkChaosKernel { virtioNet = true; }`
- [x] 3.3 Redefine `kcov-vmlinux` package as `mkChaosKernel { kcov = true; }`
- [x] 3.4 Add `kcov-net-vmlinux` package as `mkChaosKernel { virtioNet = true; kcov = true; }` to verify composition

## 4. mkChaosTest

- [x] 4.1 Implement `mkChaosTest` function that invokes `chaoscontrol-explore run` in a runCommand derivation
- [x] 4.2 Add `requiredSystemFeatures = [ "kvm" ]` to mkChaosTest derivations
- [x] 4.3 Wire up exploration parameters (vms, rounds, branches, ticks, seed, mode, diskImage, extraArgs)
- [x] 4.4 Verify output artifacts ($out/report.txt, $out/assertions.json) are produced on success
- [x] 4.5 Verify non-zero exit code on assertion failure

## 5. Flake lib and downstream API

- [x] 5.1 Export `lib.<system>.mkChaosInitrd`, `lib.<system>.mkChaosKernel`, `lib.<system>.mkChaosTest`
- [x] 5.2 Add `explore` app entry pointing to `chaoscontrol-explore` binary
- [x] 5.3 Add `explore-raft` app entry that wires net-vmlinux + initrd-raft + default explorer params
- [x] 5.4 Add `explore-sdk` app entry that wires default kernel + initrd-sdk + default explorer params

## 6. Flake checks (simulation tests)

- [x] 6.1 Add `checks.raft-sim` using mkChaosTest with net kernel + raft initrd (small rounds for CI)
- [x] 6.2 Verify `nix flake check` passes on KVM-capable machine (unit tests + simulation)
- [x] 6.3 Verify `nix flake check` skips simulation checks on non-KVM machine

## 7. Cleanup and documentation

- [x] 7.1 Update README.md build/run instructions to use `nix build` instead of shell scripts
- [x] 7.2 Update README.md with downstream flake usage example
- [x] 7.3 Update devShell shellHook to remove references to deleted build scripts
- [x] 7.4 Verify `nix flake check` still passes (existing checks: build, clippy, fmt, nixfmt, tests)
