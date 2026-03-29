{
  description = "ChaosControl — Deterministic VMM for simulation testing";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      rust-overlay,
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

          # Filter source to include Rust-relevant files + BPF sources
          # (cleanCargoSource strips .c/.h files needed by chaoscontrol-trace)
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              (craneLib.filterCargoSources path type)
              || (builtins.match ".*\\.bpf\\.c$" path != null)
              || (builtins.match ".*\\.h$" path != null);
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
          chaoscontrol = craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; });

          # Custom Linux kernel with built-in virtio for multi-VM networking
          netKernel = pkgs.linuxPackages_latest.kernel.override {
            structuredExtraConfig = with pkgs.lib.kernel; {
              VIRTIO = yes;
              VIRTIO_MMIO = yes;
              VIRTIO_NET = yes;
              VIRTIO_BLK = yes;
              PACKET = yes;
            };
          };

          # Custom Linux kernel with KCOV support for coverage-guided fuzzing
          kcovKernel = pkgs.linuxPackages_latest.kernel.override {
            structuredExtraConfig = with pkgs.lib.kernel; {
              KCOV = yes;
              KCOV_INSTRUMENT_ALL = yes;
              KCOV_ENABLE_COMPARISONS = yes;
              DEBUG_FS = yes;
              VIRTIO_NET = yes;
            };
          };
        in
        {
          inherit
            pkgs
            craneLib
            src
            commonArgs
            cargoArtifacts
            chaoscontrol
            ;

          packages = {
            default = chaoscontrol;
            chaoscontrol-vmm = chaoscontrol;

            kcov-kernel = kcovKernel;

            net-kernel = netKernel;
            net-vmlinux = pkgs.runCommand "net-vmlinux" { } ''
              mkdir -p $out
              ln -s ${netKernel.dev}/vmlinux $out/vmlinux
            '';

            kcov-vmlinux = pkgs.runCommand "kcov-vmlinux" { } ''
              mkdir -p $out
              ln -s ${kcovKernel.dev}/vmlinux $out/vmlinux
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
            tests = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

            # Nix formatting
            nixfmt = pkgs.runCommand "nixfmt-check" { nativeBuildInputs = [ pkgs.nixfmt-rfc-style ]; } ''
              cd ${self}
              nixfmt --check .
              touch $out
            '';
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
              echo "  cargo build              Build the project"
              echo "  cargo test               Run tests"
              echo "  cargo run --bin boot -- <kernel> [initrd]"
              echo "  cargo run --bin snapshot_demo -- <kernel> <initrd>"
              echo "  cargo watch -x check     Watch for changes"
              echo "  cargo clippy             Lint"
              echo ""
              echo "CI:"
              echo "  nix flake check          Run all checks (build, test, clippy, fmt, nixfmt)"
              echo ""
              echo "Tracing:"
              echo "  cargo build -p chaoscontrol-trace    Build trace harness"
              echo "  sudo chaoscontrol-trace live --pid <PID>"
              echo "  chaoscontrol-trace verify --trace-a a.json --trace-b b.json"
              echo ""
              echo "Kernels:"
              echo "  nix build .#kcov-kernel       KCOV kernel (coverage-guided fuzzing)"
              echo "  nix build .#kcov-vmlinux      KCOV vmlinux"
              echo "  nix build .#net-kernel        Virtio-net kernel (multi-VM networking)"
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
    };
}
