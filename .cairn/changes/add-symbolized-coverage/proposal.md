# Proposal: Symbolized coverage evidence

## Why

ChaosControl collects two useful coverage signals: SanCov edge hits from instrumented Rust workload code and KCOV program counters from the guest kernel. Both are opaque bitmaps or address sets. The explorer can tell that a branch is new, but a person or an operator report cannot say which module, function, file, or line produced it, and a bug report cannot rank the source locations whose coverage differs around a failure.

Antithesis treats coverage instrumentation as line-level data with symbol catalogs, a load-time module inventory, and source association in its reports. ChaosControl's bitmap-first model is effective for guidance but insufficient for triage and for identifying instrumentation gaps in the admitted cohort.

This change adds a bounded symbolized-coverage rail. It does not change the coverage-guided explorer's bitmap semantics and does not claim complete, causal, or source-correct coverage.

## What Changes

- Define a build-owned symbol catalog for each instrumented module, binding content identity, language, instrumentation kind, source locations, functions, and instrumented identifiers. r[chaoscontrol.coverage_symbols.catalog]
- Record the exact loaded-module set and associate runtime coverage observations with one module identity. r[chaoscontrol.coverage_symbols.module_identity]
- Add a pure mapping from sanitizer-edge identifiers and KCOV program counters to declared source locations, with explicit unmapped accounting. r[chaoscontrol.coverage_symbols.mapping]
- Produce a bounded symbolized coverage report and a descriptive failing-versus-control coverage ranking for a selected finding cohort. r[chaoscontrol.coverage_symbols.report]
- Bind catalogs, module loads, observation sources, limits, and the final report into a validation receipt. r[chaoscontrol.coverage_symbols.evidence]
- Preserve existing edge-coverage guidance and fail closed for symbolized claims when a module, symbol catalog, or location is missing or stale. r[chaoscontrol.coverage_symbols.nonclaims]
- Add positive and negative fixtures for exact mapping, shared libraries, link-time optimization, module drift, unmapped observations, and overclaim. r[chaoscontrol.coverage_symbols.fixtures]

## Impact

- **Build and packaging**: emit symbol catalogs and preserve them with the guest artifact closure; link-time-optimized builds must instrument at link time or declare an incomplete catalog.
- **Guest runtime**: record loaded modules and their exact identities without inventing source locations.
- **Pure core**: symbol admission, identity matching, address/ID mapping, unmapped accounting, and report construction.
- **Shells**: bounded artifact loading, observation joins, report rendering, and receipt writing.
- **Existing owners**: `chaoscontrol-explore` keeps coverage guidance; `chaoscontrol-evidence` keeps evidence records; the VMM keeps execution and KCOV ownership.

## Lifecycle Prerequisites

None for the pure rail and symbol-catalog format. A real symbolized claim requires an admitted build cohort and a KVM run that loads the declared modules.

## Out of Scope

- Changing AFL-style edge coverage, tree/state coverage, or explorer scoring.
- Source-code correctness, line-level semantic understanding, or causal attribution.
- Automatic source download, debuginfod, remote symbol services, or a new debugger.
- Dynamic language evaluation, uninstrumented native modules, containers, hosted services, or non-Rust SDKs.

## Affected Specs

- `symbolized-coverage`: catalog, module identity, mapping, report, evidence, non-claims, and fixtures.
