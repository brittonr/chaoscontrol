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

## Canonical adapter rules

The source identity is the BLAKE3 digest of the complete retained input bytes,
not a session name or a newly synthesized producer identity. File callers pass
that identity to recording, marker, supervisor-lifecycle, and fault-ledger
adapters. The dlog adapter hashes its complete supplied bytes itself. The
in-memory recording helper explicitly hashes canonical serialized recording
metadata; it does not claim that identity is the original on-disk artifact.
Adapters do not rewrite source artifacts. Payload digests bind the retained
record, including fields not projected into the bounded human-readable text.

| Source | Position and tick | Owner and interpretation |
|---|---|---|
| Recording events | Retained array index; recorded simulation tick | Checked VM index for assertion/serial/status records; `simulation_controller` for run-wide findings and legacy fault-selection projections. VM status is a recording fact, not an SDK process event. `FaultFired` means selected only, never applied or observed. |
| Recording fault stages | Positions follow recording events; original stage sequence remains in details | `fault_engine`; validated ledger and contiguous, nonoverlapping, strictly tick-ordered round deltas provide exact stage ticks. Without deltas only `Selected` on an exact simulation-tick boundary is supported; subsequent stages fail with ambiguous time. |
| Recording control ledger | Positions follow recording events and stages; standalone ledger positions are array indices; exact control tick | `fault_engine`; pause request admission/rejection, selection suppression, and application suppression retain the full control record, including run/schedule IDs, stage-event boundary, decisions, requester and due-opportunity facts. Suppression is not a stage, application, observation, or coverage claim. |
| Dlog | Original `record.seq`, not byte offset; explicit per-record retained simulation-tick anchors | Explicit VM owner and retained vCPU slot. Virtual TSC remains a detail and is never interpreted as a simulation tick. Context must cover every record in byte order with matching sequence and nondecreasing ticks; duplicated/reversed positions fail. |
| SDK supervisor lifecycle | Receipt event array index and retained supervisor tick | Manifest-admitted VM/process identity; role, generation, and exact lifecycle kind retained. The process manifest and multiprocess receipt must validate. No process/thread identity is inferred from a PID or VM status. |
| SDK assertions | Recording event index and recorded tick | Recording VM owner and assertion identity/outcome; no unretained thread or process identity is invented. |
| Serial | Recording event index and recorded tick | Recording VM owner. UTF-8 text, including newline/carriage-return/tab, is preserved within its byte bound; other control characters and oversized text fail. |
| Branch marker | Marker array index and `observed_tick` | Validated marker owner; full run/state/logical-position context remains in details. Marker order is the retained array order, not a newly inferred branch order. |
| Retained frontier trace | Exact JSONL record index; product round and selected-entry IDs remain decision-domain facts, not simulation ticks | Existing explorer producer decoder supplies a bounded read-only projection. Trace v1 omits simulation time, checkpoint identities and fork parentage; timeline admission emits named gaps with the raw source BLAKE3 identity, never timed nodes or invented fork edges. A valid prefix missing its stop is explicitly source-truncated; malformed partial JSON is rejected. |
| Real frontier entries | Original entry ID, exact `snapshot.tick`, exact `parent` ID | Typed `FrontierEntry` adapter supplies checkpoint/branch facts. Referenced ancestors, parent-before-child time and depth consistency are required; missing/pruned ancestors fail explicitly, not as fabricated roots. |

Every adapter caps record counts at the named maximum scan bound and admits
each envelope's text/detail bounds. Numeric VM conversion is checked. Input
envelopes must name a VM or scoped producer/process owner. Every repeated
`(source_id, source_sequence)` input position is rejected, even if identical;
union operators may still collapse repeated selections internally. Query rows
are ordered by `(tick, source_id, source_sequence)`; source-array positions are
not rewritten to match this display order.

Unsupported inputs fail explicitly: partial dlog records, unknown dlog tags,
dlogs without exact owner/tick context, legacy post-selection fault stages
without exact round deltas, forged/missing ledger attempts, overlapping or
uncovered deltas, unknown supervisor processes, over-bound records, and
ownerless envelopes. Raw SDK transport frames, fallback JSONL logs, aggregate
assertion summaries, arbitrary lifecycle JSON, host logs, and serial strings
that merely resemble structured events are not implicitly decoded as events.
They require a retained canonical recording/supervisor record or an explicitly
admitted envelope; the workspace does not infer missing ticks or owners.

## Workspace evidence and graph accounting

Moment-scoped query evaluation applies an explicit inclusive window around the
admitted AST. Workspace receipts bind the full `MomentContext`, including
retained horizon, session/checkpoint/branch identities when present, exact
source identities, limits, scanned/matched/returned counts, truncation, and the
canonical serialized output digest. Receipt construction as well as validation
re-evaluates the query; a plausible but forged row cannot acquire a receipt.
Source fingerprints include every input field, including supplied projections.
The I/O admission shell owns the binding from every actually read artifact to
its raw-byte identity, including empty artifacts. Moment source identities are
sorted, unique, bounded `b3:` names; every envelope origin must be a member.
The pure core cannot reopen artifacts and does not require every admitted
artifact to contain an event. Empty-artifact identities remain bound in the
full moment and visible timeline sources rather than inventing a sentinel row.

Static workspace timelines consume those admitted inputs/results plus explicit
branch/checkpoint/frontier facts. Numeric fact IDs and parents are retained;
missing parents, duplicate IDs, cycles, parent-after-child ticks, or facts
outside the retained horizon fail rather than silently dropping topology.
Declared missing artifacts are rendered visibly in Markdown and DOT. Selected
rows, truncation, sources, moment and both horizons are always visible. The
single accepted claim is `retained_facts_only`; a complete-history claim fails
deserialization. Named row and node bounds count fact nodes, selected row nodes,
and missing-artifact notes together; edge bounds count every parent edge, and
byte bounds include metadata, rows, notes, labels and closing syntax.
Untrusted labels, gap names, owners and serial text use the shared literal
Markdown escape helper: HTML, explicit links/images, formatting and bare
URL/email autolinks cannot become active content. Escaping occurs before byte
accounting and output digests; retained payload bytes and identities are not
rewritten. DOT labels remain quoted and escaped as text, not graph attributes.

Timeline receipts bind the full input, query receipt, full moment, source IDs,
limits, exact node/edge/row accounting, missing-artifact list, truncation and
exact rendered output. Changed facts, missing inputs, metadata, output or
accounting fail validation. These are local retained-fact projections, not
complete capture, causal proof, replay promotion or release eligibility.
Source truncation is independent of query truncation. Timeline inputs and
receipts retain `source_truncated`; visible aggregate `truncated` is the OR of
that fact and the validated query-result truncation. The original query result
and receipt accounting are never modified to accommodate an incomplete source.
The `--frontier` shell input is the existing `frontier-trace.jsonl` bytes, not
a new user-authored graph schema. Its admitted raw identity is included even
when no chronological node can legitimately be produced.

Implementation fixtures exercise these contracts without claiming execution
evidence until the parent integration checks run.
