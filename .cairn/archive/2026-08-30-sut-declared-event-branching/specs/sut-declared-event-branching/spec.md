# SUT-Declared Event Branching Specification

## Purpose

Let a workload declare states the harness should treat as exploration branch points, and bind those events to evidence.

## ADDED Requirements

### Requirement: Guests can declare branch events

r[chaoscontrol.event_branching.marker_api] The SDK MUST provide a declared event marker with a stable logical key, an optional structured detail value, optional canonical state and logical-position refs, and membership in the existing assertion identity namespace. Instance refs MUST NOT change the marker's logical identity.

#### Scenario: Marker with identity
- GIVEN a guest declares a marker with a stable key
- WHEN the marker is reached
- THEN the event MUST record that key and its identity without claiming pass or fail.

#### Scenario: Marker reached repeatedly
- GIVEN the same marker is reached many times at the same state
- WHEN the event is recorded
- THEN repeated identical instances MUST collapse without corrupting frontier state.

### Requirement: Markers become branch candidates

r[chaoscontrol.event_branching.frontier_entry] When the VMM observes a declared marker, the explorer MUST treat the current snapshot as a parent candidate and score it by marker rarity and novelty.

#### Scenario: Rare marker becomes a parent
- GIVEN a marker that is declared and reached
- WHEN the frontier is updated
- THEN a parent snapshot for that marker MUST be available for subsequent branches.

#### Scenario: Marker budget exceeded
- GIVEN a run that reaches markers beyond the admitted bound
- WHEN the event is recorded
- THEN a typed limit event MUST be recorded and the frontier MUST remain valid.

### Requirement: Evidence binds markers to snapshots

r[chaoscontrol.event_branching.evidence_binding] Bug reports and replay verdicts that reference a declared marker MUST record the marker identity, owning guest or process, tick, optional canonical state and logical-position refs, and a valid parent snapshot reference. Every present ref MUST validate.

#### Scenario: Marker-linked bug reproduces
- GIVEN a bug linked to a declared marker with a valid parent snapshot
- WHEN snapshot-backed replay runs
- THEN the replay verdict MUST be accepted only when the artifact validates.

#### Scenario: Marker identity drifts
- GIVEN evidence whose recorded marker identity differs from the executed marker
- WHEN replay validation runs
- THEN replay MUST fail closed.

### Requirement: Reachability of markers is evidence

r[chaoscontrol.event_branching.marker_gap] A campaign MUST record whether each declared marker was reached, and MUST not claim marker coverage for a marker that never fired.

#### Scenario: Declared marker never reached
- GIVEN a declared marker that no branch reaches
- WHEN the campaign report is produced
- THEN the report MUST list that marker as a coverage gap.

### Requirement: Marker validation is adversarial

r[chaoscontrol.event_branching.validation] Validation MUST pair a positive rare-event branching fixture with negative fixtures for never-reached markers, budget exhaustion, and identity drift.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to admit declared event branching
- WHEN SDK, explorer, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
