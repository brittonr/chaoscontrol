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
