# Feature-aware SDK test harness

## Scope

The third source batch repairs the previously failing no-default test scope.
The minimal SDK keeps its `no_std` library and unit-details API. The full SDK keeps JSON details and its existing default feature.
Only the minimal host test harness links `std` for test allocation.
Private coverage helpers compile only in the full library or the host tests. Their hash algorithm and bitmap partition remain unchanged.
The coverage hash retains FNV-1a for compatibility with existing process-local pseudo-coverage. Durable protocol identities still use BLAKE3.

The shared assertion tests use feature-appropriate details instead of an unavailable JSON dependency.
The tests keep their successful and failed inputs. A minimal-only regression checks single evaluation for both true and false conditions.
Protocol tests gate two constants with their existing `std`-only decoder tests.
The patch changes no runtime signature, protocol field, identity domain, dependency version, or library feature definition.

## Target admission

The workload example requires the full workload harness and JSON output.
The multiprocess test requires the full supervisor and guest process types.
The stable-assertion test requires the full catalog and local JSON output.
Their Cargo targets now declare `required-features = ["full"]`.
The existing supervisor binary retains the same requirement.
These targets remain part of the all-feature test scope. They are not minimal-mode tests.
Explicit no-default requests for each of the three targets fail with the required-feature diagnostic.

## Evidence

| Check | Evidence | Result |
| --- | --- | --- |
| Fresh no-default baseline | `minimal-tests-before.log` | Fails on missing host and JSON test imports |
| First test repair | `minimal-tests-after.log` | Exposes full-only workload and multiprocess targets |
| First target correction | `minimal-tests-corrected.log`, `minimal-clippy.log` | Exposes the full-only stable-assertion target |
| Final no-default tests, all compatible targets | `minimal-tests-final.log` | 15 protocol and 42 SDK tests pass |
| Final strict no-default Clippy, same targets | `minimal-clippy-final.log` | Passes with warnings denied |
| Original no-default library/binary check | `minimal-build-final.log` | Passes without the two earlier coverage-helper warnings |
| Explicit incompatible target requests | `minimal-reject-*.log` | Cargo rejects all three without `full` |
| Final matrix result | `minimal-matrix.exit` | Zero, after positive and negative outcomes |

The full-feature compiler checks use `source-checked-tests.log`, `source-checked-clippy.log`, and `source-checked-rustdoc.log` after the final target declaration.
The earlier `source-final-*` logs precede that declaration and remain historical evidence.

## Non-claims

Host tests do not establish a freestanding guest runtime, process authentication, or transport delivery.
A false condition in minimal mode remains a no-op assertion emission. The new regression checks expression evaluation, not an evidence-bearing verdict.
The repair does not establish strict Octet acceptance or resolve the separate SpaceWasm bundle mismatch.
