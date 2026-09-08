# Client repair for development acquisition. The compiler remains unchanged.
{ pkgs, rustToolchain }:
let
  version = "1.98.0";
  src = pkgs.fetchurl {
    url = "https://static.rust-lang.org/dist/rustc-${version}-src.tar.gz";
    # Nix fixed-output fetches use SHA-256 SRI, not stack content identities.
    hash = "sha256-siau83X/vp++K4X96Za1BxbVnVUmjiQNBSOWU0t16Sk=";
  };
  sourceToolchain = rustToolchain // {
    unwrapped = {
      inherit version src;
      tests = { };
    };
  };
  cargo = pkgs.cargo.override {
    rustc = sourceToolchain;
    rustPlatform = pkgs.makeRustPlatform {
      cargo = rustToolchain;
      rustc = rustToolchain;
    };
  };
in
assert rustToolchain.version == version;
cargo.overrideAttrs (old: {
  pname = "cargo-exact-revisions";
  patches = (old.patches or [ ]) ++ [
    ./cargo-exact-revisions.patch
    ./cargo-pathless-package-ids.patch
  ];
  env = builtins.removeAttrs old.env [ "RUSTC_BOOTSTRAP" ];
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    cargo test --manifest-path src/tools/cargo/Cargo.toml --locked --offline -p cargo-util-schemas --lib core::package_id_spec
    cargo test --manifest-path src/tools/cargo/Cargo.toml --locked --offline -p cargo --lib sources::git
    runHook postCheck
  '';
})
