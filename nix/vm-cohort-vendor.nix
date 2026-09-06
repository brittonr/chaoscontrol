# Preserve workspace-relative resources without changing the pinned Rust source.
{
  pkgs,
  source,
  revision,
}:
assert pkgs.lib.assertMsg (source.rev == revision) "VM Cohort vendor revision drifted";
let
  package = "vm-cohort-conformance";
  # The reviewed revision publishes this exact package version.
  version = "0.1.0";
  crate = "${package}-${version}";
  cargoSource = "git+rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3?rev=${revision}#${revision}";
  packageNames = [
    "vm-cohort-core"
    "vm-cohort-kvm"
    package
  ];
  profile = "${source}/config/generated/profile.json";
  standard = "${source}/crates/${package}/src/standard.rs";

  script = pkgs.writeShellScript "vm-cohort-vendor-layout" ''
    set -euo pipefail
    : "''${checkout:?VM Cohort checkout is required}"
    : "''${out:?VM Cohort output is required}"

    reject() { echo "$1" >&2; exit 1; }
    test -f '${profile}' || reject 'VM Cohort pinned profile is missing'
    test -d "$checkout/${crate}" || reject 'VM Cohort conformance package is missing'
    test ! -L "$checkout/${crate}" || reject 'VM Cohort conformance package is a symlink'
    cmp '${standard}' "$checkout/${crate}/src/standard.rs" || reject 'VM Cohort source bytes changed'
    test ! -e "$checkout/workspace" && test ! -L "$checkout/workspace" \
      || reject 'VM Cohort reserved workspace path exists'
    test ! -e "$out" && test ! -L "$out" || reject 'VM Cohort output already exists'

    cp -R --no-preserve=mode "$checkout/." "$out"
    mkdir -p "$out/workspace/crates" "$out/workspace/config/generated"
    mv "$out/${crate}" "$out/workspace/crates/${package}"
    ln -s 'workspace/crates/${package}' "$out/${crate}"
    cp '${profile}' "$out/workspace/config/generated/profile.json"

    cmp '${profile}' "$out/${crate}/src/../../../config/generated/profile.json"
    ${pkgs.diffutils}/bin/diff -r "$checkout/${crate}" "$out/${crate}"
    cd "$out"
    ${pkgs.b3sum}/bin/b3sum \
      'workspace/config/generated/profile.json' \
      'workspace/crates/${package}/src/standard.rs' > source-identities.b3
    printf '%s\n' '${revision}' > upstream-revision
    echo 'VM Cohort vendor layout preserves source and profile bytes'
  '';

  project = checkout: pkgs.runCommand "vm-cohort-vendor-workspace" { inherit checkout; } "${script}";
  overrideCheckout =
    packages: checkout:
    if builtins.any (p: pkgs.lib.hasPrefix "vm-cohort-" p.name) packages then
      assert pkgs.lib.assertMsg (builtins.all (
        p: p.source == cargoSource && p.version == version && builtins.elem p.name packageNames
      ) packages) "VM Cohort vendor package identity drifted";
      project checkout
    else
      checkout;
in
{
  inherit
    crate
    overrideCheckout
    project
    script
    ;
}
