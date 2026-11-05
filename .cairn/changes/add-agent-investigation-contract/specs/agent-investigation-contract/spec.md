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
