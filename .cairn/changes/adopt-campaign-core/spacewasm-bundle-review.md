# SpaceWasm bundle admission review

## Bounded review contract

Goal: explain the full-check bundle mismatch without accepting unreviewed bytes or weakening admission.

Evidence must bind the exact consumer profile, pinned Mantle source, produced manifest, and any repeated derivation check. A matching runner digest alone is insufficient. A refreshed expected digest alone is not a repair.

This review permits read-only source inspection and one bounded check-mode rebuild of the producer's upstream-test derivation. The rebuild budget is five minutes. It must preserve the existing store output and all retained receipts. No source pin, bundle digest, profile limit, or non-claim changes are authorized by this review itself.

## Approach record

- Stale admission: compare the profile revision and artifact identities with the pinned producer output. The source revision agrees. The manifest and bundle identities differ. The old complete bundle is not present in the inspected local store.
- Producer output instability: the pinned `nix/spacewasm-reference.nix` stores raw Cargo test stdout and stderr, hashes them into receipts, and copies them into the bundle. The retained stderr includes elapsed build time. A same-derivation check-mode rebuild can test reproducibility.
- Engine change: compare the produced runner with its independently pinned digest. The manifest reports the expected runner digest, but member bytes still require direct validation. This observation does not establish full bundle equivalence.

These are correlated source-review passes, not independent audits. The allowed result is a verified cause, an exact blocker, or budget exhaustion. Historical receipts remain unchanged.

## Checked result

The consumer remains pinned to Mantle `a141fcbaafe41f9a413a81275a33fe915bfca370` and SpaceWasm `e24cf09355a90497148eb5029fdb8e3400bd63e3`.

The full frozen check of consumer `6509820dee785ea288be1cbdb6e87b7a0c95194e` fails at `spacewasm-mvp-differential`:

- Expected manifest: `13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472`.
- Observed manifest: `ded66a4959c9efeda62f2eb3d13a06de6df0ad01a1d53f222c199ab6e66d9eb7`.
- Direct hashing confirms the runner still matches `be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221`.

One check-mode rebuild of `/nix/store/pqdh5xf8y0k76h7pjvaskxbq289crkwg-spacewasm-e24cf09355a90497148eb5029fdb8e3400bd63e3-unit-tests.drv` completed within the budget. Nix rejected it because the output differed from the retained original. The original store output was not replaced.

Both test runs report 231 passed, zero failed, and zero ignored. Their compilation order, test completion order, and elapsed times differ. The stderr build duration changed from 4.74 seconds to 3.76 seconds. The test duration changed from 0.00 seconds to 0.01 seconds. Both stdout and stderr hashes changed, followed by their receipt hashes.

The pinned producer copies these raw reports into its bundle and includes their identities in the manifest. This proves that repeated builds can change bundle identity without changing the selected runner. It does not prove that timing alone explains every difference from the unavailable historical bundle.

## Remaining repair boundary

Acceptance remains blocked on a producer-owned reproducibility repair. Mantle must define reproducible report content and preserve the required test facts and diagnostic evidence through an explicit contract. A published repair must pass repeated-build and negative controls before ChaosControl changes its admission profile.

Do not ignore the manifest mismatch, remove a required test, silently normalize consumer inputs, or replace the expected digest with the latest observed value. The consumer profile, limits, expected identities, and non-claims remain unchanged.

## Retained evidence

The operator retains these files under `campaign/.pi/complete-20260906/`:

- `scope-final-full-nix.log`
- `spacewasm-bundle-observed-hashes.txt`
- `spacewasm-test-derivation.txt`
- `spacewasm-test-rebuild.log` and `spacewasm-test-rebuild.exit`
- `spacewasm-test-output-baseline/`
- `spacewasm-test-output-rebuilt/`
- `spacewasm-test-output.diff`

The cause review is verified for the producer test-report derivation. Full ChaosControl acceptance and Campaign adoption remain incomplete.

## Resolution (2026-09-25)

### Producer repair

Mantle published the producer repair on `origin/main`:

- `10c9ca17deea9808978b9d6bbf89e2cd55780399` emits stable bundle evidence from both test producers.
- `57fa0f5c2f8fb1edeed6d6113b57d32357612e34` splits stable bundle evidence from run archives.

Each test producer writes a canonical `mantle-spacewasm-stable-report-v1` report. Its receipt binds only that report and its stable identity. Raw stdout, stderr, and run records moved to a separate run archive. The bundle carries only the stable reports and receipts, with role `check-receipt`. The Mantle manifest schema stays `mantle-spacewasm-reference-bundle-v1`. No later Mantle revision was needed.

### Consumer port

The consumer commits from local branch `drain/spacewasm-stable-evidence-consumer` were cherry-picked in order onto this branch with `-x`:

- `baeb89d` became `172b30a`.
- `79c1b415` became `bf627ba`. Its lock was regenerated rather than cherry-picked. The Mantle URL uses the full revision. `nix flake update mantle` rewrote `flake.lock`. Every lock node reachable from `mantle` is identical to the drain branch lock.
- `8be39bc` became `41b4916`.
- `7af3b43` became `3e77e33`.
- `66dd1e4` became `2a8cbfc`.
- `63b64678` became `88fb197`.

Commit `63762da` tightens admission in `crates/chaoscontrol-wasm-differential`. Every earlier denial remains. The new rules are:

- The required `--mantle-revision` argument must equal the profile's `mantle_revision`. The Nix check passes the locked `mantle.rev`.
- Both suites must carry a stable report and receipt at their exact `reports/checks/` paths with role `check-receipt`. The suites are `upstream-unit-tests` and `upstream-spectest-address`.
- Raw captures and run records are rejected. This covers the `stdout.txt`, `stderr.txt`, and `run-record.json` suffixes, the `runs/` prefix, and the `raw-capture`, `run-archive`, and `run-record` roles.
- Stable reports are parsed from manifest-bound bytes, with unknown fields denied. Each report must use the pinned schema and encoding version 1. Its tests must be unique, canonically ordered, and all `passed`. Its stable identity must recompute. The receipt must record status `passed` and bind the same report and command.

### Measured values

The consumer profile values were remeasured from the pinned producer bundle. It was built locally in this branch's locked environment with `--option builders ''`, from the following expression:

`(builtins.getFlake (toString ./.)).inputs.mantle.packages.x86_64-linux.spacewasm-reference-bundle`

The build produced this store path:

`/nix/store/dmi5iszcjnhy3xln964cz3i6biqbmdjz-mantle-spacewasm-reference-bundle-e24cf09355a90497148eb5029fdb8e3400bd63e3`

The measured values follow:

- Manifest digest: `b3sum manifest.json` gave `2de0a78cb1655918e2c42880b21bf03f71aa36152382ef416ce56ab625180d5e`.
- Bundle identity: `jq .bundle_identity_blake3 manifest.json` gave `86142d4286235ab4c6e8fbf8b08daafa2c921363d7968ea0302ed5212b649896`.
- Runner digest: `b3sum` of `binaries/host/mantle-spacewasm-diagnostic-runner` gave `732da9b7276cb2c36dc0e5e308e8fd8e6bc88ddd665346c5afb1f351eda2a579`. This equals the manifest `host-runner` digest.
- Stable identities: `upstream-unit-tests` is `c2d8938e19d3cf48a9f349f7aa30633d867f456df1a34f3c0fa6517b59cf0e67`, with 231 tests passed. `upstream-spectest-address` is `46e88a64a9a2aca1e6eeca915a386c433fa97876e17b29fea81b1df2efad8097`, with 1 test passed.

The three profile values equal the values ported from `63b64678`, so no profile field changed after the port. The check's `nickel export` plus `cmp` confirms that the committed JSON matches the Nickel profile.

### Reproducibility

- `nix-store --realise <drv> --check -K --option builders ''` re-executed both test producers:
  - `1dpjc2ra…-unit-tests.drv` ran for 6 s.
  - `4fbx880k…-spectest-address.drv` ran for 4 s.
  - Nix reported that each output differs. Only `stdout.txt` and `stderr.txt` changed. `stable-report.json` and `receipt.json` were byte-identical across the original and re-executed outputs.
- `inputs.mantle.checks.x86_64-linux.spacewasm-reference-repeatability` passed with the same stable identities. It was already realised in the local store.
- `nix build --rebuild` passed for the `spacewasm-reference-evidence` and `spacewasm-reference-bundle` derivations. After the rebuild, the manifest digest was still `2de0a78c…`.

### Commands and results

All builds ran under pueue group `ccmantle`, with `--option builders ''` and outputs under `/tmp/ccmantle`. The substituters were limited to the reachable public caches.

- `cargo fmt -p chaoscontrol-wasm-differential -- --check` exited 0.
- `cargo test --locked -p chaoscontrol-wasm-differential` passed 19 tests with 0 failures.
- `cargo clippy --locked -p chaoscontrol-wasm-differential --all-targets -- -D warnings` exited 0.
- The first `nix build .#checks.x86_64-linux.spacewasm-mvp-differential --option builders '' -L` run at `63762da` exited 0 in 177 s:
  - The differential reported a match: `cases=14 mismatches=0`.
  - `report_identity_blake3` was `613ccb9ad62efc709bd6dc93dbf81ea99b96c993c2a0c74d8a69b3f31ef9632b`.
  - Resume equivalence passed with `segments=33`.
  - The wrong-pin, stale-manifest, stale-identity, and stale-runner denial controls failed with their named errors.
- The second run used the same command with `--rebuild` and exited 0. Nix re-executed the check and found the output byte-identical, including the same report identity.
- `nix build .#checks.x86_64-linux.evidence-contracts --option builders '' -L` exited 0.
- Direct CLI denials ran against a mutated copy of the pinned bundle. The profile manifest digest was refreshed for each copy:
  - A renamed test in a stable report with a refreshed member digest was rejected. The error was "stable report identity does not match its admitted facts".
  - The same tamper without a manifest refresh failed with "bundle member digest mismatch".
  - A removed stable-report member failed with "missing required stable evidence member".
  - An added `upstream-unit-tests.stdout.txt` member failed with "raw run capture member".
  - The unchanged copy passed admission.

### Residual non-claims and risks

- The measured digests depend on the build environment. Mantle's own lock measured a different manifest identity, `661243d3…`. Changing ChaosControl's `nixpkgs`, `crane`, or `octet` lock can change these bytes. That would require a new measurement under the same producer contract.
- The Mantle Nix lane hashes `jq -c` output, which includes a trailing line feed. The Mantle Rust core `canonical_identity` hashes compact serde bytes without one. Mantle's parity test compares only the canonical bytes, not the resulting digests. The consumer recomputes identities with the Nix lane's form because it produced the admitted bundle. The two Mantle lanes disagree on identity bytes. That is a producer issue, reported here but not repaired.
- Raw captures and run records are rejected by name and role only. A producer member with a new name and role for raw output would need a new denial rule.
- This resolves the SpaceWasm admission mismatch only. It makes no claim of SpaceWasm correctness, SpaceWasm and Wasmtime equivalence, WebAssembly conformance, full `nix flake check` acceptance, or Campaign adoption.
