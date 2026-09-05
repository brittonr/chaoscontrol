# Explicit Serde ownership

The first source batch qualifies Serde derive macros and retains trait-only imports where method resolution needs them.
The compiler removes unused trait imports. Public types and serialized field names remain unchanged.

The helper rejects ambiguous inline scopes, conditional attributes, renamed imports, stale edits, and overlapping edits.
Its positive controls cover exact output, Unicode spans, unchanged foreign imports, and repeated application.
A compiler failure exposed one inherited macro use in `oracle/marker.rs`. That child now names both Serde derives explicitly.
The helper does not establish general Rust name-resolution equivalence.

| Check | Result | Evidence |
| --- | --- | --- |
| Seven-package baseline, all targets and features | Passed | `baseline-tests.log` |
| Compiler before child correction | Failed on missing Serde derives | `serde-compiler.log` |
| Compiler after child correction | Passed | `serde-compiler-after-marker.log` |
| Seven-package tests, all targets and features | Passed | `serde-tests.log` |
| Seven-package Clippy with `-D warnings` | Passed | `serde-clippy.log` |

This batch does not establish a clean Octet gate or lifecycle completion.
The stored-parent KVM cases remain outside this batch's ordinary test invocation.
