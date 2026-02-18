{
  description = "ChaosControl - Deterministic VMM";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rust-analyzer
            pkg-config
            rustfmt
            clippy
          ];

          shellHook = ''
            echo "ChaosControl development environment"
            echo "Rust version: $(rustc --version)"
          '';
        };
      }
    );
}
