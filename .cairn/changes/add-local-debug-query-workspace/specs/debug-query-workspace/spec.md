# Debug Query Workspace Specification

## Purpose

Provide one bounded local query, timeline, and guest-observation workspace over retained ChaosControl artifacts without a hosted service, a general scripting runtime, or an ambient shell.

## ADDED Requirements

### Requirement: Events share one canonical envelope

r[chaoscontrol.debug_query.event_envelope] Retained sources MUST adapt into one bounded event envelope carrying a stable source identity, event kind, virtual tick, VM/process/thread owner, payload digest, bounded text, and source sequence. Supported sources MUST include recording events, dlog records, SDK lifecycle and assertion records, fault stage records, serial output, and branch-marker records. The adapter MUST preserve source identities, reject duplicate source positions, and MUST NOT rewrite or re-identify the source artifact. Ambiguous ordering, unknown owners, or records outside declared bounds MUST be typed failures.

#### Scenario: Assertion and fault events are queried together

- GIVEN a recording with assertion, fault, and serial records
- WHEN the envelope adapter loads them
- THEN every record MUST retain its source identity, tick, owner, and sequence
- AND querying MUST NOT mutate the recording.

#### Scenario: Two records claim one source position

- GIVEN two adapted events share a source identity and position but differ in content
- WHEN envelope admission runs
- THEN the duplicate MUST be rejected
- AND neither event MUST be silently preferred.

### Requirement: Queries are typed and bounded

r[chaoscontrol.debug_query.language] The query surface MUST accept a typed AST with filter, map, contains, matches, union, window, and up-to-moment operations. The pure evaluator MUST enforce named limits for AST depth, node count, scanned events, result rows, payload bytes, and operation count. It MUST perform no file, clock, process, network, environment, or dynamic-code access. Malformed, unsupported, or over-bound queries MUST return typed failures.

#### Scenario: The same query returns the same result

- GIVEN the same envelope set, AST, and limits
- WHEN evaluation runs twice
- THEN the rows, ordering, counts, and truncation facts MUST be identical.

#### Scenario: A query exceeds its scan bound

- GIVEN a query would scan more events than the declared bound
- WHEN evaluation runs
- THEN it MUST return a typed over-bound result
- AND it MUST NOT silently return a partial result as complete.

### Requirement: Moment selection states its evidence requirement

r[chaoscontrol.debug_query.moment] A query MUST select a moment by retained tick, checkpoint, or branch identity and declare its past and future horizon. A historical query MAY use retained events without a live VM. A future-of-moment query or any operation that needs restored state MUST require an exact persisted session and validated checkpoint; missing, corrupt, or incompatible session state MUST fail explicitly without boot fallback or inferred state.

#### Scenario: A historical window is queried

- GIVEN retained events and a moment inside the recording horizon
- WHEN a past-only window is queried
- THEN the result MUST return from retained events
- AND it MUST identify the source artifacts and horizon.

#### Scenario: A future window lacks a session

- GIVEN a query requests events after a moment but no exact session or checkpoint exists
- WHEN admission runs
- THEN it MUST reject the missing prerequisite
- AND it MUST NOT boot a default VM or infer future events.

### Requirement: Timelines are static and bounded

r[chaoscontrol.debug_query.timeline] The workspace MUST render a self-contained timeline or branch graph from admitted query results, frontier records, checkpoint records, marker records, fault facts, and branch parentage. Rendering MUST enforce named node, edge, row, and byte limits and MUST surface source, horizon, truncation, and missing-artifact facts. It MUST NOT start a server, perform live updates, or omit a declared gap.

#### Scenario: A branch graph is rendered

- GIVEN admitted frontier and checkpoint records for a bounded run
- WHEN the timeline renders
- THEN the output MUST show branch parentage and the selected facts
- AND it MUST name any missing or truncated input.

#### Scenario: The graph exceeds its node bound

- GIVEN the admitted graph exceeds the named node limit
- WHEN rendering runs
- THEN it MUST return a typed over-bound result
- AND it MUST NOT silently drop nodes.

### Requirement: Guest observation runs only declared commands

r[chaoscontrol.debug_query.guest_observation] A guest observation MUST come from an admitted command profile with a stable identity, relative executable path, bounded arguments and environment, relative working directory, virtual-tick deadline, and output byte bounds. The guest supervisor MUST execute it in the restored guest without an ambient host shell or host process execution. Missing, absolute, traversing, symlinked, over-bound, or unadmitted commands MUST be rejected before execution. A command that mutates guest state MUST be labeled counterfactual and MUST NOT certify the original recording.

#### Scenario: A declared read-only observation runs

- GIVEN an admitted command profile and a validated restored session
- WHEN the observation command runs
- THEN its output MUST remain within the declared bounds
- AND the result MUST retain the session, checkpoint, command, and deadline identities.

#### Scenario: An observation command escapes its profile

- GIVEN a command uses an absolute path, an escaping argument, or an ambient host shell
- WHEN admission runs
- THEN it MUST be rejected before a VM or process effect.

### Requirement: Query and observation evidence is bound

r[chaoscontrol.debug_query.evidence] Every query, timeline, or observation result MUST emit a bounded receipt binding source artifact identities, the admitted AST or command profile, moment and horizon, limits, result counts, truncation, session and checkpoint identities when used, and the output digest. Validation MUST reject source drift, AST or profile drift, missing inputs, accounting mismatch, and a result beyond its bound.

#### Scenario: A source artifact changes after the query

- GIVEN a retained query receipt and a source artifact whose identity no longer matches
- WHEN receipt validation runs
- THEN validation MUST reject the result.

#### Scenario: An observation result exceeds its output bound

- GIVEN a guest observation writes beyond its declared output limit
- WHEN the result is admitted
- THEN it MUST be marked incomplete or rejected
- AND it MUST NOT be published as a complete observation.

### Requirement: The workspace keeps local and non-causal claims

r[chaoscontrol.debug_query.nonclaims] The workspace MUST NOT claim hosted service support, complete event capture, causal attribution, source-level correctness, arbitrary command execution, remote execution, replay promotion, or release eligibility. A query result MUST describe only the admitted sources, moment, horizon, query, limits, and computed rows.

#### Scenario: A timeline is promoted to a complete history

- GIVEN a static timeline is labeled as a complete record of the run
- WHEN non-claim validation runs
- THEN the artifact MUST fail validation.

### Requirement: The workspace has positive and negative fixtures

r[chaoscontrol.debug_query.fixtures] The rail MUST include positive fixtures for a cross-source query, deterministic rerun, moment window, static branch graph, declared guest observation, and bounded receipt, plus negative fixtures for duplicate source positions, malformed or over-bound AST, missing session, source drift, missing artifact, graph over-bound, path escape, unadmitted command, output overflow, snapshot mismatch, and overclaim.

#### Scenario: The workspace is proposed for product use

- GIVEN the envelope adapters, query core, moment admission, renderer, observation path, evidence, and fixtures are complete
- WHEN focused Cargo, octet, Cairn, and Nix checks run
- THEN every positive fixture MUST pass
- AND every negative fixture MUST fail at its declared boundary.
