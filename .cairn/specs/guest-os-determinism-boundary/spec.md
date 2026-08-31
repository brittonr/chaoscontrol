# Guest Os Determinism Boundary Specification

## Purpose

Defines the `guest-os-determinism-boundary` capability.

## Requirements

### Requirement: Boot entropy is deterministic

r[chaoscontrol.guest_determinism.boot_entropy] The VMM MUST inject a run-derived Linux boot seed and MUST bind that derivation to the profile. The admitted byte-exact CRNG claim MUST begin from one quiescent snapshot after boot. The profile MUST NOT claim equal CRNG output across independent fresh Linux boots.

#### Scenario: Snapshot continuations share entropy
- GIVEN one admitted quiescent snapshot
- WHEN two continuations read `getrandom` in the same call order
- THEN the byte streams MUST match.

#### Scenario: Fresh boot claim rejected
- GIVEN two independent fresh Linux boots with the same run seed
- WHEN their CRNG outputs are compared
- THEN any equality is observational only and MUST NOT be promoted by this profile.

### Requirement: Time is pinned to the virtual clock

r[chaoscontrol.guest_determinism.time_surface] Admitted monotonic reads MUST use deterministic jiffies driven by the VMM timer plan. The profile MUST hide direct TSC use and MUST NOT admit host wall-clock or host RTC reads.

#### Scenario: Monotonic reads replay
- GIVEN two continuations from one admitted snapshot
- WHEN guest code reads `clock_gettime(CLOCK_MONOTONIC)` in the same call order
- THEN the returned deltas MUST match.

### Requirement: Memory layout is profile-fixed

r[chaoscontrol.guest_determinism.layout] The admitted profile MUST control supported kernel and process layout randomization so identical runs produce identical observed layout. The receipt MUST bind the run configuration and exact layout policy. A profile MUST NOT claim caller-seeded Linux ASLR unless that seed is applied through a supported kernel interface.

#### Scenario: Fixed-layout replay
- GIVEN two identical runs under the fixed-layout profile
- WHEN guest code records the load addresses of its mappings
- THEN the addresses MUST match.

#### Scenario: Layout policy recorded
- GIVEN one completed run
- WHEN its receipt is inspected
- THEN the layout-policy binding MUST be present.

### Requirement: Signal ordering is schedule-derived

r[chaoscontrol.guest_determinism.signals] The admitted guest signal fixture MUST execute under the deterministic vCPU schedule and MUST produce the same observed signal order for identical runs. The profile MUST NOT claim control of host signal timing.

#### Scenario: Signal order replays
- GIVEN a guest that records the order of two delivered signals
- WHEN two identical runs execute
- THEN the recorded order MUST match.

### Requirement: Determinism validation is a gate

r[chaoscontrol.guest_determinism.validation_fixture] A validation fixture guest MUST reach a stable marker before reading every admitted surface. The gate MUST capture one complete VM snapshot at that marker and require bit-exact equality across two restored continuations before the profile may claim reproducibility.

#### Scenario: Bit-exact fixture passes
- GIVEN two continuations from one admitted fixture snapshot
- WHEN the drift gate compares their outputs
- THEN the gate MUST accept only identical byte output.

#### Scenario: Entropy drift detected
- GIVEN two runs whose `/dev/urandom` reads differ
- WHEN the drift gate compares their outputs
- THEN the gate MUST fail and identify the entropy surface.

### Requirement: Claims stay bounded

r[chaoscontrol.guest_determinism.boundary] The determinism profile MUST enumerate its admitted surfaces and MUST NOT claim reproducibility for reads outside that list.

#### Scenario: Outside-surface claim
- GIVEN a report that claims reproducibility for a read not on the admitted surface list
- WHEN claim validation runs
- THEN the claim MUST be rejected.

### Requirement: Determinism validation is adversarial

r[chaoscontrol.guest_determinism.validation] Validation MUST pair a positive bit-exact fixture with negative fixtures for entropy, clock, layout, and signal-order drift.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to admit the determinism profile
- WHEN pure, VM, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
