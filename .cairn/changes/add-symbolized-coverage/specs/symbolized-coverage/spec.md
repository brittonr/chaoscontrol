# Symbolized Coverage Specification

## Purpose

Associate admitted coverage observations with exact build symbols, loaded modules, and source locations, and expose bounded symbolized reports and descriptive finding correlations without changing coverage guidance.

## ADDED Requirements

### Requirement: Symbol catalogs are exact build artifacts

r[chaoscontrol.coverage_symbols.catalog] Each instrumented module MUST emit a versioned symbol catalog binding its content identity, language, instrumentation kind, function identities, source paths relative to a reviewed root, line and column ranges, and the identifier or address ranges it instruments. The catalog MUST be bounded, canonical, and content-addressed. A build that cannot supply a complete mapping, including a link-time-optimized build instrumented at the wrong stage, MUST declare an incomplete catalog rather than emit guessed locations.

#### Scenario: A Rust module catalog is admitted

- GIVEN an instrumented Rust module with a complete catalog and reviewed source root
- WHEN catalog admission runs
- THEN every declared location MUST resolve within the declared root
- AND the catalog identity MUST match the module content identity.

#### Scenario: A location escapes the declared root

- GIVEN a catalog entry has an absolute, traversing, or unresolved source path
- WHEN catalog admission runs
- THEN the catalog MUST be rejected or the entry MUST be marked incomplete.

### Requirement: Runtime module loads are exact and ordered

r[chaoscontrol.coverage_symbols.module_identity] The guest runtime MUST record every admitted instrumented module load with its content identity, load ordinal, and the instrumentation identifiers it registers. Coverage observations MUST be associated with one loaded module identity. Duplicate or conflicting identities MUST fail closed, and observations from unrecorded modules MUST be counted as unaccounted rather than assigned to a best match.

#### Scenario: A shared library loads after start

- GIVEN an instrumented shared module is loaded after process start
- WHEN its coverage observations arrive
- THEN they MUST map through its recorded load identity and ordinal.

#### Scenario: A module identity is missing

- GIVEN an observation arrives from a module absent from the load inventory
- WHEN mapping runs
- THEN the observation MUST be counted as unaccounted
- AND no source location MUST be assigned.

### Requirement: Coverage mapping is a pure projection

r[chaoscontrol.coverage_symbols.mapping] A pure core MUST map admitted sanitizer identifiers, program counters, and supported edge identifiers through the admitted catalogs and load inventory. Mapping MUST be deterministic, MUST retain per-module mapped and unmapped counts, and MUST enforce finite bounds on modules, locations, identifiers, address ranges, and report rows. It MUST perform no file, clock, process, network, or environment access.

#### Scenario: The same observations map identically

- GIVEN the same catalogs, module loads, observations, and limits
- WHEN mapping runs twice
- THEN the mapped locations, counts, and unmapped accounting MUST be identical.

#### Scenario: Bounds are exceeded

- GIVEN a catalog or observation set exceeds a declared bound
- WHEN mapping is admitted
- THEN it MUST return a typed over-bound result
- AND it MUST NOT truncate silently or allocate beyond the bound.

### Requirement: Symbolized reports are bounded and descriptive

r[chaoscontrol.coverage_symbols.report] The report MUST present per-module and per-location coverage, unaccounted counts, missing or incomplete catalogs, and the declared limits. For an explicitly selected failing and control cohort, it MAY rank locations by differential coverage, retaining the compared branch identities and counts. The report MUST NOT label a ranked location as causal, as a correctness result, or as complete coverage.

#### Scenario: A bug cohort is compared

- GIVEN one failing branch and one matching control branch with admitted symbolized coverage
- WHEN the differential ranking is produced
- THEN every row MUST retain both branch facts and counts
- AND the report MUST NOT use causal or proof wording.

#### Scenario: A module has no catalog

- GIVEN the admitted cohort includes a module with no usable catalog
- WHEN the report is produced
- THEN the module MUST appear as incomplete or unaccounted
- AND the report MUST NOT fabricate locations for it.

### Requirement: Symbolized evidence is bound and validatable

r[chaoscontrol.coverage_symbols.evidence] The receipt MUST bind catalog identities, loaded-module identities and ordinals, observation source identities, declared limits, mapping counts, the report digest, and any missing-symbol blockers. Validation MUST reject catalog drift, module drift, a changed observation source, a changed bound, or a report whose mapped/unmapped accounting does not recompute.

#### Scenario: A catalog changes after the report

- GIVEN a retained report and a catalog whose content identity no longer matches
- WHEN receipt validation runs
- THEN validation MUST reject the symbolized claim.

#### Scenario: The report is outside its bound

- GIVEN a persisted report exceeds the declared size or row bound
- WHEN the receipt is validated
- THEN it MUST be rejected or explicitly marked incomplete before publication.

### Requirement: Symbolized coverage claims stay bounded

r[chaoscontrol.coverage_symbols.nonclaims] Symbolized coverage MUST NOT claim source correctness, complete line coverage, semantic equivalence, causal attribution, universal instrumentation, VM replay proof, or release eligibility. It MUST describe only the admitted modules, catalogs, observations, bounds, and computed mapping facts.

#### Scenario: A symbolized report is promoted to proof

- GIVEN a report labels its differential ranking as the cause of a failure
- WHEN non-claim validation runs
- THEN the report MUST fail validation.

### Requirement: The symbolized rail has positive and negative fixtures

r[chaoscontrol.coverage_symbols.fixtures] The rail MUST include positive fixtures for exact Rust mapping, a shared-module load, a KCOV program-counter mapping, an incomplete-catalog fallback, and a bounded differential report, plus negative fixtures for path escape, missing module identity, duplicate identity, catalog drift, unmapped accounting mismatch, over-bound input, and causal overclaim.

#### Scenario: The rail is proposed for product use

- GIVEN catalogs, runtime module records, the pure mapping core, reports, evidence, and fixtures are complete
- WHEN focused Cargo, octet, Cairn, and Nix checks run
- THEN every positive fixture MUST pass
- AND every negative fixture MUST fail at its declared boundary.
