# Commands for the source checkpoint

These commands ran from the retained protocol-observation worktree.
The final product source is committed at `4343e37`.
The compiler, Nix, and replay checks used these product bytes before the commit. The repository and lifecycle checks followed the commit.
`source-checked-product-inputs.b3` measures selected product changes relative to `31300fa1a2d29c7496e8316f065c156f80343143`.
The command environment disables remote builders and automatic low-space collection for these invocations only.
Nix keeps its configured build directory. No global Nix configuration, permission, or security rule changes.

The command blocks use POSIX shell syntax.

## Compiler checks

```sh
BUILD_JOBS=4
export CARGO_TARGET_DIR="$PWD/target" CARGO_BUILD_JOBS="$BUILD_JOBS"
export NIX_CONFIG='builders =
min-free = 0'
set -- -p chaoscontrol-protocol -p chaoscontrol-sdk -p chaoscontrol-fault \
  -p chaoscontrol-sim-core -p chaoscontrol-vmm -p chaoscontrol-explore \
  -p chaoscontrol-evidence
nix develop -c cargo test "$@" --all-targets --all-features
nix develop -c cargo clippy "$@" --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' nix develop -c cargo doc "$@" --all-features --no-deps
nix develop -c cargo fmt "$@" --check
```

## Minimal feature matrix

```sh
nix develop -c cargo test -p chaoscontrol-protocol -p chaoscontrol-sdk \
  --all-targets --no-default-features
nix develop -c cargo clippy -p chaoscontrol-protocol -p chaoscontrol-sdk \
  --all-targets --no-default-features -- -D warnings
nix develop -c cargo check -p chaoscontrol-protocol -p chaoscontrol-sdk \
  --no-default-features
```

The following commands intentionally fail with the required-feature diagnostic.
They cannot produce successful minimal-mode results for full-only targets.

```sh
nix develop -c cargo test -p chaoscontrol-sdk --no-default-features --test multiprocess_shell
nix develop -c cargo test -p chaoscontrol-sdk --no-default-features --test stable_assertions
nix develop -c cargo check -p chaoscontrol-sdk --no-default-features --example rust_workload_harness
```

## Focused Nix checks and full retry

```sh
nix build \
  .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused \
  .#checks.x86_64-linux.protocol-observation-tests \
  .#checks.x86_64-linux.protocol-observation-contracts \
  .#checks.x86_64-linux.vm-cohort-vendor-tests \
  .#checks.x86_64-linux.vm-cohort-vendor-adapter \
  .#checks.x86_64-linux.dependency-policy \
  .#checks.x86_64-linux.source-filter \
  .#checks.x86_64-linux.vm-cohort-adapter-octet-deny-all \
  .#checks.x86_64-linux.license-boundary \
  --out-link /home/brittonr/.cache/cc-source-checked-20260906 -L
nix flake check -L
```

The focused build exits zero, but the broad source report remains warning-only.
The full flake check exits one at SpaceWasm manifest admission.
`source-checked-output-roots.txt` records the retained focused output links.

## Bounded replay

```sh
nix develop -c cargo test -p chaoscontrol-explore \
  --test protocol_observation replay:: --no-run
REPLAY_TIMEOUT_SECONDS=120
timeout "${REPLAY_TIMEOUT_SECONDS}s" nix develop -c cargo test \
  -p chaoscontrol-explore --test protocol_observation replay:: -- \
  --ignored --nocapture --test-threads=1
```

## Repository and lifecycle checks

```sh
nix develop -c cargo run -q -p chaoscontrol-evidence --bin check-product-scope -- --root .
nix develop -c cargo run -q -p chaoscontrol-evidence --bin check-contract-registry -- .
CAIRN=/tmp/chaoscontrol-protocol-cairn-tool/bin/cairn
POLICY=/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json
"$CAIRN" validate --root "$PWD" --policy "$POLICY"
for gate in proposal design tasks; do
  "$CAIRN" gate "$gate" add-protocol-observation-cohorts --root "$PWD" --policy "$POLICY"
done
```

Each command retains its output and status separately under `source-checked-*` or `minimal-*`.
The combined minimal matrix requires successful compatible checks and rejection of each incompatible target.
An exit status alone does not establish a clean Octet report, lifecycle completion, or release eligibility.
