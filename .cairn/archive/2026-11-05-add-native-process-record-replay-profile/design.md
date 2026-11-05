# Design: Publish a native-process record/replay profile

## Context

`chaoscontrol-protocol` already owns versioned, bounded, canonical producer
contracts under the `std` feature. The new descriptor follows the same shape:
typed fields, explicit bounds, canonical JSON identity, and a fixed non-claim
set. No new dependency is introduced.

## Decisions

### Decision: One profile plus one descriptor record set

The profile binds schema, version, producer cohort, record schema, replay
class, and non-claims. The descriptor binds the profile and an ordered record
set. Identity is BLAKE3 over canonical serialization of the validated value.

### Decision: Records describe lifecycle facts only

A record carries a stable process key, generation, strictly increasing
sequence within a generation, executable BLAKE3, start and end logical ticks,
one exit classification (`exited`, `killed`, `restarted`), and at most four
BLAKE3 evidence references. It carries no host path, raw environment, secret
text, or free-form payload.

### Decision: Reject drift before use

Validation rejects a stale schema or version, a record-schema mismatch, a
non-current non-claim set, an empty or over-bound record set, non-BLAKE3
identities, an end tick before the start tick, duplicate or decreasing
sequences within one generation, and drifted process ordering. A descriptor is
validated before another repository may treat it as admitted evidence.

### Decision: Diagnostics stay detached

The descriptor is not a semantic observation, world root, authority grant, or
equivalence proof. Consumers may retain it beside their own evidence; they must
not use it to satisfy missing observations or to permit repair, voting, or
release actions.

## Test Design

Positive: complete descriptor and profile identity; matched replay class.
Negative: stale schema/version, record-set bounds, duplicate or decreasing
sequence, drifted order, non-BLAKE3 identities, reversed ticks, evidence-ref
overflow, host path, secret-bearing text, and a changed non-claim set.

## Claim Boundary

The profile proves only that a bounded descriptor is canonically identified and
internally consistent over supplied facts. It does not prove that any process
ran as described or that a replay reproduces behaviour.
