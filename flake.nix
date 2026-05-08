{
  description = "ChaosControl — Deterministic VMM for simulation testing";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    tigerstyle.url = "git+file:../tigerstyle?ref=refs/heads/main&rev=2197d80a2a4a261e141531927084e66f92935f93";
    verified-logic.url = "git+file:../verified-logic?ref=refs/heads/main&rev=b332e653e3252922eb66aac6912899272d7c6c07";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      rust-overlay,
      tigerstyle,
      verified-logic,
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
            in
            (craneLib.filterCargoSources path type)
            || isEvidenceFixture
            || isDogfoodCheckpointFixture
            || (builtins.match ".*\\.bpf\\.c$" path != null)
            || (builtins.match ".*\\.h$" path != null)
            || (builtins.match ".*\\.html$" path != null)
            || (builtins.match ".*\\.js$" path != null);

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = sourceFilter;
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

          replayReadiness = pkgs.writeShellApplication {
            name = "replay-readiness";
            runtimeInputs = [
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
                  ("contract-registry", "python scripts/check-contract-registry.py", os.environ["CONTRACT_REGISTRY_STATUS"]),
                  ("evidence-contracts", "python scripts/check-evidence-contracts.py", os.environ["EVIDENCE_CONTRACTS_STATUS"]),
                  ("replay-proof-coverage", "python scripts/check-replay-proof-coverage.py", os.environ["REPLAY_PROOF_COVERAGE_STATUS"]),
                  ("readiness-promotion", "python scripts/check-readiness-promotion-gate.py", os.environ["READINESS_PROMOTION_STATUS"]),
                  ("readiness-surface-drift", "python scripts/check-readiness-surface-drift.py", os.environ["READINESS_SURFACE_DRIFT_STATUS"]),
                  ("readiness-report", "python scripts/generate-replay-readiness-report.py --check", os.environ["READINESS_REPORT_STATUS"]),
                  ("assertion-readiness-report", "python scripts/generate-assertion-readiness-report.py --check", os.environ["ASSERTION_REPORT_STATUS"]),
                  ("assertion-readiness-promotion", "python scripts/check-assertion-readiness-promotion-gate.py", os.environ["ASSERTION_PROMOTION_STATUS"]),
                  ("sdk-local-report-tracks", "python scripts/check-sdk-local-report-tracks.py", os.environ["SDK_LOCAL_REPORT_TRACKS_STATUS"]),
                  ("dogfood-artifact-sizes", "python scripts/check-dogfood-artifact-sizes.py", os.environ["ARTIFACT_SIZES_STATUS"]),
                  ("accepted-dogfood-config", "python scripts/check-accepted-dogfood-config.py --config <nix-generated>", os.environ["ACCEPTED_DOGFOOD_CONFIG_STATUS"]),
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
              run_gate contract-registry contract_registry_status python scripts/check-contract-registry.py
              run_gate evidence-contracts evidence_contracts_status python scripts/check-evidence-contracts.py
              run_gate replay-proof-coverage replay_proof_coverage_status python scripts/check-replay-proof-coverage.py
              run_gate readiness-promotion readiness_promotion_status python scripts/check-readiness-promotion-gate.py
              run_gate readiness-surface-drift readiness_surface_drift_status python scripts/check-readiness-surface-drift.py
              run_gate readiness-report readiness_report_status python scripts/generate-replay-readiness-report.py --check
              run_gate assertion-readiness-report assertion_report_status python scripts/generate-assertion-readiness-report.py --check
              run_gate assertion-readiness-promotion assertion_promotion_status python scripts/check-assertion-readiness-promotion-gate.py
              run_gate sdk-local-report-tracks sdk_local_report_tracks_status python scripts/check-sdk-local-report-tracks.py
              run_gate dogfood-artifact-sizes artifact_sizes_status python scripts/check-dogfood-artifact-sizes.py
              run_gate accepted-dogfood-config accepted_dogfood_config_status python scripts/check-accepted-dogfood-config.py --config ${acceptedVerdictDogfoodConfig} --expectations ${./dogfood-results/accepted-dogfood-expectations.json}
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

          replayReadinessSummary = pkgs.writeShellApplication {
            name = "replay-readiness-summary";
            runtimeInputs = [ pkgs.python3 ];
            text = ''
              exec python ${self}/scripts/summarize-replay-readiness-receipt.py "$@"
            '';
          };

          replayReadinessDashboard = pkgs.writeShellApplication {
            name = "replay-readiness-dashboard";
            runtimeInputs = [ pkgs.python3 ];
            text = ''
              exec python ${self}/scripts/render-replay-readiness-dashboard.py "$@"
            '';
          };

          replayReadinessReadmeStatus = pkgs.writeShellApplication {
            name = "replay-readiness-readme-status";
            runtimeInputs = [ pkgs.python3 ];
            text = ''
              exec python ${self}/scripts/update-replay-readiness-readme-status.py "$@"
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
            replay-readiness-summary = replayReadinessSummary;
            replay-readiness-dashboard = replayReadinessDashboard;
            replay-readiness-readme-status = replayReadinessReadmeStatus;

            cargo-tigerstyle = tigerstyle.packages.${system}.cargo-tigerstyle;
            tigerstyle-standards = tigerstyle.packages.${system}.tigerstyle-standards;
            verified-logic = verified-logic.packages.${system}.verified-logic;

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
                  nativeBuildInputs = [ pkgs.python3 ];
                }
                ''
                  mkdir -p $out
                  export CHAOSCONTROL_SDK_LOCAL_OUTPUT=$out/sdk.jsonl
                  ${guest-rust-workload}/bin/chaoscontrol-rust-workload-guest
                  python ${./scripts/summarize-sdk-local-output.py} \
                    --input $out/sdk.jsonl \
                    --output $out/report.json \
                    --evidence-class instrumentation-dry-run
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
            nixfmt = pkgs.runCommand "nixfmt-check" { nativeBuildInputs = [ pkgs.nixfmt-rfc-style ]; } ''
              cd ${self}
              nixfmt --check .
              touch $out
            '';

            # Nickel-backed evidence contracts and committed dogfood receipt data.
            evidence-contracts =
              pkgs.runCommand "evidence-contracts-check"
                {
                  nativeBuildInputs = [
                    pkgs.nickel
                    pkgs.python3
                  ];
                }
                ''
                  cd ${self}
                  python scripts/check-contract-registry.py
                  python scripts/check-evidence-contracts.py
                  python scripts/check-replay-proof-coverage.py
                  python scripts/check-readiness-promotion-gate.py
                  python scripts/check-readiness-surface-drift.py
                  python scripts/generate-replay-readiness-report.py --check
                  python scripts/generate-assertion-readiness-report.py --check
                  python scripts/check-assertion-readiness-promotion-gate.py
                  python scripts/check-dogfood-artifact-sizes.py
                  python scripts/check-accepted-dogfood-config.py --config ${acceptedVerdictDogfoodConfig} --expectations ${./dogfood-results/accepted-dogfood-expectations.json}
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
                  ];
                }
                ''
                  mkdir -p "$out"
                  receipt="$out/replay-readiness-receipt.json"
                  replay-readiness --receipt "$receipt"
                  replay-readiness-summary "$receipt" | tee "$out/replay-readiness-summary.txt"
                  replay-readiness-dashboard "$receipt" --output "$out/replay-readiness-dashboard.html"
                  test -s "$receipt"
                  test -s "$out/replay-readiness-summary.txt"
                  test -s "$out/replay-readiness-dashboard.html"
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

            # Track the local sibling proof/style repos used by this workspace.
            tigerstyle-policy-registry = tigerstyle.checks.${system}.policy-registry;
            tigerstyle-chaoscontrol-focused = tigerstyle.lib.mkConsumerCheck {
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
                "chaoscontrol-guest"
                "chaoscontrol-raft-guest"
                "chaoscontrol-guest-net"
                "chaoscontrol-net-guest"
                "chaoscontrol-redb-guest"
                "chaoscontrol-rust-workload-guest"
              ];
              cargoExtraArgs = "--lib";
            };
            verified-logic-verus-proofs = verified-logic.checks.${system}.verus-proofs;

            # Simulation tests live in packages, not checks — they take
            # 10+ minutes and need /dev/kvm.  Run explicitly:
            #   nix build .#raft-sim
            #   nix build .#redb-sim
            #   nix run .#explore-raft
            #   nix run .#explore-redb
          };

          apps = {
            default = {
              type = "app";
              program = "${chaoscontrol}/bin/boot";
            };
            boot = {
              type = "app";
              program = "${chaoscontrol}/bin/boot";
            };
            snapshot-demo = {
              type = "app";
              program = "${chaoscontrol}/bin/snapshot_demo";
            };
            explore = {
              type = "app";
              program = "${chaoscontrol}/bin/chaoscontrol-explore";
            };
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
              {
                type = "app";
                program = "${wrapper}/bin/explore-raft";
              };
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
              {
                type = "app";
                program = "${wrapper}/bin/explore-redb";
              };
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
              {
                type = "app";
                program = "${wrapper}/bin/explore-sdk";
              };
            rust-workload-local-report =
              let
                wrapper = pkgs.writeShellApplication {
                  name = "rust-workload-local-report";
                  runtimeInputs = [
                    guest-rust-workload
                    pkgs.coreutils
                    pkgs.python3
                  ];
                  text = ''
                    out="''${1:-./chaoscontrol-rust-workload-local-report}"
                    mkdir -p "$out"
                    export CHAOSCONTROL_SDK_LOCAL_OUTPUT="$out/sdk.jsonl"
                    rm -f "$CHAOSCONTROL_SDK_LOCAL_OUTPUT"
                    chaoscontrol-rust-workload-guest
                    python ${./scripts/summarize-sdk-local-output.py} \
                      --input "$CHAOSCONTROL_SDK_LOCAL_OUTPUT" \
                      --output "$out/report.json" \
                      --evidence-class instrumentation-dry-run
                    printf 'local report: %s\n' "$out/report.json"
                  '';
                };
              in
              {
                type = "app";
                program = "${wrapper}/bin/rust-workload-local-report";
              };
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
              {
                type = "app";
                program = "${wrapper}/bin/explore-rust-workload";
              };
            raft-accepted-verdict-dogfood = {
              type = "app";
              program = "${acceptedVerdictDogfood.raft}/bin/raft-accepted-verdict-dogfood";
            };
            redb-accepted-verdict-dogfood = {
              type = "app";
              program = "${acceptedVerdictDogfood.redb}/bin/redb-accepted-verdict-dogfood";
            };
            net-accepted-verdict-dogfood = {
              type = "app";
              program = "${acceptedVerdictDogfood.net}/bin/net-accepted-verdict-dogfood";
            };
            rust-workload-accepted-verdict-dogfood = {
              type = "app";
              program = "${acceptedVerdictDogfood.rust-workload}/bin/rust-workload-accepted-verdict-dogfood";
            };
            replay-readiness = {
              type = "app";
              program = "${replayReadiness}/bin/replay-readiness";
            };
            replay-readiness-summary = {
              type = "app";
              program = "${replayReadinessSummary}/bin/replay-readiness-summary";
            };
            replay-readiness-dashboard = {
              type = "app";
              program = "${replayReadinessDashboard}/bin/replay-readiness-dashboard";
            };
            replay-readiness-readme-status = {
              type = "app";
              program = "${replayReadinessReadmeStatus}/bin/replay-readiness-readme-status";
            };
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
              tigerstyle.packages.${system}.cargo-tigerstyle
              tigerstyle.packages.${system}.tigerstyle-standards
              verified-logic.packages.${system}.verified-logic

              # Nix formatting (matches CI check)
              pkgs.nixfmt-rfc-style
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
