## MODIFIED Requirements

### Requirement: Guest-controlled decoders and planners have fuzz targets

r[chaoscontrol.fuzzing.guest_boundary_targets]

The protocol payload/catalog/descriptor/event decoders and the virtio
descriptor-chain and block/net/entropy/scratch planners MUST have libFuzzer targets
that take fuzzer input as arguments, never KVM or device state, and bound queue,
chain, and memory shapes small enough to reach accept and reject decisions. The
targets MUST assert decoder round-trip/canonicality and independent plan-soundness
invariants and MUST run with overflow checks enabled.

A documented smoke command MUST build each target and run it under explicit time,
timeout, RSS, and allocation limits, seeded from committed corpora, and MUST NOT be
part of the default flake check. A crash found under the smoke MUST become a
permanent regression test beside the existing adversarial tests rather than being
ignored or suppressed.

#### Scenario: Protocol frames round-trip
- GIVEN arbitrary bytes
- WHEN a descriptor, event, catalog completion, or payload decodes
- THEN re-encoding reproduces the canonical frame, or the decoded value for payloads

#### Scenario: Plans satisfy their declared bounds
- GIVEN fuzzer-chosen descriptors, regions, and small limits
- WHEN a chain or request plan succeeds
- THEN every buffer is contained, no index repeats, and counts, aggregate bytes, and device or transfer limits hold

#### Scenario: Bounded smoke run
- GIVEN an operator runs the smoke command with a time budget
- WHEN every target exits without a crash
- THEN the command reports completion and the run proves only those bounded inputs and limits

#### Scenario: Crash becomes a regression
- GIVEN the smoke produces a crasher
- WHEN the finding is fixed
- THEN a regression test reproduces the former input and the target corpus retains it
