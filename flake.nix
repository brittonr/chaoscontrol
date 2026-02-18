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

  outputs = { self, nixpkgs, crane, rust-overlay }:
    let
      supportedSystems = [ "x86_64-linux" ]; # KVM is Linux-only
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # Filter source to only include Rust-relevant files
          src = craneLib.cleanCargoSource ./.;

          # Common build arguments shared between deps and final build
          commonArgs = {
            inherit src;
            strictDeps = true;
            pname = "chaoscontrol";
            version = "0.1.0";

            # libbpf-sys (via chaoscontrol-trace) needs pkg-config + system libs
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.llvmPackages.clang-unwrapped  # BPF compilation
            ];
            buildInputs = [
              pkgs.elfutils   # libelf
              pkgs.zlib       # zlib
              pkgs.libbpf     # libbpf
            ];

            # libbpf-cargo needs unwrapped clang for BPF target
            CLANG = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
          };

          # Build only the cargo dependencies — cached across rebuilds
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          # Build the full workspace
          chaoscontrol = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
        in
        {
          default = chaoscontrol;
          chaoscontrol-vmm = chaoscontrol;
        }
      );

      # nix run .#boot -- <kernel> [initrd]
      # nix run .#snapshot-demo -- <kernel> <initrd>
      apps = forAllSystems (system:
        let
          pkg = self.packages.${system}.default;
        in
        {
          default = {
            type = "app";
            program = "${pkg}/bin/boot";
          };
          boot = {
            type = "app";
            program = "${pkg}/bin/boot";
          };
          snapshot-demo = {
            type = "app";
            program = "${pkg}/bin/snapshot_demo";
          };
        }
      );

      # nix flake check
      checks = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          src = craneLib.cleanCargoSource ./.;
          commonArgs = {
            inherit src;
            strictDeps = true;
            pname = "chaoscontrol";
            version = "0.1.0";
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          # Build
          package = self.packages.${system}.default;

          # Clippy
          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          # Formatting
          fmt = craneLib.cargoFmt {
            inherit src;
            pname = "chaoscontrol";
            version = "0.1.0";
          };

          # Tests (note: KVM tests need /dev/kvm, which isn't
          # available in the Nix sandbox — those are filtered out)
          tests = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        }
      );

      # nix develop
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" ];
            targets = [ "x86_64-unknown-linux-musl" ];
          };
        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.cargo-watch
              pkgs.cargo-edit

              # eBPF tracing harness dependencies
              pkgs.clang              # BPF program compilation
              pkgs.libbpf             # BPF library (headers + lib)
              pkgs.bpftools           # bpftool (vmlinux.h generation)
              pkgs.elfutils           # libelf (libbpf-sys dependency)
              pkgs.zlib               # libbpf-sys dependency
              pkgs.pkg-config         # find system libs

              # Guest binary (musl static linking)
              pkgs.pkgsCross.musl64.stdenv.cc   # x86_64-unknown-linux-musl-gcc
            ];

            # libbpf-sys needs to find libelf and zlib
            nativeBuildInputs = [
              pkgs.pkg-config
            ];

            # BPF compilation needs unwrapped clang (nix wrapper adds
            # flags like -fzero-call-used-regs that the BPF target
            # doesn't support). libbpf-cargo reads $CLANG.
            CLANG = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";

            shellHook = ''
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
              echo "Tracing:"
              echo "  cargo build -p chaoscontrol-trace    Build trace harness"
              echo "  sudo chaoscontrol-trace live --pid <PID>"
              echo "  chaoscontrol-trace verify --trace-a a.json --trace-b b.json"
            '';
          };
        }
      );
    };
}
