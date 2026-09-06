# Binding-by-binding owner qualification

## Admission boundary

The second batch starts at `7ed930b` and changes 26 files in the seven-package scope.
The helper evaluates each reported import separately. A rejected import no longer prevents unrelated admitted edits.
The helper preserves unselected exports and `cfg` imports, but records their namespace bindings before it accepts a qualification.
It still rejects selected exports, selected conditional imports, namespace collisions, opaque attributes, child modules, globs, and unknown macro uses.
String-valued attributes also block a referenced binding. Edits cannot overlap source comments.

Positive and negative controls cover these rules, partial acceptance, idempotence, malformed Rust, and rejected report paths.
The helper remains a syntax tool. It does not resolve Rust names or establish independent review.
The public paths, signatures, wire fields, constants, and runtime decisions retain their existing meaning under the selected compiler checks.

## Checks and correction

The fresh seven-package baseline passes across all targets and all features.
The post-change tests and strict Clippy pass in the same scope.
Strict Rustdoc rejects seven links after private imports disappear. Explicit owner links correct them, and the strict retry passes.
The pinned Octet report decreases from 1,702 to 1,455 findings, with zero errors and unchanged policy identities.
The warning-only result does not establish strict acceptance.

An exact token comparison failed because rustfmt removes single-item import groups and adds permitted punctuation.
`binding-exact-syntax-attempt.rs.txt` and `binding-syntax-parity.log` retain that attempted comparison. It is not acceptance evidence.
The current helper does not expose that comparison mode.
A separate reconstruction starts from the exact Git source, applies the admitted edit plan, and runs the same formatter.
All 26 files match that reconstruction byte for byte before the seven Rustdoc link corrections.
This result checks the edit plan and formatter output, not semantic equivalence.

## Evidence

- `binding-baseline.log` records the pre-change tests.
- `binding-controls-retained.log` records the current helper controls.
- `binding-applied.log` records edits and retained bindings.
- `binding-tests.log` and `binding-clippy.log` record the post-change compiler checks.
- `binding-rustdoc.log` and `binding-rustdoc-corrected.log` record the rejected links and successful retry.
- `binding-plan-reconstruction.log` and `binding-formatted-plan.log` record the reconstruction and exact comparisons.
- `binding-octet.log` records the current findings and policy hashes.

The SpaceWasm admission guard, expected bundle identities, dependency pins, and lint policy remain unchanged.
The quality and publication tasks remain open.
