{
  pkgs,
  source,
  revision,
  rustToolchain,
}:
let
  makeLayout = import ../vm-cohort-vendor.nix;
  layout = makeLayout { inherit pkgs source revision; };
  identity = {
    name = "vm-cohort-conformance";
    version = "0.1.0";
    source = "git+rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3?rev=${revision}#${revision}";
  };
  fixture = pkgs.runCommand "vm-cohort-vendor-fixture" { } ''
    mkdir -p "$out/${layout.crate}/src" "$out/other-crate"
    cp ${source}/crates/vm-cohort-conformance/src/standard.rs "$out/${layout.crate}/src/standard.rs"
    cp ${../fixtures/vendor/probe.rs} "$out/${layout.crate}/src/probe.rs"
    printf '%s\n' 'unrelated package bytes' > "$out/other-crate/sentinel"
  '';
  projected = layout.overrideCheckout [ identity ] fixture;
  missingSource = {
    rev = revision;
    outPath = pkgs.runCommand "vm-cohort-missing-profile" { } ''mkdir -p "$out"'';
  };
  missing = makeLayout {
    inherit pkgs revision;
    source = missingSource;
  };
  rejects =
    packages: !(builtins.tryEval (toString (layout.overrideCheckout packages fixture))).success;
  revisionDrift = builtins.tryEval (
    toString
      (makeLayout {
        inherit pkgs revision;
        source = source // {
          rev = "wrong-revision";
        };
      }).script
  );
in
assert rejects [ (identity // { version = "wrong-version"; }) ];
assert rejects [ (identity // { source = "wrong-source"; }) ];
assert rejects [ (identity // { name = "vm-cohort-unknown"; }) ];
assert !revisionDrift.success;
assert layout.overrideCheckout [ { name = "unrelated"; } ] "unchanged" == "unchanged";
pkgs.runCommand "vm-cohort-vendor-tests"
  {
    nativeBuildInputs = [ rustToolchain ];
    VM_COHORT_PROFILE = "${source}/config/generated/profile.json";
  }
  ''
    set -euo pipefail
    mkdir -p "$out" "$TMPDIR/vendor/source"

    # The same compiler rejects the original flat layout.
    if rustc ${fixture}/${layout.crate}/src/probe.rs -o "$TMPDIR/baseline" > "$out/baseline.log" 2>&1; then
      echo 'the flat vendor fixture unexpectedly compiled' >&2
      exit 1
    fi
    grep -F 'config/generated/profile.json' "$out/baseline.log"
    grep -F 'No such file or directory' "$out/baseline.log"

    # Match Crane's source-group link and its selected-package link.
    ln -s ${projected}/${layout.crate} "$TMPDIR/vendor/source/${layout.crate}"
    rustc "$TMPDIR/vendor/source/${layout.crate}/src/probe.rs" -o "$TMPDIR/probe"
    "$TMPDIR/probe"
    cmp ${fixture}/other-crate/sentinel ${projected}/other-crate/sentinel
    ${pkgs.diffutils}/bin/diff -r ${fixture}/${layout.crate} ${projected}/${layout.crate}
    cmp ${source}/config/generated/profile.json ${projected}/workspace/config/generated/profile.json

    reject_projection() {
      local input="$1"
      shift
      local expected="$1"
      if checkout="$input" out="$TMPDIR/rejected-output" ${layout.script} > "$out/$expected.log" 2>&1; then
        echo "the projection accepted $expected" >&2
        exit 1
      fi
      grep -F "$expected" "$out/$expected.log"
      test ! -e "$TMPDIR/rejected-output"
    }

    cp -R --no-preserve=mode ${fixture} "$TMPDIR/changed"
    printf '\n' >> "$TMPDIR/changed/${layout.crate}/src/standard.rs"
    reject_projection "$TMPDIR/changed" 'VM Cohort source bytes changed'

    cp -R --no-preserve=mode ${fixture} "$TMPDIR/reserved"
    mkdir "$TMPDIR/reserved/workspace"
    reject_projection "$TMPDIR/reserved" 'VM Cohort reserved workspace path exists'

    cp -R --no-preserve=mode ${fixture} "$TMPDIR/dangling"
    ln -s missing "$TMPDIR/dangling/workspace"
    reject_projection "$TMPDIR/dangling" 'VM Cohort reserved workspace path exists'

    mkdir "$TMPDIR/missing-package"
    reject_projection "$TMPDIR/missing-package" 'VM Cohort conformance package is missing'

    mkdir "$TMPDIR/package-link"
    ln -s ${fixture}/${layout.crate} "$TMPDIR/package-link/${layout.crate}"
    reject_projection "$TMPDIR/package-link" 'VM Cohort conformance package is a symlink'

    if checkout=${fixture} out="$TMPDIR/missing-output" ${missing.script} > "$out/missing-profile.log" 2>&1; then
      echo 'the projection accepted a missing profile' >&2
      exit 1
    fi
    grep -F 'VM Cohort pinned profile is missing' "$out/missing-profile.log"
    test ! -e "$TMPDIR/missing-output"

    mkdir "$TMPDIR/existing-output"
    printf '%s\n' 'preserved output' > "$TMPDIR/existing-output/sentinel"
    if checkout=${fixture} out="$TMPDIR/existing-output" ${layout.script} > "$out/existing-output.log" 2>&1; then
      echo 'the projection replaced an existing output' >&2
      exit 1
    fi
    grep -F 'VM Cohort output already exists' "$out/existing-output.log"
    test "$(< "$TMPDIR/existing-output/sentinel")" = 'preserved output'

    cp ${projected}/source-identities.b3 "$out/source-identities.b3"
    printf '%s\n' 'vendor layout: positive compiler parity and negative controls passed' > "$out/result"
  ''
