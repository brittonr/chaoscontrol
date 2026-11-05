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
