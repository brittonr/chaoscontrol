# Keep the Cargo provider explicit. The repository lock pins its upstream source.
{ pkgs }:
let
  reviewedVersion = "1.92.0";
  regressionPatch = builtins.path {
    path = ./patches/cargo-pathless-tests.patch;
    name = "cargo-pathless-tests.patch";
  };
  repairPatch = builtins.path {
    path = ./patches/cargo-pathless-source.patch;
    name = "cargo-pathless-source.patch";
  };
  schemaChecks = ''
    runHook preCheck
    cargo test --offline --locked --manifest-path "$buildAndTestSubdir/Cargo.toml" -p cargo-util-schemas --lib
    runHook postCheck
  '';
  cargo =
    assert pkgs.cargo.version == reviewedVersion;
    pkgs.cargo.overrideAttrs (previous: {
      pname = "cargo-radicle";
      patches = (previous.patches or [ ]) ++ [
        regressionPatch
        repairPatch
      ];
      doCheck = true;
      checkPhase = schemaChecks;
      meta = previous.meta // {
        license = previous.meta.license ++ [ pkgs.lib.licenses.agpl3Plus ];
      };
    });
  counterexample = pkgs.cargo.overrideAttrs (previous: {
    pname = "cargo-radicle-counterexample";
    patches = (previous.patches or [ ]) ++ [ regressionPatch ];
    cargoBuildFlags = [
      "-p"
      "cargo-util-schemas"
    ];
    doCheck = true;
    checkPhase = schemaChecks;
    doInstallCheck = false;
    installPhase = ''
      mkdir -p "$out"
    '';
  });
in
{
  inherit cargo counterexample;

  # Preserve Crane's artifact selection. Only its metadata subprocess changes.
  installHook =
    hook:
    hook.overrideAttrs (previous: {
      buildCommand = previous.buildCommand + ''
        substituteInPlace "$out/nix-support/setup-hook" \
          --replace-fail 'command cargo metadata' \
          'command ${cargo}/bin/cargo metadata'
      '';
    });
}
