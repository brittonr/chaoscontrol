{
  description = "ChaosControl — Deterministic VMM for simulation testing";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    octet.url = "git+file:../octet?ref=refs/heads/main&rev=9c7ba87bef2934d2b7b144167e13c8d18eac8958";
    trellis.url = "git+file:../trellis?ref=refs/heads/main&rev=46ab2d92b9cfd2cfc4e631a56f3e667ee7263685";
    advisory-db = {
      url = "github:RustSec/advisory-db";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      rust-overlay,
      octet,
      trellis,
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
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

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
              isDogfoodCheckpointFixture = pkgs.lib.hasPrefix "dogfood-results/raft-20260506-095025/" relPath;
              isDogfoodAssertionHarnessFixture = relPath == "dogfood-results/local-assertion-harnesses.json";
            in
            (craneLib.filterCargoSources path type)
            || isEvidenceFixture
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
            }:
            muslCraneLib.buildPackage (
              muslCommonArgs
              // {
                inherit pname;
                cargoArtifacts = muslCargoArtifacts;
                cargoExtraArgs = "${cargoExtraArgs} --target x86_64-unknown-linux-musl";
              }
            );

          guest-sdk = mkGuestPackage { pname = "chaoscontrol-guest"; };
          guest-raft = mkGuestPackage { pname = "chaoscontrol-raft-guest"; };
          guest-net = mkGuestPackage { pname = "chaoscontrol-net-guest"; };
          guest-redb = mkGuestPackage { pname = "chaoscontrol-redb-guest"; };
          guest-rust-workload = mkGuestPackage { pname = "chaoscontrol-rust-workload-guest"; };

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
              failAfterValues ? defaultSnapshotProbeFailAfterValues,
              maxAttempts ? null,
              expectation ? null,
              diskImage ? null,
            }:
            let
              args = [
                "--workload"
                workload
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
                pkgs.python3
              ];
              text = ''
                python ${./scripts/accepted-snapshot-verdict-dogfood.py} ${pkgs.lib.escapeShellArgs args} "$@"
              '';
            };

          acceptedVerdictDogfoodExpectations = builtins.fromJSON (
            builtins.readFile ./dogfood-results/accepted-dogfood-expectations.json
          );
          acceptedVerdictDogfoodExpectationWorkloads = acceptedVerdictDogfoodExpectations.workloads;

          acceptedVerdictDogfoodWorkloads = {
            raft = {
              name = "raft-accepted-verdict-dogfood";
              workload = "raft";
              kernel = mkChaosKernel { virtioNet = true; };
              initrd = initrd-raft;
              assertionId = 1806003755;
              cmdlineTemplate = "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after={fail_after}";
              vms = 3;
              rounds = 3;
              branches = 2;
              ticks = 80;
              memoryMb = 256;
              failAfterValues = acceptedVerdictDogfoodExpectationWorkloads.raft.runner.fail_after_values;
              maxAttempts = acceptedVerdictDogfoodExpectationWorkloads.raft.runner.max_attempts;
              expectation = acceptedVerdictDogfoodExpectationWorkloads.raft;
            };
            redb = {
              name = "redb-accepted-verdict-dogfood";
              workload = "redb";
              kernel = mkChaosKernel { };
              initrd = initrd-redb;
              diskImage = redb-disk-image;
              assertionId = 2718281828;
              cmdlineTemplate = "redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after={fail_after}";
              vms = 1;
              rounds = 3;
              branches = 2;
              ticks = 80;
              memoryMb = 256;
              failAfterValues = acceptedVerdictDogfoodExpectationWorkloads.redb.runner.fail_after_values;
              maxAttempts = acceptedVerdictDogfoodExpectationWorkloads.redb.runner.max_attempts;
              expectation = acceptedVerdictDogfoodExpectationWorkloads.redb;
            };
            net = {
              name = "net-accepted-verdict-dogfood";
              workload = "net";
              kernel = mkChaosKernel { virtioNet = true; };
              initrd = initrd-net;
              assertionId = 3141592653;
              cmdlineTemplate = "net_bug=snapshot_replay_probe net_snapshot_probe_fail_after={fail_after}";
              vms = 3;
              rounds = 4;
              branches = 3;
              ticks = 120;
              memoryMb = 256;
              failAfterValues = acceptedVerdictDogfoodExpectationWorkloads.net.runner.fail_after_values;
              maxAttempts = acceptedVerdictDogfoodExpectationWorkloads.net.runner.max_attempts;
              expectation = acceptedVerdictDogfoodExpectationWorkloads.net;
            };
            rust-workload = {
              name = "rust-workload-accepted-verdict-dogfood";
              workload = "rust-workload";
              kernel = mkChaosKernel { kcov = true; };
              initrd = initrd-rust-workload;
              assertionId = 1414213562;
              cmdlineTemplate = "rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after={fail_after}";
              vms = 1;
              rounds = 3;
              branches = 2;
              ticks = 80;
              memoryMb = 128;
              failAfterValues =
                acceptedVerdictDogfoodExpectationWorkloads."rust-workload".runner.fail_after_values;
              maxAttempts = acceptedVerdictDogfoodExpectationWorkloads."rust-workload".runner.max_attempts;
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
              pkgs.python3
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

              python3 - "$out/matrix-receipt.json" "$out/summary.txt" <<'PY'
              import json
              import sys
              receipt_path, summary_path = sys.argv[1:]
              receipt = json.loads(open(receipt_path, encoding="utf-8").read())
              rows = receipt.get("rows", [])
              passed = receipt.get("passed") is True
              lines = [
                  f"vm determinism matrix: {'pass' if passed else 'fail'}",
                  f"matrix_id: {receipt.get('matrix_id')}",
                  f"gate: {receipt.get('gate')}",
                  f"rows: {len(rows)}",
                  f"scope: {receipt.get('scope')}",
              ]
              for row in rows:
                  profile = row.get("profile", {})
                  report = row.get("report", {})
                  lines.append(
                      f"- {profile.get('row_id')}: status={row.get('status')} passed={report.get('passed')} runs={report.get('runs')} product={profile.get('local_product_profile')} workers={profile.get('worker_count')} workload={profile.get('workload')} kernel={profile.get('kernel_fingerprint')} initrd={profile.get('initrd_fingerprint')} device={profile.get('device_profile')} clock={profile.get('clock_profile')} controller={profile.get('controller_profile')} hypervisor={profile.get('hypervisor_profile')} mismatches={len(report.get('mismatches', []))}"
                  )
              with open(summary_path, "w", encoding="utf-8") as fh:
                  fh.write("\n".join(lines) + "\n")
              print("\n".join(lines))
              PY
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
              pkgs.python3
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
                python - "$receipt" <<'PY'
              import json
              import os
              import sys
              from pathlib import Path

              out = Path(sys.argv[1])
              dogfood = os.environ["DOGFOOD"] or None
              dogfood_output = os.environ["DOGFOOD_OUTPUT"] or None
              dogfood_summary_raw = os.environ["DOGFOOD_SUMMARY_JSON"] or "null"
              try:
                  dogfood_summary = json.loads(dogfood_summary_raw)
              except json.JSONDecodeError as exc:
                  raise SystemExit(f"invalid DOGFOOD_SUMMARY_JSON: {exc}")
              if dogfood_summary is not None and not isinstance(dogfood_summary, dict):
                  raise SystemExit("DOGFOOD_SUMMARY_JSON must be an object or null")

              def load_expectation(workload):
                  if not workload:
                      return None
                  with Path(os.environ["DOGFOOD_EXPECTATIONS"]).open() as handle:
                      root = json.load(handle)
                  value = (root.get("workloads") or {}).get(workload)
                  if value is None:
                      raise SystemExit(f"missing dogfood expectation for {workload}")
                  return value

              def expectation_status(expectation, summary):
                  if expectation is None or summary is None:
                      return "not-applicable" if expectation is None else "not-observed"
                  expected = expectation.get("expected") or {}
                  mismatches = []
                  if summary.get("accepted") is not expected.get("accepted"):
                      mismatches.append("accepted")
                  verdict = summary.get("verdict") if isinstance(summary.get("verdict"), dict) else {}
                  if verdict.get("replay_class") != expected.get("replay_class"):
                      mismatches.append("replay_class")
                  depth = verdict.get("replay_parent_depth")
                  min_depth = expected.get("min_replay_parent_depth")
                  if isinstance(min_depth, int) and (not isinstance(depth, int) or depth < min_depth):
                      mismatches.append("replay_parent_depth")
                  seed = summary.get("seed")
                  allowed_seeds = expected.get("allowed_seeds")
                  if isinstance(allowed_seeds, list) and seed not in allowed_seeds:
                      mismatches.append("seed")
                  fail_after = summary.get("snapshot_probe_fail_after")
                  fail_after_values = expected.get("fail_after_values")
                  if isinstance(fail_after_values, list) and fail_after not in fail_after_values:
                      mismatches.append("fail_after")
                  if mismatches:
                      return "mismatched:" + ",".join(mismatches)
                  return "matched"

              dogfood_expectation = load_expectation(dogfood)
              dogfood_expectation_status = expectation_status(dogfood_expectation, dogfood_summary)

              gates = [
                  ("contract-registry", "check-contract-registry .", os.environ["CONTRACT_REGISTRY_STATUS"]),
                  ("evidence-contracts", "check-evidence-contracts --root .", os.environ["EVIDENCE_CONTRACTS_STATUS"]),
                  ("replay-proof-coverage", "check-replay-proof-coverage .", os.environ["REPLAY_PROOF_COVERAGE_STATUS"]),
                  ("readiness-promotion", "check-readiness-promotion-gate --root .", os.environ["READINESS_PROMOTION_STATUS"]),
                  ("readiness-surface-drift", "check-readiness-surface-drift .", os.environ["READINESS_SURFACE_DRIFT_STATUS"]),
                  ("readiness-report", "generate-replay-readiness-report --check .", os.environ["READINESS_REPORT_STATUS"]),
                  ("assertion-readiness-report", "generate-assertion-readiness-report --check .", os.environ["ASSERTION_REPORT_STATUS"]),
                  ("assertion-readiness-promotion", "check-assertion-readiness-promotion-gate .", os.environ["ASSERTION_PROMOTION_STATUS"]),
                  ("sdk-local-report-tracks", "check-sdk-local-report-tracks", os.environ["SDK_LOCAL_REPORT_TRACKS_STATUS"]),
                  ("sdk-assertion-quality", "check-sdk-assertion-quality", os.environ["SDK_ASSERTION_QUALITY_STATUS"]),
                  ("consistency-checker-fixtures", "check-consistency-fixtures .", os.environ["CONSISTENCY_FIXTURES_STATUS"]),
                  ("dogfood-artifact-sizes", "check-dogfood-artifact-sizes", os.environ["ARTIFACT_SIZES_STATUS"]),
                  ("accepted-dogfood-config", "check-accepted-dogfood-config --config <nix-generated>", os.environ["ACCEPTED_DOGFOOD_CONFIG_STATUS"]),
              ]
              receipt = {
                  "schema_version": 1,
                  "command": "replay-readiness",
                  "status": os.environ["STATUS"],
                  "exit_code": int(os.environ["EXIT_CODE"]),
                  "failed_phase": os.environ["FAILED_PHASE"] or None,
                  "started_at": os.environ["STARTED_AT"],
                  "finished_at": os.environ["FINISHED_AT"],
                  "static_gates": [
                      {"name": name, "command": command, "status": status}
                      for name, command, status in gates
                  ],
                  "dogfood": {
                      "selected_workload": dogfood,
                      "status": os.environ["DOGFOOD_STATUS"],
                      "output": dogfood_output,
                      "summary": dogfood_summary,
                      "expectation": dogfood_expectation,
                      "expectation_status": dogfood_expectation_status,
                      "evidence_curation": "explicit-follow-up",
                  },
                  "scope": "bounded committed replay/evidence readiness; not universal determinism or hosted-product parity",
              }
              tmp = out.with_suffix(out.suffix + ".tmp")
              tmp.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
              tmp.replace(out)
              PY
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
                if dogfood_summary_json="$(python ${./scripts/summarize-accepted-dogfood-output.py} --json "$dogfood_output")"; then
                  python ${./scripts/summarize-accepted-dogfood-output.py} "$dogfood_output"
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
              run_gate assertion-readiness-promotion assertion_promotion_status check-assertion-readiness-promotion-gate .
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
              pkgs.coreutils
              pkgs.python3
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
              mkdir -p "$(dirname "$dest")"
              cp -R ${./docs/templates/rust-workload} "$dest"
              chmod -R u+w "$dest"
              python - "$dest" "$workload" <<'PY'
              import json, pathlib, sys
              dest = pathlib.Path(sys.argv[1])
              workload = sys.argv[2]
              package = workload.replace('_', '-').lower() + "-chaos-workload"
              replacements = {
                  "my-service-chaos-workload": package,
                  "my-service": workload,
              }
              for path in dest.rglob("*"):
                  if path.is_file() and path.suffix in {".md", ".rs", ".toml"}:
                      text = path.read_text()
                      for old, new in replacements.items():
                          text = text.replace(old, new)
                      path.write_text(text)
              manifest = {
                  "schema": "chaoscontrol.rust_workload_scaffold.v1",
                  "workload": workload,
                  "template_source": "docs/templates/rust-workload",
                  "local_dry_run": f"CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/{workload}.sdk.jsonl cargo run --bin {package}",
                  "local_report": f"summarize-sdk-local-output --input /tmp/{workload}.sdk.jsonl --output /tmp/{workload}.local-report.json",
                  "quality_gate": f"check-sdk-assertion-quality --input /tmp/{workload}.local-report.json",
                  "bounded_vm_campaign": "nix run github:your-org/chaoscontrol#explore-rust-workload -- /tmp/cc-rust-workload-vm",
                  "promotion_boundary": "local assertion quality is not snapshot-backed replay proof; require accepted replay verdict artifacts before support promotion",
              }
              (dest / "chaoscontrol-scaffold.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
              PY
              echo "scaffolded Rust workload at: $dest"
              echo "manifest: $dest/chaoscontrol-scaffold.json"
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
              replayReadiness
              replayReadinessSchedulerReceipt
              pkgs.python3
            ];
            text = ''
              exec python ${./scripts/local-multi-hypervisor-kvm-smoke.py} \
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

            inherit
              guest-sdk
              guest-raft
              guest-net
              guest-redb
              guest-rust-workload
              ;
            inherit
              initrd-sdk
              initrd-raft
              initrd-net
              initrd-redb
              initrd-rust-workload
              ;
            inherit redb-disk-image;

            raft-accepted-verdict-dogfood = acceptedVerdictDogfood.raft;
            redb-accepted-verdict-dogfood = acceptedVerdictDogfood.redb;
            net-accepted-verdict-dogfood = acceptedVerdictDogfood.net;
            rust-workload-accepted-verdict-dogfood = acceptedVerdictDogfood.rust-workload;
            accepted-verdict-dogfood-config = acceptedVerdictDogfoodConfig;
            replay-readiness = replayReadiness;
            scaffold-rust-workload = scaffoldRustWorkload;
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
                    pkgs.cargo-audit
                    pkgs.python3
                  ];
                }
                ''
                  cargo-audit audit --no-fetch --stale --db ${advisory-db} --file ${self}/Cargo.lock --json > "$TMPDIR/audit.json"
                  python ${self}/scripts/check-cargo-audit-report.py \
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
                  check-evidence-contracts --root .
                  check-replay-proof-coverage .
                  check-replay-proof-coverage --check-doc .
                  materialize-snapshot-chunks --selftest
                  check-readiness-promotion-gate --root .
                  check-readiness-surface-drift .
                  replay-readiness-triage --root . --sample-receipt --check docs/operator-triage-runbook.md
                  generate-replay-readiness-report --check .
                  generate-assertion-readiness-report --check .
                  check-assertion-readiness-promotion-gate .
                  check-sdk-assertion-quality
                  check-dogfood-artifact-sizes
                  check-accepted-dogfood-config --config ${acceptedVerdictDogfoodConfig} --expectations ${./dogfood-results/accepted-dogfood-expectations.json}
                  touch $out
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
                  cat > "$out/scheduler-execution-plan.json" <<EOF
                  {
                    "schema_version": 1,
                    "command": "replay-readiness-scheduler-receipt",
                    "status": "planned",
                    "generated_at": "2026-05-11T00:00:00Z",
                    "scope": "bounded local replay run manifest; not a hosted service, not a fleet-scale scheduler, not a shared queue, and not product-parity evidence",
                    "raw_log_scraping": false,
                    "source_decision_receipt": "$out/decision-receipt.json",
                    "schedule": { "mode": "manual-batch", "max_runs": 2, "concurrency": 1 },
                    "run_plan": [
                      {
                        "run_id": "local-run-static-0001",
                        "workload": "static-readiness",
                        "command": "replay-readiness --receipt '$out/scheduled-run-1.json'",
                        "receipt_path": "$out/scheduled-run-1.json",
                        "decision_policy": "record-local-decision"
                      },
                      {
                        "run_id": "local-run-static-0002",
                        "workload": "static-readiness",
                        "command": "replay-readiness --receipt '$out/scheduled-run-2.json'",
                        "receipt_path": "$out/scheduled-run-2.json",
                        "decision_policy": "record-local-decision"
                      }
                    ],
                    "anti_claims": [
                      "This is not a hosted service.",
                      "This is not a fleet-scale scheduler and not a shared queue.",
                      "This scheduler receipt uses no raw-log scraping and does not prove product parity."
                    ]
                  }
                  EOF
                  replay-readiness-scheduler-receipt --run-plan "$out/scheduler-execution-plan.json" --output "$out/scheduler-execution-receipt.json" > "$out/scheduler-execution-summary.txt"
                  replay-readiness-scheduler-receipt --check-execution "$out/scheduler-execution-receipt.json" >> "$out/scheduler-execution-summary.txt"
                  cat > "$out/fleet-scheduler-plan.json" <<EOF
                  {
                    "schema_version": 1,
                    "queue": {
                      "queue_id": "fleet-queue-local-check",
                      "lease_timeout_seconds": 900,
                      "max_concurrency": 2,
                      "state_path": "$out/fleet-scheduler-state.json",
                      "entries": [
                        {"queue_entry_id": "queue-static-0001", "run_id": "fleet-run-static-0001", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/fleet-scheduled-run-1.json'", "receipt_path": "$out/fleet-scheduled-run-1.json"},
                        {"queue_entry_id": "queue-static-0002", "run_id": "fleet-run-static-0002", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/fleet-scheduled-run-2.json'", "receipt_path": "$out/fleet-scheduled-run-2.json"}
                      ]
                    },
                    "workers": [{"worker_id": "worker-a"}, {"worker_id": "worker-b"}],
                    "operator_decisions": ["$out/decision-receipt.json"]
                  }
                  EOF
                  replay-readiness-scheduler-receipt --run-fleet-plan "$out/fleet-scheduler-plan.json" --output "$out/fleet-scheduler-receipt.json" > "$out/fleet-scheduler-summary.txt"
                  replay-readiness-scheduler-receipt --check-fleet "$out/fleet-scheduler-receipt.json" >> "$out/fleet-scheduler-summary.txt"
                  cat > "$out/local-multi-hypervisor-campaign-plan.json" <<EOF
                  {
                    "schema_version": 1,
                    "campaign_id": "local-multi-hypervisor-check",
                    "max_hypervisors": 2,
                    "state_path": "$out/local-multi-hypervisor-campaign-state.json",
                    "artifact_index_path": "$out/local-multi-hypervisor-artifact-index.json",
                    "follow_up_policy": {"enabled": false, "reproduce": false, "minimize": false},
                    "hypervisors": [
                      {"hypervisor_worker_id": "local-hv-a", "node_id": "local-node-a", "resource_budget": {"vcpus": 2, "memory_mib": 1024}, "artifact_root": "$out/local-hv-a"},
                      {"hypervisor_worker_id": "local-hv-b", "node_id": "local-node-b", "resource_budget": {"vcpus": 2, "memory_mib": 1024}, "artifact_root": "$out/local-hv-b"}
                    ],
                    "queue": {
                      "entries": [
                        {"queue_entry_id": "mhq-static-0001", "run_id": "mh-run-static-0001", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/local-multi-hypervisor-run-1.json'", "receipt_path": "$out/local-multi-hypervisor-run-1.json"},
                        {"queue_entry_id": "mhq-static-0002", "run_id": "mh-run-static-0002", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/local-multi-hypervisor-run-2.json'", "receipt_path": "$out/local-multi-hypervisor-run-2.json"}
                      ]
                    },
                    "operator_decisions": ["$out/decision-receipt.json"]
                  }
                  EOF
                  replay-readiness-scheduler-receipt --run-multi-hypervisor-plan "$out/local-multi-hypervisor-campaign-plan.json" --output "$out/local-multi-hypervisor-campaign-receipt.json" > "$out/local-multi-hypervisor-campaign-summary.txt"
                  replay-readiness-scheduler-receipt --check-multi-hypervisor "$out/local-multi-hypervisor-campaign-receipt.json" >> "$out/local-multi-hypervisor-campaign-summary.txt"
                  replay-readiness-scheduler-receipt --render-multi-hypervisor-dashboard "$out/local-multi-hypervisor-campaign-receipt.json" --output "$out/local-multi-hypervisor-dashboard.html" >> "$out/local-multi-hypervisor-campaign-summary.txt"
                  cat > "$out/hosted-shared-state-plan.json" <<EOF
                  {
                    "schema_version": 1,
                    "machines": [
                      {"machine_id": "machine-a", "writer_id": "writer-machine-a"},
                      {"machine_id": "machine-b", "writer_id": "writer-machine-b"}
                    ],
                    "hypervisor_workers": [
                      {"hypervisor_worker_id": "hv-a", "machine_id": "machine-a"},
                      {"hypervisor_worker_id": "hv-b", "machine_id": "machine-b"}
                    ],
                    "queue": {
                      "queue_id": "hosted-shared-state-check",
                      "state_path": "$out/hosted-shared-queue-state.json",
                      "entries": [
                        {"queue_entry_id": "hosted-static-0001", "run_id": "hosted-run-static-0001", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/hosted-run-1.json'", "receipt_path": "$out/hosted-run-1.json", "decision_action": "reproduce"},
                        {"queue_entry_id": "hosted-static-0002", "run_id": "hosted-run-static-0002", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/hosted-run-2.json'", "receipt_path": "$out/hosted-run-2.json", "decision_action": "triage"}
                      ]
                    },
                    "decision_store": {"store_id": "hosted-shared-decision-store-check", "path": "$out/hosted-shared-decision-store.json"}
                  }
                  EOF
                  replay-readiness-scheduler-receipt --run-hosted-shared-state-plan "$out/hosted-shared-state-plan.json" --output "$out/hosted-shared-state-receipt.json" > "$out/hosted-shared-state-summary.txt"
                  replay-readiness-scheduler-receipt --check-hosted-shared-state "$out/hosted-shared-state-receipt.json" >> "$out/hosted-shared-state-summary.txt"
                  cat > "$out/networked-hosted-scheduler-plan.json" <<EOF
                  {
                    "schema_version": 1,
                    "harness_id": "networked-hosted-check",
                    "transport": "loopback-tcp",
                    "machines": [
                      {"machine_id": "machine-a", "writer_id": "writer-machine-a"},
                      {"machine_id": "machine-b", "writer_id": "writer-machine-b"}
                    ],
                    "worker_sessions": [
                      {"worker_session_id": "session-a", "hypervisor_worker_id": "hv-a", "machine_id": "machine-a", "started_by": "independent-process", "heartbeat_revision": 1, "last_heartbeat": "unix:1000"},
                      {"worker_session_id": "session-b", "hypervisor_worker_id": "hv-b", "machine_id": "machine-b", "started_by": "independent-process", "heartbeat_revision": 1, "last_heartbeat": "unix:1001"}
                    ],
                    "queue": {
                      "queue_id": "networked-hosted-check",
                      "adapter": "shared-loopback-file",
                      "state_snapshot_path": "$out/networked-hosted-queue-state.json",
                      "entries": [
                        {"queue_entry_id": "networked-static-0001", "run_id": "networked-run-static-0001", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/networked-run-1.json'", "receipt_path": "$out/networked-run-1.json", "decision_action": "reproduce"},
                        {"queue_entry_id": "networked-static-0002", "run_id": "networked-run-static-0002", "workload": "static-readiness", "command": "replay-readiness --receipt '$out/networked-run-2.json'", "receipt_path": "$out/networked-run-2.json", "decision_action": "triage"}
                      ]
                    },
                    "decision_store": {"store_id": "networked-hosted-decision-store-check", "adapter": "shared-loopback-file", "state_snapshot_path": "$out/networked-hosted-decision-store.json"}
                  }
                  EOF
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
                  nativeBuildInputs = [ pkgs.python3 ];
                }
                ''
                  mkdir -p "$out"
                  python - <<'PY' > "$out/summary.txt"
                  import json
                  from pathlib import Path

                  receipt_path = Path("${./dogfood-results/vm-determinism-drift-latest/receipt.json}")
                  receipt = json.loads(receipt_path.read_text())
                  expected_cases = {
                      "single-vm-1vcpu",
                      "single-vm-2vcpu",
                      "controller-3vm-1vcpu",
                      "controller-3vm-2vcpu",
                  }

                  def require(condition, message):
                      if not condition:
                          raise SystemExit(message)

                  require(receipt.get("schema_version") == 1, "schema_version must be 1")
                  require(receipt.get("gate") == "vm-determinism-drift", "unexpected gate")
                  require(str(receipt.get("kernel_crc32", "")).startswith("crc32:"), "missing kernel_crc32")
                  require(str(receipt.get("initrd_crc32", "")).startswith("crc32:"), "missing initrd_crc32")

                  cases = receipt.get("cases")
                  require(isinstance(cases, list) and cases, "cases must be a non-empty list")
                  seen = {case.get("name") for case in cases}
                  require(seen == expected_cases, f"unexpected cases: {sorted(seen)}")

                  for case in cases:
                      name = case.get("name")
                      require(case.get("runs") == 5, f"{name}: expected 5 runs")
                      require(case.get("passed") is True, f"{name}: not passed")
                      require(case.get("mismatches") == [], f"{name}: mismatches present")
                      require(case.get("dlog_structural_match") is True, f"{name}: dlog structural mismatch")
                      require(case.get("dlog_mismatches") == [], f"{name}: dlog mismatches present")
                      require(case.get("dlog_divergences") == [], f"{name}: dlog divergences present")
                      observations = case.get("observations")
                      require(isinstance(observations, list), f"{name}: observations must be a list")
                      require(len(observations) == case.get("runs"), f"{name}: observation count != runs")

                  print("vm-determinism-drift receipt: pass")
                  for case in cases:
                      print(f"{case['name']}: {case['runs']} runs, mismatches=0, dlog_structural_match=true")
                  PY
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
                    pkgs.gnugrep
                    pkgs.python3
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
            snapshot-demo = mkApp "Run the local ChaosControl snapshot demo." "${chaoscontrol}/bin/snapshot_demo";
            explore = mkApp "Run the ChaosControl explorer with caller-supplied arguments." "${chaoscontrol}/bin/chaoscontrol-explore";
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

              # eBPF tracing harness
              pkgs.clang
              pkgs.libbpf
              pkgs.bpftools
              pkgs.elfutils
              pkgs.zlib
              pkgs.pkg-config

              # Guest binary (musl static linking)
              pkgs.pkgsCross.musl64.stdenv.cc

              # OpenSpec
              pkgs.nodejs_22

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
