{
  description = "ChaosControl — Deterministic VMM for simulation testing";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nickel-1-17-0.url = "github:nickel-lang/nickel/1320a983e6c3d1e2fb53dd2464b084b4903b1426";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    octet.url = "git+file:../octet?ref=refs/heads/main&rev=9c7ba87bef2934d2b7b144167e13c8d18eac8958";
    trellis.url = "git+file:../trellis?ref=refs/heads/main&rev=46ab2d92b9cfd2cfc4e631a56f3e667ee7263685";
    vm-cohort-src = {
      url = "git+rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3?rev=ab123e3673b6dd616b3df5d044026b5e85755149";
      flake = false;
    };
    mantle = {
      url = "github:OnixResearch/mantle/a141fcbaafe41f9a413a81275a33fe915bfca370";
      inputs.crane.follows = "crane";
      inputs.octet.follows = "octet";
      inputs.tigerstyle.follows = "octet";
    };
    advisory-db = {
      url = "github:RustSec/advisory-db";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nickel-1-17-0,
      crane,
      rust-overlay,
      octet,
      trellis,
      vm-cohort-src,
      mantle,
      advisory-db,
    }:
    let
      supportedSystems = [ "x86_64-linux" ]; # KVM is Linux-only
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      # Shared per-system definitions — computed once, used by
      # packages, checks, apps, and devShells.
      eachSystem = forAllSystems (
        system:
        let
          exactNickel = nickel-1-17-0.packages.${system}.default;
          pkgs = import nixpkgs {
            inherit system;
            overlays = [
              (import rust-overlay)
              (_final: _previous: { nickel = exactNickel; })
            ];
          };

          vmCohortRevision = "ab123e3673b6dd616b3df5d044026b5e85755149";
          vmCohortDependencyCheck =
            assert pkgs.lib.assertMsg (
              vm-cohort-src.rev == vmCohortRevision
            ) "ChaosControl VM Cohort Nix input revision drifted";
            pkgs.runCommand "chaoscontrol-vm-cohort-dependency"
              {
                nativeBuildInputs = [ pkgs.ripgrep ];
                src = self;
              }
              ''
                set -euo pipefail
                cd "$src"
                expected="${vmCohortRevision}"
                dependency_count=3
                manifest=crates/chaoscontrol-vm-cohort-adapter/Cargo.toml
                test "$(rg -o "$expected" "$manifest" | wc -l)" -eq "$dependency_count"
                lock_source="source = \"git+rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3?rev=$expected#$expected\""
                test "$(rg -F -c "$lock_source" Cargo.lock)" -eq "$dependency_count"
                test "$(rg -c '^vm-cohort-(conformance|core|kvm) = \{ git = "rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3", rev = "'"$expected"'"' "$manifest")" -eq "$dependency_count"
                if rg -n '^vm-cohort-(conformance|core|kvm) = .*\b(branch|tag|path)\s*=' "$manifest"; then
                  echo "VM Cohort Cargo dependencies include a moving ref or path fallback" >&2
                  exit 1
                fi
                test -f ${vm-cohort-src}/crates/vm-cohort-core/src/lib.rs
                test -f ${vm-cohort-src}/crates/vm-cohort-kvm/src/lib.rs
                test -f ${vm-cohort-src}/crates/vm-cohort-conformance/src/lib.rs
                if rg -n 'chaoscontrol_(fault|replay|explore|evidence)|chaoscontrol-fault|chaoscontrol-replay' \
                  ${vm-cohort-src}/crates; then
                  echo "ChaosControl policy types leaked into VM Cohort" >&2
                  exit 1
                fi
                touch "$out"
              '';
          # r[impl chaoscontrol.nickel_toolchain.cohort]
          # r[impl chaoscontrol.nickel_toolchain.compatibility]
          nickelCohortCheck =
            pkgs.runCommand "chaoscontrol-nickel-cohort-exact"
              {
                nativeBuildInputs = [
                  pkgs.gnugrep
                  pkgs.nickel
                  pkgs.ripgrep
                ];
                src = self;
              }
              ''
                set -euo pipefail
                cd "$src"
                test '${nickel-1-17-0.rev}' = '1320a983e6c3d1e2fb53dd2464b084b4903b1426'
                test "$(nickel --version)" = 'nickel-lang-cli nickel 1.17.0 (rev 1320a98)'
                if rg -n 'nickel 1\.15\.1|nixpkgs#nickel' \
                  crates contracts/evidence/fixtures/valid; then
                  echo 'old, ambient, or fallback Nickel evaluator remains' >&2
                  exit 1
                fi
                nickel export --format json contracts/evidence/examples/raft-run-config.ncl >/dev/null
                nickel export --format json contracts/evidence/examples/register-simulator-profile.ncl >/dev/null
                nickel export --format json contracts/evidence/examples/raft-campaign-profile.ncl >/dev/null
                nickel export --format json contracts/evidence/examples/raft-fault-schedule-profile.ncl >/dev/null
                nickel export --format json contracts/guest-determinism/fixtures/valid/bit-exact.valid.ncl >/dev/null
                for invalid in \
                  contracts/guest-determinism/fixtures/invalid/accepted-drift.invalid.ncl \
                  contracts/nickel-toolchain/fixtures/invalid/malformed.ncl \
                  contracts/nickel-toolchain/fixtures/invalid/missing-import.ncl \
                  contracts/evidence/fixtures/invalid/run-profile.unknown-field.invalid.ncl \
                  contracts/evidence/fixtures/invalid/run-profile.zero-budget.invalid.ncl \
                  contracts/evidence/fixtures/invalid/fault-schedule.unknown-action.invalid.ncl; do
                  if nickel export --format json "$invalid" >/dev/null 2>&1; then
                    echo "invalid Nickel fixture unexpectedly passed: $invalid" >&2
                    exit 1
                  fi
                done
                touch "$out"
              '';

          vmCohortAdoptionContractCheck =
            pkgs.runCommand "chaoscontrol-vm-cohort-adoption-contract"
              {
                nativeBuildInputs = [ pkgs.nickel ];
                src = self;
              }
              ''
                set -euo pipefail
                cd "$src"
                observed="$TMPDIR/vm-cohort-adoption.json"
                nickel export --format json contracts/vm-cohort-adoption/adoption.ncl > "$observed"
                cmp "$observed" contracts/vm-cohort-adoption/adoption.json
                for invalid in contracts/vm-cohort-adoption/fixtures/invalid/*.ncl; do
                  if nickel export --format json "$invalid" >/dev/null 2>&1; then
                    echo "invalid VM Cohort adoption profile passed: $invalid" >&2
                    exit 1
                  fi
                done
                touch "$out"
              '';

          rustToolchain = pkgs.rust-bin.stable.latest.default;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # Musl-targeting toolchain for statically-linked guest binaries
          muslRustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "x86_64-unknown-linux-musl" ];
          };
          muslCraneLib = (crane.mkLib pkgs).overrideToolchain muslRustToolchain;
          muslCC = pkgs.pkgsCross.musl64.stdenv.cc;

          # Filter source to include Rust-relevant files, BPF sources, and
          # contract-backed test fixtures used by compile-time include_str!()
          # tests. Crane's cleanCargoSource strips non-Cargo JSON evidence by
          # default, which can make Nix checks fail while local Cargo passes.
          sourceFilter =
            path: type:
            let
              relPath = pkgs.lib.removePrefix "${toString ./.}/" (toString path);
              isEvidenceFixture = pkgs.lib.hasPrefix "contracts/evidence/fixtures/" relPath;
              isArchitectureFixture = pkgs.lib.hasPrefix "contracts/architecture-modules/fixtures/" relPath;
              isPropertyCoverageFixture = pkgs.lib.hasPrefix "contracts/property-coverage/" relPath;
              isKvmReleaseMatrix = relPath == "contracts/kvm-release/matrix.json";
              isAssertionReadinessFixture = pkgs.lib.hasPrefix "crates/chaoscontrol-evidence/tests/fixtures/assertion-readiness/" relPath;
              isDogfoodCheckpointFixture = pkgs.lib.hasPrefix "dogfood-results/raft-20260506-095025/" relPath;
              isDogfoodAssertionHarnessFixture = relPath == "dogfood-results/local-assertion-harnesses.json";
            in
            (craneLib.filterCargoSources path type)
            || isEvidenceFixture
            || isArchitectureFixture
            || isPropertyCoverageFixture
            || isKvmReleaseMatrix
            || isAssertionReadinessFixture
            || isDogfoodCheckpointFixture
            || isDogfoodAssertionHarnessFixture
            || (builtins.match ".*\\.bpf\\.c$" path != null)
            || (builtins.match ".*\\.h$" path != null)
            || (builtins.match ".*\\.html$" path != null)
            || (builtins.match ".*\\.js$" path != null);

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = sourceFilter;
          };

          mkApp = description: program: {
            type = "app";
            inherit program;
            meta.description = description;
          };

          tigerstyleSrc = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type: (sourceFilter path type) || builtins.baseNameOf path == "dylint.toml";
          };
          vmCohortAdapterOctetWorkspace =
            pkgs.runCommand "chaoscontrol-vm-cohort-adapter-octet-workspace" { }
              ''
                mkdir -p "$out/src"
                cp ${./checks/vm-cohort-adapter-octet/Cargo.toml} "$out/Cargo.toml"
                cp ${./checks/vm-cohort-adapter-octet/Cargo.lock} "$out/Cargo.lock"
                cp ${./checks/vm-cohort-adapter-octet/dylint.toml} "$out/dylint.toml"
                cp -R ${./crates/chaoscontrol-vm-cohort-adapter/src}/. "$out/src/"
                substituteInPlace "$out/Cargo.toml" \
                  --replace-fail '@CHAOSCONTROL_SRC@' '${self}' \
                  --replace-fail '@VM_COHORT_SRC@' '${vm-cohort-src}'
              '';

          # Common build arguments shared across all crane invocations
          commonArgs = {
            inherit src;
            strictDeps = true;
            pname = "chaoscontrol";
            version = "0.1.0";

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.llvmPackages.clang-unwrapped # BPF compilation
              pkgs.bpftools # bpftool (vmlinux.h generation)
            ];
            buildInputs = [
              pkgs.elfutils # libelf
              pkgs.zlib # zlib
              pkgs.libbpf # libbpf
            ];

            # libbpf-cargo needs unwrapped clang for BPF target
            CLANG = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
          };

          # Build only the cargo dependencies — cached across rebuilds
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          ociIntakeArtifacts = craneLib.buildDepsOnly (
            commonArgs
            // {
              pname = "chaoscontrol-oci-intake-deps";
              cargoExtraArgs = "-p chaoscontrol-evidence --bin oci-intake";
            }
          );
          guestDeterminismGateArtifacts = craneLib.buildDepsOnly (
            commonArgs
            // {
              pname = "chaoscontrol-guest-determinism-gate-deps";
              cargoExtraArgs = "-p chaoscontrol-evidence --bin guest-determinism-gate";
            }
          );

          # Build the full workspace
          # doCheck = false — tests run via checks.tests instead.
          chaoscontrol = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoExtraArgs = "--workspace --bins";
              doCheck = false;
            }
          );

          ociIntake = craneLib.buildPackage (
            commonArgs
            // {
              pname = "chaoscontrol-oci-intake";
              cargoArtifacts = ociIntakeArtifacts;
              cargoExtraArgs = "-p chaoscontrol-evidence --bin oci-intake";
              doCheck = false;
              doNotPostBuildInstallCargoBinaries = true;
              installPhaseCommand = ''
                mkdir -p $out/bin
                cp target/release/oci-intake $out/bin/
              '';
            }
          );
          guestDeterminismGate = craneLib.buildPackage (
            commonArgs
            // {
              pname = "chaoscontrol-guest-determinism-gate";
              cargoArtifacts = guestDeterminismGateArtifacts;
              cargoExtraArgs = "-p chaoscontrol-evidence --bin guest-determinism-gate";
              doCheck = false;
              doNotPostBuildInstallCargoBinaries = true;
              installPhaseCommand = ''
                mkdir -p $out/bin
                cp target/release/guest-determinism-gate $out/bin/
              '';
            }
          );

          mantleSpacewasmBundle = mantle.packages.${system}.spacewasm-reference-bundle;
          mantleSpacewasmToolchain = mantle.packages.${system}.spacewasm-reference-rust-toolchain;
          spacewasmResumeProbe =
            pkgs.runCommand "chaoscontrol-spacewasm-resume-probe"
              {
                nativeBuildInputs = [
                  mantleSpacewasmToolchain
                  pkgs.gnutar
                  pkgs.gzip
                ];
              }
              ''
                set -eu
                export CARGO_HOME="$TMPDIR/cargo-home"
                export CARGO_NET_OFFLINE=true
                export CARGO_TARGET_DIR="$TMPDIR/target"
                sourceRoot="$TMPDIR/spacewasm-source"
                vendorRoot="$TMPDIR/vendor"
                mkdir -p "$CARGO_HOME" "$sourceRoot" "$vendorRoot"
                tar -xzf ${mantleSpacewasmBundle}/source/spacewasm-e24cf09355a90497148eb5029fdb8e3400bd63e3.tar.gz \
                  --strip-components=1 -C "$sourceRoot"
                tar -xf ${mantleSpacewasmBundle}/dependencies/vendor.tar -C "$vendorRoot"
                mkdir -p "$sourceRoot/.cargo" "$sourceRoot/examples"
                cat > "$sourceRoot/.cargo/config.toml" <<EOF
                [source.crates-io]
                replace-with = "vendored-sources"

                [source.vendored-sources]
                directory = "$vendorRoot"

                [net]
                offline = true
                EOF
                cp ${./tools/spacewasm-resume-probe.rs} \
                  "$sourceRoot/examples/chaoscontrol_spacewasm_resume_probe.rs"
                cd "$sourceRoot"
                cargo build --locked --offline --release --no-default-features \
                  --example chaoscontrol_spacewasm_resume_probe
                mkdir -p "$out/bin"
                cp "$CARGO_TARGET_DIR/release/examples/chaoscontrol_spacewasm_resume_probe" \
                  "$out/bin/spacewasm-resume-probe"
              '';

          # --- Guest binary builds (musl static) ---

          muslCommonArgs = {
            inherit src;
            strictDeps = true;
            pname = "chaoscontrol-guest";
            version = "0.1.0";

            CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
            CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";

            nativeBuildInputs = [
              pkgs.pkg-config
              muslCC
            ];
            buildInputs = [ ];

            # Disable BPF build script (not needed for guests)
            CLANG = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
          };

          muslCargoArtifacts = muslCraneLib.buildDepsOnly (
            muslCommonArgs
            // {
              # Only build deps for guest crates — avoids libbpf-sys
              # (chaoscontrol-trace needs libbpf, guests don't)
              cargoExtraArgs = builtins.concatStringsSep " " [
                "--target x86_64-unknown-linux-musl"
                "-p chaoscontrol-guest"
                "-p chaoscontrol-guest-determinism-probe"
                "-p chaoscontrol-raft-guest"
                "-p chaoscontrol-net-guest"
                "-p chaoscontrol-rust-workload-guest"
              ];
            }
          );

          # Build a single guest crate from the workspace
          mkGuestPackage =
            {
              pname,
              cargoExtraArgs ? "-p ${pname}",
              installPhaseCommand ? null,
              doNotPostBuildInstallCargoBinaries ? false,
            }:
            muslCraneLib.buildPackage (
              muslCommonArgs
              // {
                inherit pname doNotPostBuildInstallCargoBinaries;
                cargoArtifacts = muslCargoArtifacts;
                cargoExtraArgs = "${cargoExtraArgs} --target x86_64-unknown-linux-musl";
              }
              // pkgs.lib.optionalAttrs (installPhaseCommand != null) {
                inherit installPhaseCommand;
              }
            );

          guest-sdk = mkGuestPackage { pname = "chaoscontrol-guest"; };
          guest-determinism-probe = mkGuestPackage {
            pname = "chaoscontrol-guest-determinism-probe";
            doNotPostBuildInstallCargoBinaries = true;
            installPhaseCommand = ''
              mkdir -p $out/bin
              cp target/x86_64-unknown-linux-musl/release/chaoscontrol-guest-determinism-probe \
                $out/bin/
            '';
          };
          guest-raft = mkGuestPackage { pname = "chaoscontrol-raft-guest"; };
          guest-net = mkGuestPackage { pname = "chaoscontrol-net-guest"; };
          guest-redb = mkGuestPackage { pname = "chaoscontrol-redb-guest"; };
          guest-rust-workload = mkGuestPackage { pname = "chaoscontrol-rust-workload-guest"; };
          guest-supervisor = mkGuestPackage {
            pname = "chaoscontrol-sdk";
            cargoExtraArgs = "-p chaoscontrol-sdk --bin chaoscontrol-guest-supervisor";
            doNotPostBuildInstallCargoBinaries = true;
            installPhaseCommand = ''
              mkdir -p $out/bin
              cp target/x86_64-unknown-linux-musl/release/chaoscontrol-guest-supervisor $out/bin/
            '';
          };

          # --- Initrd builder ---

          mkChaosInitrd =
            {
              init, # guest binary derivation (must have bin/<name>)
              name ? "chaoscontrol-initrd",
              extraDirs ? [
                "dev"
                "proc"
                "sys"
                "sys/kernel/debug"
              ],
            }:
            pkgs.runCommand name
              {
                nativeBuildInputs = [ pkgs.cpio ];
              }
              ''
                mkdir -p root
                for d in ${pkgs.lib.concatStringsSep " " extraDirs}; do
                  mkdir -p "root/$d"
                done
                cp ${init}/bin/* root/init
                chmod +x root/init
                (cd root && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > $out
              '';

          initrd-sdk = mkChaosInitrd {
            init = guest-sdk;
            name = "chaoscontrol-initrd-sdk";
          };
          initrd-determinism-probe = mkChaosInitrd {
            init = guest-determinism-probe;
            name = "chaoscontrol-initrd-determinism-probe";
          };
          initrd-raft = mkChaosInitrd {
            init = guest-raft;
            name = "chaoscontrol-initrd-raft";
          };
          initrd-net = mkChaosInitrd {
            init = guest-net;
            name = "chaoscontrol-initrd-net";
          };
          initrd-redb = mkChaosInitrd {
            init = guest-redb;
            name = "chaoscontrol-initrd-redb";
            extraDirs = [
              "dev"
              "proc"
              "sys"
              "sys/kernel/debug"
              "data"
            ];
          };
          initrd-rust-workload = mkChaosInitrd {
            init = guest-rust-workload;
            name = "chaoscontrol-initrd-rust-workload";
          };
          initrd-multiprocess =
            pkgs.runCommand "chaoscontrol-initrd-multiprocess"
              {
                nativeBuildInputs = [ pkgs.cpio ];
              }
              ''
                mkdir -p root/{dev,proc,sys,sys/kernel/debug,etc/chaoscontrol,data,run}
                cp ${guest-supervisor}/bin/chaoscontrol-guest-supervisor root/init
                cp ${./contracts/guest-processes/fixtures/valid/cooperating-processes.json} \
                  root/etc/chaoscontrol/process-manifest.json
                chmod +x root/init
                (cd root && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > $out
              '';

          # --- Disk image builder ---

          redb-disk-image =
            pkgs.runCommand "redb-disk-image"
              {
                nativeBuildInputs = [ pkgs.e2fsprogs ];
              }
              ''
                dd if=/dev/zero of=$out bs=1M count=64
                mkfs.ext4 -F -q $out
              '';

          # --- Kernel builder ---

          mkChaosKernel =
            {
              virtioBlk ? true,
              virtioNet ? false,
              kcov ? false,
              extraConfig ? { },
            }:
            let
              cfg =
                with pkgs.lib.kernel;
                (pkgs.lib.optionalAttrs (virtioBlk || virtioNet) {
                  VIRTIO = yes;
                  VIRTIO_MMIO = yes;
                })
                // (pkgs.lib.optionalAttrs virtioBlk {
                  VIRTIO_BLK = yes;
                  EXT4_FS = yes;
                })
                // (pkgs.lib.optionalAttrs virtioNet {
                  VIRTIO_NET = yes;
                  PACKET = yes;
                })
                // (pkgs.lib.optionalAttrs kcov {
                  KCOV = yes;
                  KCOV_INSTRUMENT_ALL = yes;
                  KCOV_ENABLE_COMPARISONS = yes;
                  DEBUG_FS = yes;
                })
                // extraConfig;
              kernel = pkgs.linuxPackages_latest.kernel.override {
                structuredExtraConfig = cfg;
              };
            in
            pkgs.runCommand "chaoscontrol-vmlinux" { } ''
              mkdir -p $out
              ln -s ${kernel.dev}/vmlinux $out/vmlinux
            '';
          # --- Accepted snapshot-backed replay dogfood wrappers ---

          defaultSnapshotProbeFailAfterValues = [
            25
            30
            35
            20
            40
          ];

          mkAcceptedSnapshotVerdictDogfood =
            {
              name,
              workload,
              kernel,
              initrd,
              assertionId,
              cmdlineTemplate,
              vms,
              rounds,
              branches,
              ticks,
              memoryMb,
              cohortProfile,
              evidencePrefix,
              runTimeout,
              exportTimeout,
              replayTimeout,
              failAfterValues ? defaultSnapshotProbeFailAfterValues,
              maxAttempts ? null,
              expectation ? null,
              diskImage ? null,
            }:
            let
              args = [
                "--workload"
                workload
                "--cohort"
                "${./contracts/fresh-workload-proofs/cohort.json}"
                "--evidence-prefix"
                evidencePrefix
                "--run-timeout"
                (toString runTimeout)
                "--export-timeout"
                (toString exportTimeout)
                "--repro-timeout"
                (toString replayTimeout)
                "--explore"
                "${chaoscontrol}/bin/chaoscontrol-explore"
                "--kernel"
                "${kernel}/vmlinux"
                "--initrd"
                "${initrd}"
              ]
              ++ pkgs.lib.optionals (diskImage != null) [
                "--disk-image"
                "${diskImage}"
              ]
              ++ [
                "--assertion-id"
                (toString assertionId)
                "--cmdline-template"
                cmdlineTemplate
                "--vms"
                (toString vms)
                "--rounds"
                (toString rounds)
                "--branches"
                (toString branches)
                "--ticks"
                (toString ticks)
                "--memory-mb"
                (toString memoryMb)
                "--fail-after-values"
                (pkgs.lib.concatMapStringsSep "," toString failAfterValues)
              ]
              ++ pkgs.lib.optionals (maxAttempts != null) [
                "--max-attempts"
                (toString maxAttempts)
              ];
            in
            pkgs.writeShellApplication {
              inherit name;
              runtimeInputs = [
                chaoscontrol
                pkgs.coreutils
              ];
              text = ''
                exec accepted-snapshot-verdict-dogfood ${pkgs.lib.escapeShellArgs args} "$@"
              '';
            };

          acceptedVerdictDogfoodExpectations = builtins.fromJSON (
            builtins.readFile ./dogfood-results/accepted-dogfood-expectations.json
          );
          acceptedVerdictDogfoodExpectationWorkloads = acceptedVerdictDogfoodExpectations.workloads;
          freshWorkloadProofCohort = builtins.fromJSON (
            builtins.readFile ./contracts/fresh-workload-proofs/cohort.json
          );
          freshWorkloadProofProfiles = builtins.listToAttrs (
            map (profile: {
              name = profile.workload;
              value = profile;
            }) freshWorkloadProofCohort.workloads
          );

          acceptedVerdictDogfoodWorkloads = {
            raft =
              let
                profile = freshWorkloadProofProfiles.raft;
              in
              {
                name = "raft-accepted-verdict-dogfood";
                workload = profile.workload;
                kernel = mkChaosKernel { virtioNet = true; };
                initrd = initrd-raft;
                assertionId = profile.assertion.compatibility_id;
                cmdlineTemplate = profile.cmdline_template;
                vms = profile.bounds.vms;
                rounds = profile.bounds.rounds;
                branches = profile.bounds.branches;
                ticks = profile.bounds.ticks;
                memoryMb = profile.bounds.memory_mib;
                runTimeout = profile.bounds.run_timeout_seconds;
                exportTimeout = profile.bounds.export_timeout_seconds;
                replayTimeout = profile.bounds.replay_timeout_seconds;
                cohortProfile = profile;
                evidencePrefix = "dogfood-results/raft-fresh-v2-proof-20260809";
                failAfterValues = [ profile.bounds.snapshot_probe_fail_after ];
                maxAttempts = profile.bounds.max_attempts;
                expectation = acceptedVerdictDogfoodExpectationWorkloads.raft;
              };
            redb =
              let
                profile = freshWorkloadProofProfiles.redb;
              in
              {
                name = "redb-accepted-verdict-dogfood";
                workload = profile.workload;
                kernel = mkChaosKernel { };
                initrd = initrd-redb;
                diskImage = redb-disk-image;
                assertionId = profile.assertion.compatibility_id;
                cmdlineTemplate = profile.cmdline_template;
                vms = profile.bounds.vms;
                rounds = profile.bounds.rounds;
                branches = profile.bounds.branches;
                ticks = profile.bounds.ticks;
                memoryMb = profile.bounds.memory_mib;
                runTimeout = profile.bounds.run_timeout_seconds;
                exportTimeout = profile.bounds.export_timeout_seconds;
                replayTimeout = profile.bounds.replay_timeout_seconds;
                cohortProfile = profile;
                evidencePrefix = "dogfood-results/redb-fresh-v2-proof-20260809";
                failAfterValues = [ profile.bounds.snapshot_probe_fail_after ];
                maxAttempts = profile.bounds.max_attempts;
                expectation = acceptedVerdictDogfoodExpectationWorkloads.redb;
              };
            net =
              let
                profile = freshWorkloadProofProfiles.net;
              in
              {
                name = "net-accepted-verdict-dogfood";
                workload = profile.workload;
                kernel = mkChaosKernel { virtioNet = true; };
                initrd = initrd-net;
                assertionId = profile.assertion.compatibility_id;
                cmdlineTemplate = profile.cmdline_template;
                vms = profile.bounds.vms;
                rounds = profile.bounds.rounds;
                branches = profile.bounds.branches;
                ticks = profile.bounds.ticks;
                memoryMb = profile.bounds.memory_mib;
                runTimeout = profile.bounds.run_timeout_seconds;
                exportTimeout = profile.bounds.export_timeout_seconds;
                replayTimeout = profile.bounds.replay_timeout_seconds;
                cohortProfile = profile;
                evidencePrefix = "dogfood-results/net-fresh-v2-proof-20260809";
                failAfterValues = [ profile.bounds.snapshot_probe_fail_after ];
                maxAttempts = profile.bounds.max_attempts;
                expectation = acceptedVerdictDogfoodExpectationWorkloads.net;
              };
            rust-workload =
              let
                profile = freshWorkloadProofProfiles."rust-workload";
              in
              {
                name = "rust-workload-accepted-verdict-dogfood";
                workload = profile.workload;
                kernel = mkChaosKernel { kcov = true; };
                initrd = initrd-rust-workload;
                assertionId = profile.assertion.compatibility_id;
                cmdlineTemplate = profile.cmdline_template;
                vms = profile.bounds.vms;
                rounds = profile.bounds.rounds;
                branches = profile.bounds.branches;
                ticks = profile.bounds.ticks;
                memoryMb = profile.bounds.memory_mib;
                runTimeout = profile.bounds.run_timeout_seconds;
                exportTimeout = profile.bounds.export_timeout_seconds;
                replayTimeout = profile.bounds.replay_timeout_seconds;
                cohortProfile = profile;
                evidencePrefix = "dogfood-results/rust-workload-fresh-v2-proof-20260809";
                failAfterValues = [ profile.bounds.snapshot_probe_fail_after ];
                maxAttempts = profile.bounds.max_attempts;
                expectation = acceptedVerdictDogfoodExpectationWorkloads."rust-workload";
              };
          };

          acceptedVerdictDogfood = pkgs.lib.mapAttrs (
            _: cfg: mkAcceptedSnapshotVerdictDogfood cfg
          ) acceptedVerdictDogfoodWorkloads;

          acceptedVerdictDogfoodConfig = pkgs.writeText "accepted-verdict-dogfood-config.json" (
            builtins.toJSON (
              pkgs.lib.mapAttrs (_: cfg: {
                workload = cfg.workload;
                assertion_id = cfg.assertionId;
                cmdline_template = cfg.cmdlineTemplate;
                fail_after_values = cfg.failAfterValues or defaultSnapshotProbeFailAfterValues;
                max_attempts = cfg.maxAttempts or null;
                expectation = cfg.expectation or null;
              }) acceptedVerdictDogfoodWorkloads
            )
          );

          vmDeterminismDrift = pkgs.writeShellApplication {
            name = "vm-determinism-drift";
            runtimeInputs = [
              chaoscontrol
              pkgs.coreutils
            ];
            text = ''
              usage() {
                cat <<'EOF'
              usage: vm-determinism-drift [--out DIR] [--runs N] [-- DETERMINISM_STRESS_ARGS...]

              Runs the bounded operator VM determinism drift gate with the current
              hide-tsc clock profile across the single-VM and controller cases. The
              default writes a JSON receipt and dlogs under
              ./dogfood-results/vm-determinism-drift-latest/. Extra arguments after
              -- are forwarded to determinism_stress.
              EOF
              }

              out="./dogfood-results/vm-determinism-drift-latest"
              runs="5"
              extra_args=()
              while [ "$#" -gt 0 ]; do
                case "$1" in
                  -h|--help)
                    usage
                    exit 0
                    ;;
                  --out)
                    if [ "$#" -lt 2 ]; then
                      echo "--out requires a directory" >&2
                      exit 2
                    fi
                    out="$2"
                    shift 2
                    ;;
                  --runs)
                    if [ "$#" -lt 2 ]; then
                      echo "--runs requires a count" >&2
                      exit 2
                    fi
                    runs="$2"
                    shift 2
                    ;;
                  --)
                    shift
                    extra_args=("$@")
                    break
                    ;;
                  *)
                    echo "unknown argument: $1" >&2
                    usage >&2
                    exit 2
                    ;;
                esac
              done

              mkdir -p "$out"
              determinism_stress \
                ${mkChaosKernel { }}/vmlinux \
                ${initrd-rust-workload} \
                "$runs" \
                --single-clock-profile hide-tsc \
                --receipt "$out/receipt.json" \
                --dlog-dir "$out/dlogs" \
                "''${extra_args[@]}"
              printf 'vm determinism drift receipt: %s\n' "$out/receipt.json"
            '';
          };

          vmDeterminismMatrix = pkgs.writeShellApplication {
            name = "vm-determinism-matrix";
            runtimeInputs = [
              chaoscontrol
              pkgs.coreutils
            ];
            text = ''
              usage() {
                cat <<'EOF'
              usage: vm-determinism-matrix [--out DIR] [--runs N] [-- DETERMINISM_STRESS_ARGS...]

              Runs the bounded operator hide-tsc determinism matrix rail. The rail
              executes the existing determinism_stress cases and emits both the
              legacy drift receipt and the bounded profile-matrix receipt plus a
              concise summary under ./dogfood-results/vm-determinism-matrix-latest/.
              EOF
              }

              out="./dogfood-results/vm-determinism-matrix-latest"
              runs="3"
              extra_args=()
              while [ "$#" -gt 0 ]; do
                case "$1" in
                  -h|--help)
                    usage
                    exit 0
                    ;;
                  --out)
                    if [ "$#" -lt 2 ]; then
                      echo "--out requires a directory" >&2
                      exit 2
                    fi
                    out="$2"
                    shift 2
                    ;;
                  --runs)
                    if [ "$#" -lt 2 ]; then
                      echo "--runs requires a count" >&2
                      exit 2
                    fi
                    runs="$2"
                    shift 2
                    ;;
                  --)
                    shift
                    extra_args=("$@")
                    break
                    ;;
                  *)
                    echo "unknown argument: $1" >&2
                    usage >&2
                    exit 2
                    ;;
                esac
              done

              mkdir -p "$out"
              determinism_stress \
                ${mkChaosKernel { }}/vmlinux \
                ${initrd-rust-workload} \
                "$runs" \
                --single-clock-profile hide-tsc \
                --receipt "$out/drift-receipt.json" \
                --matrix-receipt "$out/matrix-receipt.json" \
                --dlog-dir "$out/dlogs" \
                "''${extra_args[@]}"

              render-vm-determinism-matrix-summary \
                "$out/matrix-receipt.json" \
                "$out/summary.txt"
              printf 'vm determinism matrix receipt: %s\n' "$out/matrix-receipt.json"
              printf 'vm determinism matrix summary: %s\n' "$out/summary.txt"
            '';
          };

          replayReadiness = pkgs.writeShellApplication {
            name = "replay-readiness";
            runtimeInputs = [
              chaoscontrol
              pkgs.coreutils
              pkgs.nickel
            ];
            text = ''
              usage() {
                cat <<'EOF'
              usage: replay-readiness [--receipt PATH] [--dogfood raft|redb|net|rust-workload] [-- DOGFOOD_ARGS...]

              Runs committed replay readiness checks. With --receipt, writes a JSON
              operator receipt for CI/dashboard ingestion. With --dogfood, runs one
              selected accepted-verdict dogfood rail after checks pass. Selected dogfood
              is the slow KVM path and may build kernel/initrd/runtime artifacts if
              uncached.
              EOF
              }

              dogfood=""
              receipt=""
              dogfood_args=()
              while [ "$#" -gt 0 ]; do
                case "$1" in
                  -h|--help)
                    usage
                    exit 0
                    ;;
                  --receipt)
                    if [ "$#" -lt 2 ]; then
                      echo "--receipt requires a path" >&2
                      exit 2
                    fi
                    receipt="$2"
                    shift 2
                    ;;
                  --dogfood)
                    if [ "$#" -lt 2 ]; then
                      echo "--dogfood requires raft, redb, net, or rust-workload" >&2
                      exit 2
                    fi
                    dogfood="$2"
                    shift 2
                    ;;
                  --)
                    shift
                    dogfood_args=("$@")
                    break
                    ;;
                  *)
                    echo "unknown argument: $1" >&2
                    usage >&2
                    exit 2
                    ;;
                esac
              done

              started_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
              contract_registry_status="pending"
              evidence_contracts_status="pending"
              replay_proof_coverage_status="pending"
              readiness_promotion_status="pending"
              readiness_surface_drift_status="pending"
              readiness_report_status="pending"
              assertion_report_status="pending"
              assertion_promotion_status="pending"
              sdk_local_report_tracks_status="pending"
              sdk_assertion_quality_status="pending"
              consistency_fixtures_status="pending"
              artifact_sizes_status="pending"
              accepted_dogfood_config_status="pending"
              dogfood_status="skipped"
              dogfood_output=""
              dogfood_summary_json="null"
              invocation_cwd="$PWD"

              write_receipt() {
                local status="$1"
                local failed_phase="$2"
                local exit_code="$3"
                if [ -z "$receipt" ]; then
                  return 0
                fi
                mkdir -p "$(dirname "$receipt")"
                STATUS="$status" \
                FAILED_PHASE="$failed_phase" \
                EXIT_CODE="$exit_code" \
                STARTED_AT="$started_at" \
                FINISHED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
                DOGFOOD="$dogfood" \
                DOGFOOD_STATUS="$dogfood_status" \
                DOGFOOD_OUTPUT="$dogfood_output" \
                DOGFOOD_SUMMARY_JSON="$dogfood_summary_json" \
                CONTRACT_REGISTRY_STATUS="$contract_registry_status" \
                EVIDENCE_CONTRACTS_STATUS="$evidence_contracts_status" \
                REPLAY_PROOF_COVERAGE_STATUS="$replay_proof_coverage_status" \
                READINESS_PROMOTION_STATUS="$readiness_promotion_status" \
                READINESS_SURFACE_DRIFT_STATUS="$readiness_surface_drift_status" \
                READINESS_REPORT_STATUS="$readiness_report_status" \
                ASSERTION_REPORT_STATUS="$assertion_report_status" \
                ASSERTION_PROMOTION_STATUS="$assertion_promotion_status" \
                SDK_LOCAL_REPORT_TRACKS_STATUS="$sdk_local_report_tracks_status" \
                SDK_ASSERTION_QUALITY_STATUS="$sdk_assertion_quality_status" \
                CONSISTENCY_FIXTURES_STATUS="$consistency_fixtures_status" \
                ARTIFACT_SIZES_STATUS="$artifact_sizes_status" \
                ACCEPTED_DOGFOOD_CONFIG_STATUS="$accepted_dogfood_config_status" \
                DOGFOOD_EXPECTATIONS="${./dogfood-results/accepted-dogfood-expectations.json}" \
                materialize-replay-readiness-receipt "$receipt"
              }

              run_gate() {
                local name="$1"
                local status_var="$2"
                shift 2
                printf -v "$status_var" '%s' running
                if "$@"; then
                  printf -v "$status_var" '%s' pass
                else
                  rc=$?
                  printf -v "$status_var" '%s' fail
                  write_receipt failed "$name" "$rc"
                  exit "$rc"
                fi
              }

              prepare_dogfood_output() {
                if [ -z "$dogfood" ]; then
                  return 0
                fi
                local idx=0
                while [ "$idx" -lt "''${#dogfood_args[@]}" ]; do
                  case "''${dogfood_args[idx]}" in
                    --output)
                      next=$((idx + 1))
                      if [ "$next" -ge "''${#dogfood_args[@]}" ]; then
                        echo "--output requires a path" >&2
                        dogfood_status="fail"
                        write_receipt failed dogfood-selection 2
                        exit 2
                      fi
                      dogfood_output="''${dogfood_args[next]}"
                      case "$dogfood_output" in
                        /*) ;;
                        *)
                          dogfood_output="$invocation_cwd/$dogfood_output"
                          dogfood_args[next]="$dogfood_output"
                          ;;
                      esac
                      return 0
                      ;;
                    --output=*)
                      dogfood_output="''${dogfood_args[idx]#--output=}"
                      case "$dogfood_output" in
                        /*) ;;
                        *) dogfood_output="$invocation_cwd/$dogfood_output" ;;
                      esac
                      dogfood_args[idx]="--output=$dogfood_output"
                      return 0
                      ;;
                  esac
                  idx=$((idx + 1))
                done
                dogfood_output="$invocation_cwd/dogfood-results/replay-readiness-$dogfood-$(date -u +"%Y%m%d-%H%M%S")"
                dogfood_args=(--output "$dogfood_output" "''${dogfood_args[@]}")
              }

              capture_dogfood_summary() {
                if [ -z "$dogfood_output" ]; then
                  return 0
                fi
                if dogfood_summary_json="$(summarize-accepted-dogfood-output --json "$dogfood_output")"; then
                  summarize-accepted-dogfood-output "$dogfood_output"
                else
                  rc=$?
                  dogfood_summary_json="null"
                  return "$rc"
                fi
              }

              run_dogfood() {
                local runner="$1"
                dogfood_status="running"
                prepare_dogfood_output
                echo "dogfood output: $dogfood_output"
                if "$runner" "''${dogfood_args[@]}"; then
                  dogfood_status="pass"
                  capture_dogfood_summary || true
                  write_receipt passed "" 0
                else
                  rc=$?
                  dogfood_status="fail"
                  capture_dogfood_summary || true
                  write_receipt failed dogfood "$rc"
                  exit "$rc"
                fi
              }

              echo "== replay readiness: static checks =="
              cd ${self}
              run_gate contract-registry contract_registry_status check-contract-registry .
              run_gate evidence-contracts evidence_contracts_status check-evidence-contracts --root .
              run_gate replay-proof-coverage replay_proof_coverage_status check-replay-proof-coverage .
              run_gate readiness-promotion readiness_promotion_status check-readiness-promotion-gate --root .
              run_gate readiness-surface-drift readiness_surface_drift_status check-readiness-surface-drift .
              run_gate readiness-report readiness_report_status generate-replay-readiness-report --check .
              run_gate assertion-readiness-report assertion_report_status generate-assertion-readiness-report --check .
              run_gate assertion-readiness-boundary assertion_promotion_status check-assertion-readiness-boundary .
              run_gate sdk-local-report-tracks sdk_local_report_tracks_status check-sdk-local-report-tracks
              run_gate sdk-assertion-quality sdk_assertion_quality_status check-sdk-assertion-quality
              run_gate consistency-checker-fixtures consistency_fixtures_status check-consistency-fixtures .
              run_gate dogfood-artifact-sizes artifact_sizes_status check-dogfood-artifact-sizes
              run_gate accepted-dogfood-config accepted_dogfood_config_status check-accepted-dogfood-config --config ${acceptedVerdictDogfoodConfig} --expectations ${./dogfood-results/accepted-dogfood-expectations.json}
              echo "replay readiness checks passed"

              case "$dogfood" in
                "")
                  echo "no dogfood selected; pass --dogfood <workload> -- <args> for one slow KVM proof rail"
                  write_receipt passed "" 0
                  ;;
                raft)
                  run_dogfood ${acceptedVerdictDogfood.raft}/bin/raft-accepted-verdict-dogfood
                  ;;
                redb)
                  run_dogfood ${acceptedVerdictDogfood.redb}/bin/redb-accepted-verdict-dogfood
                  ;;
                net)
                  run_dogfood ${acceptedVerdictDogfood.net}/bin/net-accepted-verdict-dogfood
                  ;;
                rust-workload)
                  run_dogfood ${acceptedVerdictDogfood.rust-workload}/bin/rust-workload-accepted-verdict-dogfood
                  ;;
                *)
                  echo "unsupported dogfood workload: $dogfood" >&2
                  usage >&2
                  dogfood_status="fail"
                  write_receipt failed dogfood-selection 2
                  exit 2
                  ;;
              esac
            '';
          };

          scaffoldRustWorkload = pkgs.writeShellApplication {
            name = "scaffold-rust-workload";
            runtimeInputs = [
              chaoscontrol
              pkgs.coreutils
            ];
            text = ''
              usage() {
                echo "usage: scaffold-rust-workload DEST [WORKLOAD_NAME]" >&2
              }
              if [ "''${1:-}" = "-h" ] || [ "''${1:-}" = "--help" ]; then
                usage
                exit 0
              fi
              if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
                usage
                exit 2
              fi
              dest="$1"
              workload="''${2:-my-service}"
              if [ -e "$dest" ]; then
                echo "destination already exists: $dest" >&2
                exit 1
              fi
              CHAOSCONTROL_SCAFFOLD_TEMPLATE=${./docs/templates/rust-workload} \
                CHAOSCONTROL_SOURCE_ROOT=${self} \
                exec scaffold-rust-workload "$dest" "$workload"
            '';
          };

          freshRustWorkloadProof = pkgs.writeShellApplication {
            name = "fresh-rust-workload-proof";
            runtimeInputs = [
              pkgs.cargo
              pkgs.coreutils
              pkgs.rustc
              scaffoldRustWorkload
              acceptedVerdictDogfood.rust-workload
            ];
            text = ''
              usage() {
                cat <<'EOF'
              usage: fresh-rust-workload-proof --scaffold DIR --output DIR [--name NAME]

              Creates and builds a Rust workload scaffold, then runs the bounded
              downstream-shaped KVM snapshot/replay rail. The KVM result is a
              cohort-scoped onboarding classification, not proof for arbitrary
              scaffold code.
              EOF
              }

              scaffold=""
              output=""
              name="my-service"
              while [ "$#" -gt 0 ]; do
                case "$1" in
                  -h|--help)
                    usage
                    exit 0
                    ;;
                  --scaffold)
                    scaffold="$2"
                    shift 2
                    ;;
                  --output)
                    output="$2"
                    shift 2
                    ;;
                  --name)
                    name="$2"
                    shift 2
                    ;;
                  *)
                    echo "unknown argument: $1" >&2
                    usage >&2
                    exit 2
                    ;;
                esac
              done
              if [ -z "$scaffold" ] || [ -z "$output" ]; then
                usage >&2
                exit 2
              fi

              scaffold-rust-workload "$scaffold" "$name"
              cargo build --manifest-path "$scaffold/Cargo.toml"

              success_status=0
              no_bug_status=2
              set +e
              rust-workload-accepted-verdict-dogfood \
                --output "$output" \
                --evidence-prefix "$output"
              proof_status="$?"
              set -e
              case "$proof_status" in
                "$success_status")
                  echo "promotion classification: promoted-bounded"
                  echo "evidence: $output/accepted-snapshot-verdict-summary.json"
                  ;;
                "$no_bug_status")
                  echo "promotion classification: diagnostic-no-bug"
                  echo "evidence: $output/attempts-summary.json"
                  ;;
                *)
                  echo "promotion classification: blocked (rail exit $proof_status)" >&2
                  exit "$proof_status"
                  ;;
              esac
            '';
          };

          replayReadinessSummary = pkgs.writeShellApplication {
            name = "replay-readiness-summary";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-summary "$@"
            '';
          };

          replayReadinessDashboard = pkgs.writeShellApplication {
            name = "replay-readiness-dashboard";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-dashboard "$@"
            '';
          };

          replayReadinessTriage = pkgs.writeShellApplication {
            name = "replay-readiness-triage";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-triage "$@"
            '';
          };

          replayReadinessFleetIndex = pkgs.writeShellApplication {
            name = "replay-readiness-fleet-index";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-fleet-index "$@"
            '';
          };

          replayReadinessDecisionReceipt = pkgs.writeShellApplication {
            name = "replay-readiness-decision-receipt";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-decision-receipt "$@"
            '';
          };

          replayReadinessSchedulerReceipt = pkgs.writeShellApplication {
            name = "replay-readiness-scheduler-receipt";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-scheduler-receipt "$@"
            '';
          };

          inProcessSimulatorReceipt = pkgs.writeShellApplication {
            name = "in-process-simulator-receipt";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/in-process-simulator-receipt "$@"
            '';
          };

          localMultiHypervisorKvmSmoke = pkgs.writeShellApplication {
            name = "local-multi-hypervisor-kvm-smoke";
            runtimeInputs = [
              chaoscontrol
              replayReadiness
              replayReadinessSchedulerReceipt
            ];
            text = ''
              exec local-multi-hypervisor-kvm-smoke \
                --replay-readiness ${replayReadiness}/bin/replay-readiness \
                --scheduler-receipt ${replayReadinessSchedulerReceipt}/bin/replay-readiness-scheduler-receipt \
                "$@"
            '';
          };

          replayReadinessReadmeStatus = pkgs.writeShellApplication {
            name = "replay-readiness-readme-status";
            runtimeInputs = [ chaoscontrol ];
            text = ''
              exec ${chaoscontrol}/bin/replay-readiness-readme-status "$@"
            '';
          };

          # --- Simulation test runner ---

          mkChaosTest =
            {
              name,
              kernel, # output of mkChaosKernel ($out/vmlinux)
              initrd, # output of mkChaosInitrd (single .gz file)
              vms ? 3,
              rounds ? 50,
              branches ? 8,
              ticks ? 1000,
              seed ? 42,
              mode ? "hybrid",
              diskImage ? null,
              extraArgs ? "",
            }:
            pkgs.runCommand "chaoscontrol-test-${name}"
              {
                nativeBuildInputs = [ chaoscontrol ];
                requiredSystemFeatures = [ "kvm" ];
              }
              ''
                mkdir -p $out
                chaoscontrol-explore run \
                  --kernel ${kernel}/vmlinux \
                  --initrd ${initrd} \
                  --vms ${toString vms} \
                  --rounds ${toString rounds} \
                  --branches ${toString branches} \
                  --ticks ${toString ticks} \
                  --seed ${toString seed} \
                  --mode ${mode} \
                  ${pkgs.lib.optionalString (diskImage != null) "--disk-image ${diskImage}"} \
                  ${extraArgs} \
                  --output $out
              '';
        in
        {
          inherit
            pkgs
            craneLib
            src
            commonArgs
            cargoArtifacts
            chaoscontrol
            mkChaosInitrd
            mkChaosKernel
            mkChaosTest
            ;

          packages = {
            default = chaoscontrol;
            chaoscontrol-vmm = chaoscontrol;
            oci-intake = ociIntake;
            guest-determinism-gate = guestDeterminismGate;

            inherit
              guest-sdk
              guest-determinism-probe
              guest-raft
              guest-net
              guest-redb
              guest-rust-workload
              guest-supervisor
              ;
            inherit
              initrd-sdk
              initrd-determinism-probe
              initrd-raft
              initrd-net
              initrd-redb
              initrd-rust-workload
              initrd-multiprocess
              ;
            inherit redb-disk-image;

            raft-accepted-verdict-dogfood = acceptedVerdictDogfood.raft;
            redb-accepted-verdict-dogfood = acceptedVerdictDogfood.redb;
            net-accepted-verdict-dogfood = acceptedVerdictDogfood.net;
            rust-workload-accepted-verdict-dogfood = acceptedVerdictDogfood.rust-workload;
            accepted-verdict-dogfood-config = acceptedVerdictDogfoodConfig;
            replay-readiness = replayReadiness;
            scaffold-rust-workload = scaffoldRustWorkload;
            fresh-rust-workload-proof = freshRustWorkloadProof;
            vm-determinism-matrix = vmDeterminismMatrix;
            replay-readiness-summary = replayReadinessSummary;
            replay-readiness-dashboard = replayReadinessDashboard;
            replay-readiness-triage = replayReadinessTriage;
            replay-readiness-fleet-index = replayReadinessFleetIndex;
            replay-readiness-decision-receipt = replayReadinessDecisionReceipt;
            replay-readiness-scheduler-receipt = replayReadinessSchedulerReceipt;
            in-process-simulator-receipt = inProcessSimulatorReceipt;
            local-multi-hypervisor-kvm-smoke = localMultiHypervisorKvmSmoke;
            replay-readiness-readme-status = replayReadinessReadmeStatus;

            cargo-tigerstyle = octet.packages.${system}.cargo-tigerstyle;
            tigerstyle-standards = octet.packages.${system}.tigerstyle-standards;
            verified-logic = trellis.packages.${system}.verified-logic;

            determinism-probe-vmlinux = mkChaosKernel { };
            net-vmlinux = mkChaosKernel { virtioNet = true; };
            kcov-vmlinux = mkChaosKernel { kcov = true; };
            kcov-net-vmlinux = mkChaosKernel {
              virtioNet = true;
              kcov = true;
            };

            raft-sim = mkChaosTest {
              name = "raft-sim";
              kernel = mkChaosKernel { virtioNet = true; };
              initrd = initrd-raft;
              vms = 3;
              rounds = 5;
              branches = 4;
              ticks = 500;
              seed = 42;
              mode = "hybrid";
            };

            redb-sim = mkChaosTest {
              name = "redb-sim";
              kernel = mkChaosKernel { };
              initrd = initrd-redb;
              vms = 1;
              rounds = 5;
              branches = 4;
              ticks = 5000;
              seed = 42;
              mode = "hybrid";
              diskImage = redb-disk-image;
            };

            rust-workload-sim = mkChaosTest {
              name = "rust-workload-sim";
              kernel = mkChaosKernel { kcov = true; };
              initrd = initrd-rust-workload;
              vms = 1;
              rounds = 5;
              branches = 4;
              ticks = 500;
              seed = 42;
              mode = "fault-schedule";
            };

            rust-workload-local-report =
              pkgs.runCommand "rust-workload-local-report"
                {
                  nativeBuildInputs = [ chaoscontrol ];
                }
                ''
                  mkdir -p $out
                  export CHAOSCONTROL_SDK_LOCAL_OUTPUT=$out/sdk.jsonl
                  ${guest-rust-workload}/bin/chaoscontrol-rust-workload-guest
                  summarize-sdk-local-output \
                    --input $out/sdk.jsonl \
                    --output $out/report.json \
                    --evidence-class instrumentation-dry-run
                  check-sdk-assertion-quality --input $out/report.json > $out/assertion-quality.json
                '';

            # Generated documentation (mdBook + rustdoc)
            docs =
              let
                # Rustdoc for the full workspace
                rustdoc = craneLib.cargoDoc (
                  commonArgs
                  // {
                    inherit cargoArtifacts;
                    cargoDocExtraArgs = "--workspace --no-deps";
                    RUSTDOCFLAGS = "-D warnings";
                  }
                );
              in
              pkgs.runCommand "chaoscontrol-docs"
                {
                  nativeBuildInputs = [ pkgs.mdbook ];
                }
                ''
                  # Make CLI binaries findable for --help capture.
                  # generate.sh looks in CARGO_TARGET_DIR/{release,debug}/
                  mkdir -p $TMPDIR/cargo-target/release
                  for bin in ${chaoscontrol}/bin/*; do
                    ln -s $bin $TMPDIR/cargo-target/release/$(basename $bin)
                  done
                  export CARGO_TARGET_DIR=$TMPDIR/cargo-target

                  # Generate mdBook source from code
                  bash ${./docs/generate.sh} $TMPDIR/book

                  # Build mdBook
                  mdbook build $TMPDIR/book --dest-dir $out/book

                  # Copy rustdoc alongside
                  cp -r ${rustdoc}/share/doc $out/api
                '';
          };

          checks = {
            # Build the full workspace
            package = chaoscontrol;

            # Repository-owned package, artifact, template, prior-grant, and
            # third-party license boundary.
            license-boundary =
              pkgs.runCommand "chaoscontrol-license-boundary"
                {
                  nativeBuildInputs = [ rustToolchain ];
                }
                ''
                  rustc ${self}/tools/check-license-boundary.rs -o check-license-boundary
                  ./check-license-boundary ${self}
                  touch "$out"
                '';

            # Exact VM Cohort Cargo, lock, Nix, package, and boundary identity.
            vm-cohort-dependency = vmCohortDependencyCheck;
            vm-cohort-adoption-contract = vmCohortAdoptionContractCheck;
            nickel-cohort-exact = nickelCohortCheck;

            # Clippy — deny warnings
            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- --deny warnings";
              }
            );

            # Rust formatting
            fmt = craneLib.cargoFmt {
              inherit src;
              pname = "chaoscontrol";
              version = "0.1.0";
            };

            # Unit tests (KVM integration tests are #[ignore] —
            # the Nix sandbox has no /dev/kvm)
            tests = craneLib.cargoTest (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoExtraArgs = "--lib";
              }
            );

            # r[impl chaoscontrol.state_machine_properties.fast_lane]
            # Portable bounded model agreement and Nickel profile projection.
            property-coverage =
              pkgs.runCommand "property-coverage-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.nickel
                  ];
                }
                ''
                  cd ${self}
                  mkdir -p "$out"
                  nickel export --format json contracts/property-coverage/profiles.ncl > "$out/profiles.json"
                  cmp "$out/profiles.json" contracts/property-coverage/profiles.json
                  for invalid in \
                    contracts/property-coverage/fixtures/invalid/zero-steps.invalid.ncl \
                    contracts/property-coverage/fixtures/invalid/overclaim.invalid.ncl
                  do
                    if nickel export "$invalid" >/dev/null 2>&1; then
                      echo "invalid property profile unexpectedly passed: $invalid" >&2
                      exit 1
                    fi
                  done
                  run-property-lane --lane fast --output "$out/fast-receipt.json"
                  cmp "$out/fast-receipt.json" dogfood-results/state-machine-property-coverage-20260809/fast-receipt.json
                '';

            # Cheap eBPF schema, profile, layout, accounting, and source guard rail.
            ebpf-trace-evidence =
              pkgs.runCommand "ebpf-trace-evidence-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.nickel
                  ];
                }
                ''
                  cd ${self}
                  ebpf-trace-evidence-selftest
                  nickel export contracts/evidence/examples/kvm-ebpf-trace-capture-profile.ncl >/dev/null
                  if nickel export contracts/evidence/fixtures/invalid/ebpf-trace-capture-profile.multi-producer-exact.invalid.ncl >/dev/null 2>&1; then
                    echo "invalid eBPF trace profile unexpectedly passed" >&2
                    exit 1
                  fi
                  touch $out
                '';

            # A tiny malicious guest must reach the production MMIO path without crashing the VMM.
            virtio-malicious-guest-kvm-smoke = craneLib.cargoTest (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoExtraArgs = "-p chaoscontrol-vmm --test virtio_kvm_smoke -- --ignored";
                requiredSystemFeatures = [ "kvm" ];
              }
            );

            # Keep optional profiling instrumentation compiling.
            profiling = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "-p chaoscontrol-vmm -p chaoscontrol-explore --features profiling -- --deny warnings";
              }
            );

            # Rustdoc — deny warnings
            rustdoc = craneLib.cargoDoc (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoDocExtraArgs = "--workspace --no-deps";
                RUSTDOCFLAGS = "-D warnings";
              }
            );

            # Nix formatting
            nixfmt = pkgs.runCommand "nixfmt-check" { nativeBuildInputs = [ pkgs.nixfmt ]; } ''
              cd ${self}
              nixfmt --check flake.nix
              touch $out
            '';

            # RustSec dependency vulnerability audit over the locked workspace.
            dependency-audit =
              pkgs.runCommand "dependency-audit-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.cargo-audit
                  ];
                }
                ''
                  cargo-audit audit --no-fetch --stale --db ${advisory-db} --file ${self}/Cargo.lock --json > "$TMPDIR/audit.json"
                  check-cargo-audit-report \
                    --report "$TMPDIR/audit.json" \
                    --allowlist ${self}/audits/cargo-audit-warning-allowlist.json
                  mkdir -p "$out"
                  cp "$TMPDIR/audit.json" "$out/cargo-audit.json"
                  cp ${self}/audits/cargo-audit-warning-allowlist.json "$out/cargo-audit-warning-allowlist.json"
                '';

            # Cargo-deny dependency policy over license, ban, and source hygiene.
            dependency-policy = craneLib.cargoDeny (
              commonArgs
              // {
                cargoDenyExtraArgs = "--locked";
                cargoDenyChecks = "bans licenses sources";
              }
            );

            # Portable exact-snapshot descriptor, projection, and consumer fixture.
            snapshot-descriptor-contracts =
              pkgs.runCommand "snapshot-descriptor-contracts-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.nickel
                  ];
                }
                ''
                  cd ${self}
                  check-snapshot-descriptor-contracts --root . --check
                  snapshot-descriptor-fixture --out "$TMPDIR/snapshot-descriptor-fixture"
                  test -s "$TMPDIR/snapshot-descriptor-fixture/snapshot-descriptor.monolithic.json"
                  test -s "$TMPDIR/snapshot-descriptor-fixture/snapshot-descriptor.chunked.json"
                  test -s "$TMPDIR/snapshot-descriptor-fixture/snapshot-restore-receipt.json"
                  test -s "$TMPDIR/snapshot-descriptor-fixture/molten-shaped-snapshot-reference.json"
                  touch "$out"
                '';

            # Nickel-backed evidence contracts and committed dogfood receipt data.
            evidence-contracts =
              pkgs.runCommand "evidence-contracts-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.nickel
                  ];
                }
                ''
                  cd ${self}
                  check-contract-registry .
                  check-sim-core-purity .
                  check-architecture-boundaries .
                  check-rust-product-automation-source .
                  check-cargo-audit-report --selftest
                  summarize-accepted-dogfood-output \
                    ${self}/dogfood-results/raft-fresh-v2-proof-20260809 \
                    > "$TMPDIR/dogfood-summary.txt"
                  summarize-accepted-dogfood-output \
                    --json \
                    ${self}/dogfood-results/raft-fresh-v2-proof-20260809 \
                    > "$TMPDIR/dogfood-summary.json"
                  mkdir "$TMPDIR/malformed-summary"
                  printf '{invalid\n' > "$TMPDIR/malformed-summary/accepted-snapshot-verdict-summary.json"
                  if summarize-accepted-dogfood-output "$TMPDIR/malformed-summary"; then
                    echo "malformed dogfood summary unexpectedly passed" >&2
                    exit 1
                  fi
                  cp -R ${self}/dogfood-results/raft-20260506-131815 "$TMPDIR/materialize-dogfood"
                  chmod -R u+w "$TMPDIR/materialize-dogfood"
                  materialize-dogfood-receipt \
                    "$TMPDIR/materialize-dogfood" \
                    --git-revision fixture \
                    --replay-status accepted \
                    --replay-message no-bug \
                    --replay-exit-status 0
                  test -s "$TMPDIR/materialize-dogfood/run-config.json"
                  test -s "$TMPDIR/materialize-dogfood/receipt.json"
                  CHAOSCONTROL_SCAFFOLD_TEMPLATE=${self}/docs/templates/rust-workload \
                    CHAOSCONTROL_SOURCE_ROOT=${self} \
                    scaffold-rust-workload "$TMPDIR/scaffold" parity-fixture
                  test -s "$TMPDIR/scaffold/chaoscontrol-scaffold.json"
                  if CHAOSCONTROL_SCAFFOLD_TEMPLATE=${self}/docs/templates/rust-workload \
                    CHAOSCONTROL_SOURCE_ROOT=${self} \
                    scaffold-rust-workload "$TMPDIR/scaffold" parity-fixture; then
                    echo "existing scaffold destination unexpectedly passed" >&2
                    exit 1
                  fi
                  if CHAOSCONTROL_SCAFFOLD_TEMPLATE=${self}/docs/templates/rust-workload \
                    CHAOSCONTROL_SOURCE_ROOT=${self} \
                    scaffold-rust-workload ${self}/write-must-fail parity-fixture; then
                    echo "read-only scaffold destination unexpectedly passed" >&2
                    exit 1
                  fi
                  check-evidence-contracts --root .
                  check-kvm-release-matrix --root .
                  check-profile-admission run contracts/evidence/fixtures/valid/run-profile.valid.json contracts/evidence/fixtures/valid/run-profile.projection-receipt.json
                  check-profile-admission simulator contracts/evidence/fixtures/valid/simulator-profile.valid.json contracts/evidence/fixtures/valid/simulator-profile.projection-receipt.json
                  check-profile-admission campaign contracts/evidence/fixtures/valid/campaign-profile.valid.json contracts/evidence/fixtures/valid/campaign-profile.projection-receipt.json
                  check-profile-admission schedule contracts/evidence/fixtures/valid/fault-schedule-profile.valid.json contracts/evidence/fixtures/valid/fault-schedule-profile.projection-receipt.json
                  nickel export --format json contracts/fresh-workload-proofs/cohort.ncl > "$TMPDIR/fresh-workload-proof-cohort.json"
                  cmp "$TMPDIR/fresh-workload-proof-cohort.json" contracts/fresh-workload-proofs/cohort.json
                  if nickel export contracts/fresh-workload-proofs/fixtures/invalid/missing-non-claim.invalid.ncl >/dev/null 2>&1; then
                    echo "invalid fresh workload proof cohort unexpectedly passed" >&2
                    exit 1
                  fi
                  check-replay-proof-coverage .
                  check-replay-proof-coverage --check-doc .
                  materialize-snapshot-chunks --selftest
                  check-readiness-promotion-gate --root .
                  check-readiness-surface-drift .
                  replay-readiness-triage --root . --sample-receipt --check docs/operator-triage-runbook.md
                  generate-replay-readiness-report --check .
                  generate-assertion-readiness-report --check .
                  check-assertion-readiness-boundary .
                  check-sdk-assertion-quality
                  check-dogfood-artifact-sizes
                  check-accepted-dogfood-config --config ${acceptedVerdictDogfoodConfig} --expectations ${./dogfood-results/accepted-dogfood-expectations.json}
                  touch $out
                '';

            # Exact, remeasured SpaceWasm MVP differential evidence against Wasmtime.
            spacewasm-mvp-differential =
              pkgs.runCommand "spacewasm-mvp-differential-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.nickel
                    pkgs.wasmtime
                    pkgs.b3sum
                    pkgs.jq
                    spacewasmResumeProbe
                  ];
                }
                ''
                  mkdir -p "$out"
                  nickel export --format json \
                    ${self}/contracts/evidence/examples/spacewasm-mvp-differential-profile.ncl \
                    > "$out/profile.json"
                  cmp "$out/profile.json" \
                    ${self}/contracts/evidence/examples/spacewasm-mvp-differential-profile.json
                  if nickel export --format json \
                    ${self}/contracts/evidence/fixtures/invalid/spacewasm-mvp-differential-profile.post-mvp.invalid.ncl \
                    > /dev/null 2>&1; then
                    echo "post-MVP negative profile unexpectedly passed" >&2
                    exit 1
                  fi
                  chaoscontrol-wasm-differential \
                    --profile "$out/profile.json" \
                    --bundle ${mantleSpacewasmBundle} \
                    --wasmtime ${pkgs.wasmtime}/bin/wasmtime \
                    --out "$out/report.json" \
                    --artifacts "$out/modules"
                  jq -e '
                    [.comparisons[] | select(.case_id == "mvp-positive" or .case_id == "streaming-positive")]
                    | length == 2
                    and .[0].module_blake3 == .[1].module_blake3
                    and all(.[]; .verdict == "match")
                  ' "$out/report.json" > /dev/null
                  spacewasm-resume-probe \
                    "$out/modules/generated-valid-0000.wasm" \
                    "$out/resume-report.raw.json"
                  moduleDigest="$(b3sum --no-names "$out/modules/generated-valid-0000.wasm")"
                  probeDigest="$(b3sum --no-names ${spacewasmResumeProbe}/bin/spacewasm-resume-probe)"
                  profileIdentity="$(jq -r .profile_identity_blake3 "$out/report.json")"
                  jq --sort-keys \
                    --arg moduleDigest "$moduleDigest" \
                    --arg probeDigest "$probeDigest" \
                    --arg profileIdentity "$profileIdentity" \
                    '. + {
                      module_blake3: $moduleDigest,
                      probe_blake3: $probeDigest,
                      profile_identity_blake3: $profileIdentity,
                      evidence_role: "diagnostic-only",
                      non_claims: [
                        "not-portable-interpreter-state",
                        "not-spacewasm-correctness",
                        "not-webassembly-conformance"
                      ]
                    }' \
                    "$out/resume-report.raw.json" > "$out/resume-report.json"
                  jq -e --slurpfile profile "$out/profile.json" '
                    .source_revision == $profile[0].spacewasm_revision
                    and .segment_fuel == $profile[0].runtime.spacewasm_resume_segment_fuel
                    and .maximum_segments == $profile[0].runtime.maximum_resume_segments
                    and .segments > 1
                    and .segments <= .maximum_segments
                    and .stream_chunk_bytes == 1
                    and .uninterrupted == "finished"
                    and .segmented == "finished"
                    and .streaming == "finished"
                  ' "$out/resume-report.json" > /dev/null
                  rm "$out/resume-report.raw.json"
                '';

            # CI/dashboard-facing replay readiness receipt plus stable summary line.
            replay-readiness =
              pkgs.runCommand "replay-readiness-check"
                {
                  nativeBuildInputs = [
                    replayReadiness
                    replayReadinessSummary
                    replayReadinessDashboard
                    replayReadinessTriage
                    replayReadinessFleetIndex
                    replayReadinessDecisionReceipt
                    replayReadinessSchedulerReceipt
                    inProcessSimulatorReceipt
                  ];
                }
                ''
                  mkdir -p "$out"
                  receipt="$out/replay-readiness-receipt.json"
                  replay-readiness --receipt "$receipt"
                  replay-readiness-summary "$receipt" | tee "$out/replay-readiness-summary.txt"
                  replay-readiness-dashboard "$receipt" --output "$out/replay-readiness-dashboard.html"
                  replay-readiness-triage "$receipt" --root ${self} --output "$out/operator-triage-runbook.md"
                  replay-readiness-fleet-index --output "$out/fleet-triage-index.html" "$receipt"
                  replay-readiness-decision-receipt --sample --output "$out/decision-receipt.json"
                  replay-readiness-decision-receipt --check "$out/decision-receipt.json" > "$out/decision-receipt-summary.txt"
                  replay-readiness-scheduler-receipt --sample --output "$out/scheduler-receipt.json"
                  replay-readiness-scheduler-receipt --check "$out/scheduler-receipt.json" > "$out/scheduler-receipt-summary.txt"
                  in-process-simulator-receipt --sample --output "$out/in-process-simulator-receipt.json" > "$out/in-process-simulator-summary.txt"
                  in-process-simulator-receipt --check "$out/in-process-simulator-receipt.json" >> "$out/in-process-simulator-summary.txt"
                  replay-readiness-scheduler-receipt \
                    --materialize-ci-plans "$out" \
                    --replay-readiness "$(command -v replay-readiness)"
                  replay-readiness-scheduler-receipt --run-plan "$out/scheduler-execution-plan.json" --output "$out/scheduler-execution-receipt.json" > "$out/scheduler-execution-summary.txt"
                  replay-readiness-scheduler-receipt --check-execution "$out/scheduler-execution-receipt.json" >> "$out/scheduler-execution-summary.txt"
                  replay-readiness-scheduler-receipt --run-fleet-plan "$out/fleet-scheduler-plan.json" --output "$out/fleet-scheduler-receipt.json" > "$out/fleet-scheduler-summary.txt"
                  replay-readiness-scheduler-receipt --check-fleet "$out/fleet-scheduler-receipt.json" >> "$out/fleet-scheduler-summary.txt"
                  replay-readiness-scheduler-receipt --run-multi-hypervisor-plan "$out/local-multi-hypervisor-campaign-plan.json" --output "$out/local-multi-hypervisor-campaign-receipt.json" > "$out/local-multi-hypervisor-campaign-summary.txt"
                  replay-readiness-scheduler-receipt --check-multi-hypervisor "$out/local-multi-hypervisor-campaign-receipt.json" >> "$out/local-multi-hypervisor-campaign-summary.txt"
                  replay-readiness-scheduler-receipt --render-multi-hypervisor-dashboard "$out/local-multi-hypervisor-campaign-receipt.json" --output "$out/local-multi-hypervisor-dashboard.html" >> "$out/local-multi-hypervisor-campaign-summary.txt"
                  replay-readiness-scheduler-receipt --run-hosted-shared-state-plan "$out/hosted-shared-state-plan.json" --output "$out/hosted-shared-state-receipt.json" > "$out/hosted-shared-state-summary.txt"
                  replay-readiness-scheduler-receipt --check-hosted-shared-state "$out/hosted-shared-state-receipt.json" >> "$out/hosted-shared-state-summary.txt"
                  replay-readiness-scheduler-receipt --run-networked-hosted-plan "$out/networked-hosted-scheduler-plan.json" --output "$out/networked-hosted-scheduler-receipt.json" > "$out/networked-hosted-scheduler-summary.txt"
                  replay-readiness-scheduler-receipt --check-networked-hosted "$out/networked-hosted-scheduler-receipt.json" >> "$out/networked-hosted-scheduler-summary.txt"
                  test -s "$receipt"
                  test -s "$out/replay-readiness-summary.txt"
                  test -s "$out/replay-readiness-dashboard.html"
                  test -s "$out/operator-triage-runbook.md"
                  test -s "$out/fleet-triage-index.html"
                  test -s "$out/decision-receipt.json"
                  test -s "$out/decision-receipt-summary.txt"
                  test -s "$out/scheduler-receipt.json"
                  test -s "$out/scheduler-receipt-summary.txt"
                  test -s "$out/in-process-simulator-receipt.json"
                  test -s "$out/in-process-simulator-summary.txt"
                  test -s "$out/scheduler-execution-plan.json"
                  test -s "$out/scheduler-execution-receipt.json"
                  test -s "$out/scheduler-execution-summary.txt"
                  test -s "$out/fleet-scheduler-plan.json"
                  test -s "$out/fleet-scheduler-receipt.json"
                  test -s "$out/fleet-scheduler-summary.txt"
                  test -s "$out/fleet-scheduler-state.json"
                  test -s "$out/fleet-scheduled-run-1.json"
                  test -s "$out/fleet-scheduled-run-2.json"
                  test -s "$out/local-multi-hypervisor-campaign-plan.json"
                  test -s "$out/local-multi-hypervisor-campaign-receipt.json"
                  test -s "$out/local-multi-hypervisor-campaign-summary.txt"
                  test -s "$out/local-multi-hypervisor-dashboard.html"
                  test -s "$out/local-multi-hypervisor-campaign-state.json"
                  test -s "$out/local-multi-hypervisor-run-1.json"
                  test -s "$out/local-multi-hypervisor-run-2.json"
                  test -s "$out/hosted-shared-state-plan.json"
                  test -s "$out/hosted-shared-state-receipt.json"
                  test -s "$out/hosted-shared-state-summary.txt"
                  test -s "$out/hosted-shared-queue-state.json"
                  test -s "$out/hosted-shared-decision-store.json"
                  test -s "$out/hosted-run-1.json"
                  test -s "$out/hosted-run-2.json"
                  test -s "$out/networked-hosted-scheduler-plan.json"
                  test -s "$out/networked-hosted-scheduler-receipt.json"
                  test -s "$out/networked-hosted-scheduler-summary.txt"
                  test -s "$out/networked-hosted-queue-state.json"
                  test -s "$out/networked-hosted-decision-store.json"
                  test -s "$out/networked-run-1.json"
                  test -s "$out/networked-run-2.json"
                  test -s "$out/scheduled-run-1.json"
                  test -s "$out/scheduled-run-2.json"
                '';

            # Cheap CI/report guard for the latest packaged hide-TSC VM drift receipt.
            vm-determinism-drift-receipt =
              pkgs.runCommand "vm-determinism-drift-receipt-check"
                {
                  nativeBuildInputs = [ chaoscontrol ];
                }
                ''
                  mkdir -p "$out"
                  check-vm-determinism-drift-receipt \
                    ${./dogfood-results/vm-determinism-drift-latest/receipt.json} \
                    > "$out/summary.txt"
                  cp ${./dogfood-results/vm-determinism-drift-latest/receipt.json} "$out/receipt.json"
                  test -s "$out/summary.txt"
                  test -s "$out/receipt.json"
                '';

            # KVM-required smoke gate for the snapshot-backed Raft replay rail.
            snapshot-replay-smoke =
              pkgs.runCommand "snapshot-replay-smoke-check"
                {
                  nativeBuildInputs = [
                    chaoscontrol
                    pkgs.coreutils
                  ];
                  requiredSystemFeatures = [ "kvm" ];
                }
                ''
                  KERNEL=${mkChaosKernel { virtioNet = true; }}/vmlinux \
                    INITRD=${initrd-raft} \
                    OUT=$out \
                    ${pkgs.bash}/bin/bash ${./scripts/snapshot-replay-smoke.sh}
                '';

            local-multi-hypervisor-kvm-smoke =
              pkgs.runCommand "local-multi-hypervisor-kvm-smoke-check"
                {
                  nativeBuildInputs = [ localMultiHypervisorKvmSmoke ];
                  requiredSystemFeatures = [ "kvm" ];
                }
                ''
                  local-multi-hypervisor-kvm-smoke --out "$out"
                  test -s "$out/campaign-plan.json"
                  test -s "$out/campaign-receipt.json"
                  test -s "$out/campaign-state.json"
                  test -s "$out/summary.txt"
                '';

            # Track the local sibling proof/style repos used by this workspace.
            tigerstyle-policy-registry = octet.checks.${system}.policy-registry;
            vm-cohort-adapter-octet-deny-all =
              (octet.lib.mkConsumerCheck {
                inherit system;
                src = vmCohortAdapterOctetWorkspace;
                cargoLock = ./checks/vm-cohort-adapter-octet/Cargo.lock;
                packages = [ "chaoscontrol-vm-cohort-adapter" ];
                cargoExtraArgs = "--all-targets --all-features";
                nativeBuildInputs = [ pkgs.stdenv.cc ];
              }).overrideAttrs
                (_previous: {
                  DYLINT_RUSTFLAGS = "--deny warnings";
                });
            snapshot-descriptor-octet-deny-all =
              (octet.lib.mkConsumerCheck {
                inherit system;
                src = tigerstyleSrc;
                cargoLock = ./Cargo.lock;
                packages = [ "chaoscontrol-snapshot-descriptor" ];
                cargoExtraArgs = "--all-targets --all-features";
                nativeBuildInputs = [ pkgs.stdenv.cc ];
              }).overrideAttrs
                (_previous: {
                  DYLINT_RUSTFLAGS = "--deny warnings";
                });
            tigerstyle-chaoscontrol-focused = octet.lib.mkConsumerCheck {
              inherit system;
              src = tigerstyleSrc;
              cargoLock = ./Cargo.lock;
              nativeBuildInputs = [
                pkgs.llvmPackages.clang-unwrapped
                pkgs.bpftools
                pkgs.pkg-config
                pkgs.stdenv.cc
              ];
              buildInputs = [
                pkgs.elfutils
                pkgs.zlib
                pkgs.libbpf
              ];
              packages = [
                "chaoscontrol-protocol"
                "chaoscontrol-sdk"
                "chaoscontrol-fault"
                "chaoscontrol-vmm"
                "chaoscontrol-trace"
                "chaoscontrol-explore"
                "chaoscontrol-dashboard"
                "chaoscontrol-replay"
                "chaoscontrol-evidence"
                "chaoscontrol-guest"
                "chaoscontrol-raft-guest"
                "chaoscontrol-guest-net"
                "chaoscontrol-net-guest"
                "chaoscontrol-redb-guest"
                "chaoscontrol-rust-workload-guest"
              ];
              cargoExtraArgs = "--lib";
            };
            verified-logic-verus-proofs = trellis.checks.${system}.verus-proofs;

            # Simulation tests live in packages, not checks — they take
            # 10+ minutes and need /dev/kvm.  Run explicitly:
            #   nix build .#raft-sim
            #   nix build .#redb-sim
            #   nix run .#explore-raft
            #   nix run .#explore-redb
          };

          apps = {
            default = mkApp "Boot a ChaosControl VM from explicit kernel/initrd arguments." "${chaoscontrol}/bin/boot";
            boot = mkApp "Boot a ChaosControl VM from explicit kernel/initrd arguments." "${chaoscontrol}/bin/boot";
            guest-determinism-gate = mkApp "Run the bounded guest OS bit-exact drift gate." "${guestDeterminismGate}/bin/guest-determinism-gate";
            snapshot-demo = mkApp "Run the local ChaosControl snapshot demo." "${chaoscontrol}/bin/snapshot_demo";
            explore = mkApp "Run the ChaosControl explorer with caller-supplied arguments." "${chaoscontrol}/bin/chaoscontrol-explore";
            oci-intake = mkApp "Lower a bounded image topology into a guest process bundle." "${ociIntake}/bin/oci-intake";
            scaffold-rust-workload = mkApp "Copy the Rust workload harness template and write explicit local/VM promotion commands." "${scaffoldRustWorkload}/bin/scaffold-rust-workload";
            explore-raft =
              let
                wrapper = pkgs.writeShellApplication {
                  name = "explore-raft";
                  runtimeInputs = [ chaoscontrol ];
                  text = ''
                    chaoscontrol-explore run \
                      --kernel ${mkChaosKernel { virtioNet = true; }}/vmlinux \
                      --initrd ${initrd-raft} \
                      --vms 3 --rounds 100 --branches 8 --ticks 1000 \
                      --seed 42 --mode hybrid \
                      "$@"
                  '';
                };
              in
              mkApp "Run the bounded Raft exploration wrapper." "${wrapper}/bin/explore-raft";
            explore-redb =
              let
                wrapper = pkgs.writeShellApplication {
                  name = "explore-redb";
                  runtimeInputs = [ chaoscontrol ];
                  text = ''
                    chaoscontrol-explore run \
                      --kernel ${mkChaosKernel { }}/vmlinux \
                      --initrd ${initrd-redb} \
                      --disk-image ${redb-disk-image} \
                      --vms 1 --rounds 100 --branches 8 --ticks 5000 \
                      --seed 42 --mode hybrid \
                      "$@"
                  '';
                };
              in
              mkApp "Run the bounded redb exploration wrapper." "${wrapper}/bin/explore-redb";
            explore-sdk =
              let
                wrapper = pkgs.writeShellApplication {
                  name = "explore-sdk";
                  runtimeInputs = [ chaoscontrol ];
                  text = ''
                    chaoscontrol-explore run \
                      --kernel ${mkChaosKernel { }}/vmlinux \
                      --initrd ${initrd-sdk} \
                      --vms 1 --rounds 50 --branches 8 --ticks 500 \
                      --seed 42 --mode fault-schedule \
                      "$@"
                  '';
                };
              in
              mkApp "Run the SDK guest exploration wrapper." "${wrapper}/bin/explore-sdk";
            rust-workload-local-report =
              let
                wrapper = pkgs.writeShellApplication {
                  name = "rust-workload-local-report";
                  runtimeInputs = [
                    guest-rust-workload
                    chaoscontrol
                    pkgs.coreutils
                  ];
                  text = ''
                    out="''${1:-./chaoscontrol-rust-workload-local-report}"
                    mkdir -p "$out"
                    export CHAOSCONTROL_SDK_LOCAL_OUTPUT="$out/sdk.jsonl"
                    rm -f "$CHAOSCONTROL_SDK_LOCAL_OUTPUT"
                    chaoscontrol-rust-workload-guest
                    summarize-sdk-local-output \
                      --input "$CHAOSCONTROL_SDK_LOCAL_OUTPUT" \
                      --output "$out/report.json" \
                      --evidence-class instrumentation-dry-run
                    printf 'local report: %s\n' "$out/report.json"
                  '';
                };
              in
              mkApp "Generate the local rust-workload SDK instrumentation report." "${wrapper}/bin/rust-workload-local-report";
            explore-rust-workload =
              let
                wrapper = pkgs.writeShellApplication {
                  name = "explore-rust-workload";
                  runtimeInputs = [
                    chaoscontrol
                    pkgs.coreutils
                  ];
                  text = ''
                    out="''${1:-./chaoscontrol-rust-workload-vm-report}"
                    shift || true
                    mkdir -p "$out"
                    printf '{"schema":"chaoscontrol.vm_campaign.classification.v1","evidence_class":"bounded-vm-campaign","initrd":"%s","replay_boundary":"campaign output may contain VM execution evidence; standalone replay proof still requires replay/minimization artifacts"}\n' \
                      "${initrd-rust-workload}" > "$out/evidence-classification.json"
                    chaoscontrol-explore run \
                      --kernel ${mkChaosKernel { kcov = true; }}/vmlinux \
                      --initrd ${initrd-rust-workload} \
                      --vms 1 --rounds 5 --branches 4 --ticks 500 \
                      --seed 42 --mode fault-schedule \
                      --output "$out" \
                      "$@"
                    printf 'vm campaign output: %s\n' "$out"
                  '';
                };
              in
              mkApp "Run the bounded rust-workload VM campaign wrapper." "${wrapper}/bin/explore-rust-workload";
            raft-accepted-verdict-dogfood = mkApp "Run the accepted-verdict Raft dogfood proof rail." "${acceptedVerdictDogfood.raft}/bin/raft-accepted-verdict-dogfood";
            redb-accepted-verdict-dogfood = mkApp "Run the accepted-verdict redb dogfood proof rail." "${acceptedVerdictDogfood.redb}/bin/redb-accepted-verdict-dogfood";
            net-accepted-verdict-dogfood = mkApp "Run the accepted-verdict network dogfood proof rail." "${acceptedVerdictDogfood.net}/bin/net-accepted-verdict-dogfood";
            rust-workload-accepted-verdict-dogfood = mkApp "Run the accepted-verdict rust-workload dogfood proof rail." "${acceptedVerdictDogfood.rust-workload}/bin/rust-workload-accepted-verdict-dogfood";
            fresh-rust-workload-proof = mkApp "Build a Rust scaffold and run the bounded onboarding proof classification." "${freshRustWorkloadProof}/bin/fresh-rust-workload-proof";
            check-kvm-release-matrix = mkApp "Validate the typed KVM release matrix and adversarial receipt fixtures." "${chaoscontrol}/bin/check-kvm-release-matrix";
            kvm-release-matrix = mkApp "Run the required bounded KVM release matrix and emit one receipt." "${chaoscontrol}/bin/run-kvm-release-matrix";
            replay-readiness = mkApp "Run committed replay readiness gates and optionally one dogfood rail." "${replayReadiness}/bin/replay-readiness";
            vm-determinism-drift = mkApp "Run the bounded hide-tsc VM determinism drift gate and emit a receipt." "${vmDeterminismDrift}/bin/vm-determinism-drift";
            vm-determinism-matrix = mkApp "Run the bounded hide-tsc VM determinism matrix rail and emit receipts." "${vmDeterminismMatrix}/bin/vm-determinism-matrix";
            replay-readiness-summary = mkApp "Summarize a replay readiness receipt." "${replayReadinessSummary}/bin/replay-readiness-summary";
            replay-readiness-dashboard = mkApp "Render a replay readiness receipt as an HTML dashboard." "${replayReadinessDashboard}/bin/replay-readiness-dashboard";
            replay-readiness-triage = mkApp "Render a local operator triage runbook from a replay readiness receipt." "${replayReadinessTriage}/bin/replay-readiness-triage";
            replay-readiness-fleet-index = mkApp "Render a static multi-receipt fleet triage index." "${replayReadinessFleetIndex}/bin/replay-readiness-fleet-index";
            replay-readiness-decision-receipt = mkApp "Write or validate a bounded local replay-readiness decision receipt." "${replayReadinessDecisionReceipt}/bin/replay-readiness-decision-receipt";
            replay-readiness-scheduler-receipt = mkApp "Write or validate a bounded local replay-readiness scheduler receipt." "${replayReadinessSchedulerReceipt}/bin/replay-readiness-scheduler-receipt";
            in-process-simulator-receipt = mkApp "Emit or validate a bounded in-process simulator receipt." "${inProcessSimulatorReceipt}/bin/in-process-simulator-receipt";
            local-multi-hypervisor-kvm-smoke = mkApp "Run the bounded local KVM multi-hypervisor replay-readiness smoke rail." "${localMultiHypervisorKvmSmoke}/bin/local-multi-hypervisor-kvm-smoke";
            replay-readiness-readme-status = mkApp "Check README replay-readiness status against a receipt." "${replayReadinessReadmeStatus}/bin/replay-readiness-readme-status";
          };

          devShell = pkgs.mkShell {
            buildInputs = [
              (pkgs.rust-bin.stable.latest.default.override {
                extensions = [
                  "rust-src"
                  "rust-analyzer"
                ];
                targets = [ "x86_64-unknown-linux-musl" ];
              })
              pkgs.cargo-watch
              pkgs.cargo-edit
              pkgs.nickel

              # eBPF tracing harness
              pkgs.clang
              pkgs.libbpf
              pkgs.bpftools
              pkgs.elfutils
              pkgs.zlib
              pkgs.pkg-config

              # Guest binary (musl static linking)
              pkgs.pkgsCross.musl64.stdenv.cc

              # Local sibling proof/style tools
              octet.packages.${system}.cargo-tigerstyle
              octet.packages.${system}.tigerstyle-standards
              trellis.packages.${system}.verified-logic

              # Nix formatting (matches CI check)
              pkgs.nixfmt
            ];

            nativeBuildInputs = [ pkgs.pkg-config ];

            CLANG = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";

            shellHook = ''
              export PATH="$PWD/scripts:$PATH"

              echo "ChaosControl development environment"
              echo "Rust: $(rustc --version)"
              echo "Clang: $(clang --version | head -1)"
              echo ""
              echo "Commands:"
              echo "  cargo build              Build VMM + tools"
              echo "  cargo test               Run unit tests"
              echo "  cargo clippy             Lint"
              echo ""
              echo "Guests & Initrds (Nix):"
              echo "  nix build .#guest-raft   Build Raft guest binary"
              echo "  nix build .#initrd-raft  Build Raft initrd image"
              echo "  nix build .#guest-sdk    Build SDK guest binary"
              echo "  nix build .#guest-net    Build net guest binary"
              echo ""
              echo "Exploration:"
              echo "  nix run .#explore-raft   Run Raft exploration (builds everything)"
              echo "  nix run .#explore        Run explorer with manual args"
              echo ""
              echo "Kernels:"
              echo "  nix build .#net-vmlinux       Virtio-net kernel"
              echo "  nix build .#kcov-vmlinux      KCOV kernel"
              echo "  nix build .#kcov-net-vmlinux  Both"
              echo ""
              echo "CI:"
              echo "  nix flake check          All checks (build, test, clippy, fmt)"
              echo "  nix build .#raft-sim     Run simulation test (needs KVM, ~10 min)"
            '';
          };
        }
      );
    in
    {
      packages = forAllSystems (system: eachSystem.${system}.packages);
      checks = forAllSystems (system: eachSystem.${system}.checks);
      apps = forAllSystems (system: eachSystem.${system}.apps);
      devShells = forAllSystems (system: {
        default = eachSystem.${system}.devShell;
      });
      lib = forAllSystems (system: {
        inherit (eachSystem.${system}) mkChaosInitrd mkChaosKernel mkChaosTest;
      });
    };
}
