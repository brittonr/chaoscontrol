## Context

The replay crate has recording, checkpoint, debugger, inspection, and triage surfaces. The explorer writes frontier traces, checkpoint files, branch-marker coverage, assertion summaries, and bug reports. The VMM emits dlogs and serial output. The fault engine emits a stage ledger. These are all bounded, but they do not share an envelope or a query surface.

The existing debugger can seek within one in-memory recording and inspect memory and registers. The new persisted-session change owns cross-process exact restore. Neither provides a cross-source event query or a static timeline over the branch graph.

## Decisions

### Decision: One canonical event envelope, many producers

**Choice:** producers adapt their retained facts into one bounded envelope with a stable source identity, event kind, virtual tick, VM/process/thread owner, payload digest, bounded text, and sequence. The envelope is a view over retained artifacts; querying does not rewrite the source artifact.

**Rationale:** one query surface requires one meaning for time, source, and ordering. Rewriting producers would lose their existing evidence identities.

### Decision: The query language is a typed AST, not a scripting runtime

**Choice:** the language supports filter, map, contains, matches, union, window, and up-to-moment operations over the envelope. The pure core admits bounded ASTs and evaluates them deterministically. No arbitrary code, dynamic library, network call, or filesystem read occurs in the core.

**Rationale:** a typed AST is testable and safe. A general scripting runtime would introduce authority and determinism problems unrelated to the debug value.

### Decision: Moment queries state their evidence requirement

**Choice:** a historical query needs only retained events. A future-of-moment query or guest observation needs an exact persisted session and a validated checkpoint. If the session is missing, the query fails with the missing prerequisite; it does not boot a default VM or infer state from logs.

**Rationale:** the repository already distinguishes metadata-only recordings from exact sessions. The query surface must not collapse that boundary.

### Decision: Timeline rendering is static and bounded

**Choice:** the timeline renderer consumes admitted query results and branch facts and emits self-contained HTML, Markdown, or DOT within declared row, node, and byte bounds. It runs no server and performs no live updates.

**Rationale:** a static artifact is reviewable, local, and consistent with the evidence-first product boundary.

### Decision: Guest observation is a declared command, not a shell

**Choice:** an observation command comes from an admitted command profile with a stable identity, executable path, arguments, environment, working directory, virtual-tick deadline, and output bounds. The guest supervisor runs it in the restored guest. There is no ambient host shell and no host process execution. A command that mutates guest state is labeled counterfactual and cannot certify the original recording.

**Rationale:** a free-form shell would exceed the repository's typed-command and authority boundaries. A declared command still supports bounded inspection.

## Risks / Trade-offs

- Producers may disagree on tick, owner, or ordering; the envelope adapter must define those facts and reject ambiguous events.
- A query over every retained event can be expensive; scan, row, depth, and byte bounds are mandatory.
- Static timelines can imply completeness; the renderer must show source, horizon, truncation, and missing-artifact facts.
- Guest observation commands add an execution surface; admission, no-follow paths, bounded environment, and output limits must be tested adversarially.
- A future-of-moment query depends on retained snapshots; an absent or corrupt session must remain an explicit blocker.
