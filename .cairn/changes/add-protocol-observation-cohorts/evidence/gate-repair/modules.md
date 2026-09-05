# Inline-module owner paths

The second owner-path batch qualifies standard-library references in 42 source files.
The helper admits checked inline modules and rejects external modules, conflicting bindings, unknown glob imports, and ambiguous macro tokens.
The edits preserve serialized fields, constants, control flow, and external effects.

The first attempt missed constructors after a single field colon inside `vec!`.
The compiler rejected unresolved `BTreeMap` references in protocol and SDK tests.
A corrected token check distinguishes a field colon from the two colons in a qualified path.
Positive and negative helper controls cover both forms.
The corrected helper regenerated the batch from the verified `d9a8dae` source.

| Check | Result | Evidence |
| --- | --- | --- |
| First compiler rejection | Expected unresolved name after an incomplete edit | `module-owner-tests.log` |
| Partial manual correction | Exposed the remaining SDK occurrence | `module-owner-tests-corrected.log` |
| Regenerated batch, seven-package tests | Passed across all targets and all features | `module-owner-final-tests.log`, `.exit` |
| Regenerated batch, strict Clippy | Passed across the same scope | `module-owner-clippy.log`, `.exit` |
| Pinned Octet | 2,245 warnings, zero errors | `octet-modules.log`, `.exit` |

The warning count decreased by 75 from the previous 2,320 findings.
The original report had 2,458 findings. No catalog, severity, scope, baseline, or suppression changed.
Octet still reports `warning-only`. This result does not establish strict acceptance or lifecycle completion.

The next helper uses compiler-reported binding names rather than a fixed standard-library list.
Its candidate set excludes generated BPF skeleton output. Those findings need a generator repair and remain in the full lint report.
It also rejects unknown absolute paths, path traversal, conflicting namespaces, and attribute-dependent bindings.
No source changes from that next helper form part of this batch.
