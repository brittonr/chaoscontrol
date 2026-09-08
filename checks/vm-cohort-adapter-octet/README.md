# VM Cohort adapter Octet scope

This focused workspace replaces private Radicle package IDs with exact Nix-provided path sources before strict Octet compilation. It checks the ChaosControl adapter and compiles its exact dependencies. It does not weaken the product Cargo or Nix pin and does not prove consumer parity or release eligibility.

## Lockfile regeneration

The `vm-cohort-octet-workspace` package exposes this exact assembled workspace. Use it when dependency manifests change. Do not generate this lockfile from an ambient sibling checkout.

```console
workspace=$(nix build path:$PWD#vm-cohort-octet-workspace --no-link --print-out-paths)
scratch=$(mktemp -d)
cp -R "$workspace/." "$scratch/"
chmod -R u+w "$scratch"
nix develop -c cargo metadata --offline --format-version 1 --manifest-path "$scratch/Cargo.toml" > "$scratch/metadata.log"
diff -u checks/vm-cohort-adapter-octet/Cargo.lock "$scratch/Cargo.lock"
```

Review the diff before you copy the generated lockfile. A diff exit status of one means that the files differ. Offline metadata requires the normal registry cache.

```console
cp "$scratch/Cargo.lock" checks/vm-cohort-adapter-octet/Cargo.lock
nix build path:$PWD#checks.x86_64-linux.vm-cohort-adapter-octet-deny-all --no-link -L
```

The Octet lockfile mutation guard remains enabled. A successful dependency build does not replace this strict check.
