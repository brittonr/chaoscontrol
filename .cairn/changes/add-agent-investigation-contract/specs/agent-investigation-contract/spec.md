## ADDED Requirements

### Requirement: Mid-run bug events expose a stable failure reference

r[chaoscontrol.agent_investigation.bug_event_reference]

Every `BugFound` dashboard event MUST carry the emitted failure's dedup key and the
replay parent depth of the branch that produced it. The dedup key MUST equal the
key stored in the corresponding checkpoint bug report, and the depth MUST equal
that report's `replay_parent_depth`. Both fields MUST be additive: previously
serialized events without them MUST deserialize with defined defaults, and
serialization MUST always include them. Emitting an event MUST NOT serialize,
hash, or persist a snapshot inside the run loop.

#### Scenario: Event matches the checkpoint record

- GIVEN a failing assertion admitted by exploration
- WHEN the bug is extracted and a `BugFound` event is emitted
- THEN the event carries the same dedup key and replay parent depth as the bug report

#### Scenario: Legacy event input

- GIVEN a serialized `BugFound` event without the new fields
- WHEN it is deserialized
- THEN the fields take their defined defaults and no other event field changes

### Requirement: Checkpoint export selects by failure identity

r[chaoscontrol.agent_investigation.export_by_dedup_key]

`export-bugs` MUST accept a dedup-key filter and write only checkpoint bugs whose
stored key equals it. The filter MUST compose with the assertion-id and
minimum-parent-depth filters, `max_bugs` MUST still cap writes after matching, and
a checkpoint bug without a stored key MUST NOT match a requested key. Unmatched
filters MUST write no artifacts and MUST NOT error.

#### Scenario: Keyed selection

- GIVEN a checkpoint holding bugs with distinct dedup keys
- WHEN the operator exports with one of those keys
- THEN only that bug's artifact is written and the summary counts the scan and match

#### Scenario: Legacy checkpoint

- GIVEN a checkpoint whose bugs predate stored dedup keys
- WHEN the operator requests a specific key
- THEN nothing matches, no artifact is written, and the command exits successfully


### Requirement: Checkpoint failures can be located without artifacts

r[chaoscontrol.agent_investigation.locate_bugs]

A noninteractive listing operation MUST surface admitted checkpoint-held failures
as stable records without writing artifacts or loading snapshot bytes. Each record
MUST carry its checkpoint index and artifact name, assertion id, optional dedup
key, tick, replay parent depth, optional snapshot digest, identity presence, and
process instances. Filters by assertion id, dedup key, and inclusive tick range
MUST compose, an inverted range MUST be rejected, and a bug without a stored dedup
key MUST NOT match a requested key.

#### Scenario: Locate by failure identity
- GIVEN a checkpoint holding admitted failures
- WHEN the operator lists with a dedup key or tick range
- THEN only matching records are returned in checkpoint order and no bug file is written

#### Scenario: Invalid range
- GIVEN a minimum tick greater than the maximum
- WHEN listing runs
- THEN it fails with an explicit range error


### Requirement: A single failure can be inspected as versioned read-only JSON

r[chaoscontrol.agent_investigation.inspect_bug]

An inspect operation MUST accept exactly one selector (checkpoint index or stored
dedup key), validate the checkpoint bug set, and return a versioned record
containing the checkpoint index, artifact name, the exact stored bug report, and
explicit non-claims. Zero matches, ambiguous matches, and empty or dual selectors
MUST fail with distinct typed outcomes. Inspection MUST NOT write artifacts, load
snapshot bytes, execute a VM, or claim replay evidence.

#### Scenario: Inspect by key
- GIVEN a checkpoint with one bug for a dedup key
- WHEN the operator inspects that key
- THEN the exact stored report is returned with its artifact name and non-claims

#### Scenario: Selector errors
- GIVEN no selector, both selectors, or a selector matching zero/two bugs
- WHEN inspection runs
- THEN it fails with the corresponding typed outcome and writes nothing


### Requirement: Retained checkpoint admission is versioned and fails closed

r[chaoscontrol.agent_investigation.verify_checkpoint]

An admission operation MUST validate checkpoint structure and the referenced
snapshot artifacts, returning a versioned record with a BLAKE3 checkpoint digest,
bug counts, validated counts, per-bug issues, a verdict, and explicit non-claims.
Each unique snapshot reference MUST be validated once; missing artifacts, digest
mismatches, or depth mismatches MUST make the verdict invalid rather than being
repaired or ignored. Admission MUST NOT execute a VM or claim replay evidence.

#### Scenario: Admission with a missing artifact
- GIVEN a checkpoint whose bug references an absent snapshot file
- WHEN admission runs
- THEN the verdict is invalid and the issue names the bug and reason

#### Scenario: Shared snapshot reference
- GIVEN several bugs referencing the same snapshot artifact
- WHEN admission runs
- THEN the artifact bytes are validated once and each matching bug counts as validated


### Requirement: Admission can bind the expected session digest

r[chaoscontrol.agent_investigation.session_digest_binding]

Admission MUST accept an expected checkpoint digest and reject a mismatch before
validating any snapshot artifact. The digest MUST be the BLAKE3 digest of the exact
checkpoint bytes and MUST be reported in the admission record. A mismatch MUST be a
distinct typed failure, not a warning, and MUST NOT fall back to the actual digest.

#### Scenario: Bound digest matches
- GIVEN a checkpoint and its reported BLAKE3 digest
- WHEN admission runs with that digest
- THEN it proceeds with artifact validation

#### Scenario: Bound digest differs
- GIVEN an expected digest that differs from the checkpoint bytes
- WHEN admission runs
- THEN it fails with a digest-mismatch error before reading snapshots


### Requirement: A failure moment has a stable read-only reference

r[chaoscontrol.agent_investigation.moment_reference]

A moment-reference operation MUST bind the checkpoint BLAKE3 digest, checkpoint
index and artifact name, bug/assertion identity, dedup key, recorded failure tick,
replay parent depth, and the snapshot digest when a reference is present. A supplied
tick MUST equal the recorded failure tick or fail with a distinct error. A present
snapshot reference MUST be validated (digest and depth) before the digest is
reported. The record MUST state that it is not replay evidence and that the tick is
the recorded failure tick, not an arbitrary seek position.

#### Scenario: Stable reference
- GIVEN a valid checkpoint bug
- WHEN a moment reference is requested
- THEN the session digest, bug identity, tick, and optional snapshot digest are returned together

#### Scenario: Tick mismatch
- GIVEN a requested tick different from the stored failure tick
- WHEN the reference is requested
- THEN it fails with a typed tick-mismatch error


### Requirement: Recorded sessions expose bounded JSON seek/inspect

r[chaoscontrol.agent_investigation.debug_state_json]

A debug command MUST support a versioned JSON output that binds the recording
BLAKE3 digest, session id, seed, configuration summary, total ticks, target tick,
inclusive event window, nearest checkpoint (with distance) or null, bounded event
projections, checkpoint serial output, and explicit non-claims. Event projections
MUST be capped, and the record MUST NOT claim a live VM, replay verdict, or that the
nearest checkpoint is replayable.

#### Scenario: Seek within a recording
- GIVEN a recorded session
- WHEN debug runs with JSON output at a tick
- THEN the digest, window bounds, nearest checkpoint, and bounded events are returned together

#### Scenario: Missing checkpoint
- GIVEN a tick before any checkpoint
- WHEN debug runs with JSON output
- THEN the nearest checkpoint is null and the rest of the bounded record is still returned


### Requirement: Admission distinguishes structural from snapshot validation

r[chaoscontrol.agent_investigation.admission_modes]

Admission MUST expose a structure-only mode that validates the checkpoint record and
bug set without reading snapshot artifacts, and MUST report the mode, whether
snapshots were checked, the referenced/validated counts, and mode-specific
non-claims. Full mode MUST retain snapshot digest/depth validation and fail closed.
Structure-only success MUST NOT be presented as snapshot admission or replay
evidence.

#### Scenario: Structure-only with absent artifacts
- GIVEN a checkpoint whose snapshot artifact is absent
- WHEN structure-only admission runs
- THEN it succeeds with `snapshots_checked: false` and no snapshot issues

#### Scenario: Full admission still fails
- GIVEN the same checkpoint
- WHEN full admission runs
- THEN it reports the missing artifact and an invalid verdict


### Requirement: A referenced parent snapshot can be decoded and summarized

r[chaoscontrol.agent_investigation.inspect_snapshot]

An inspection operation MUST decode the replay parent snapshot referenced by one
selected checkpoint failure, validate its depth against the bug, and return a
versioned summary bound to the checkpoint digest, snapshot digest, tick, VM count,
per-device state entry counts, pending observation count, and fault operation
sequence. It MUST fail when the bug has no reference or the artifact is invalid,
and MUST NOT execute a VM or claim replay/admissibility.

#### Scenario: Parent summary
- GIVEN a bug with a valid snapshot reference
- WHEN the snapshot is inspected
- THEN the summary carries the snapshot digest and decoded shape fields

#### Scenario: Missing reference
- GIVEN a bug without a snapshot reference
- WHEN inspection runs
- THEN it fails with a distinct not-found outcome


### Requirement: Unique snapshot references can be listed without decoding

r[chaoscontrol.agent_investigation.list_snapshots]

A listing operation MUST group checkpoint snapshot references by path and return the
digest, store, codec, depth, referencing bug indices, and artifact names without
reading artifact bytes. References sharing a path with differing digests or depths
MUST be rejected as a conflict. The record MUST carry the checkpoint digest and
state that presence is not admission or replay evidence.

#### Scenario: Shared reference grouped
- GIVEN several bugs referencing one snapshot path
- WHEN references are listed
- THEN one group carries all referencing bug indices and artifact names

#### Scenario: Conflicting metadata
- GIVEN one path with two different digests
- WHEN references are listed
- THEN the listing fails with a conflict error


### Requirement: A failure's restore inputs can be planned and checked

r[chaoscontrol.agent_investigation.restore_plan]

A planning operation MUST emit a versioned read-only restore plan for one selected
failure, binding the checkpoint digest and bug identity to the snapshot reference/
path presence, declared kernel/initrd/disk paths and their presence, and a readiness
verdict with an explicit missing list. It MUST NOT execute a VM, verify snapshot
digests, or claim admission, replay, or workload correctness.

#### Scenario: All declared inputs present
- GIVEN a failure whose snapshot file and declared kernel/initrd exist
- WHEN the plan is requested
- THEN it reports ready with an empty missing list

#### Scenario: Missing input
- GIVEN a declared kernel path that does not exist
- WHEN the plan is requested
- THEN readiness is false and the missing list names the kernel path


### Requirement: Restore planning can verify the snapshot on request

r[chaoscontrol.agent_investigation.restore_plan_verification]

Restore planning MUST support an explicit verification mode that validates the
referenced snapshot artifact's digest and depth before reporting readiness, and MUST
report whether verification ran. In the default mode the plan MUST NOT decode the
artifact, and an invalid or missing artifact MUST NOT be reported as verified.

#### Scenario: Verification requested
- GIVEN a valid stored snapshot artifact
- WHEN planning runs in verification mode
- THEN the plan reports snapshot_verified=true and ready=true

#### Scenario: Verification failure
- GIVEN a missing or invalid artifact
- WHEN planning runs in verification mode
- THEN planning fails rather than reporting a ready, unverified plan
