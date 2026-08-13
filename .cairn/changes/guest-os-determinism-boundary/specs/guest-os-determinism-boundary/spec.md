# Guest OS Determinism Boundary Specification

## Purpose

Make every admitted guest operating-system surface reproducible so unmodified guest code replays bit-exactly within a declared profile.

## ADDED Requirements

### Requirement: Boot entropy is deterministic

r[chaoscontrol.guest_determinism.boot_entropy] The VMM MUST inject the guest kernel's boot-time entropy from a run-derived deterministic stream so that the CRNG and the `/dev/urandom` device read sequence are reproducible for the same seed and configuration.

#### Scenario: Identical runs share entropy
- GIVEN two runs with the same seed and configuration
- WHEN each guest reads `/dev/urandom` in the same call order
- THEN the byte streams MUST match.

#### Scenario: Seed changes the stream
- GIVEN two runs with different seeds
- WHEN each guest reads `/dev/urandom`
- THEN the streams MAY differ, and both MUST remain reproducible for their own seed.

### Requirement: Time is pinned to the virtual clock

r[chaoscontrol.guest_determinism.time_surface] Time reads available to guest code MUST derive from the pinned virtual TSC and MUST NOT observe host wall-clock or host RTC variation within the declared profile.

#### Scenario: Monotonic reads replay
- GIVEN two identical runs
- WHEN guest code reads `clock_gettime(CLOCK_MONOTONIC)` in the same call order
- THEN the returned deltas MUST match.

### Requirement: Memory layout is run-derived

r[chaoscontrol.guest_determinism.layout] ASLR and process memory layout seeds MUST derive from the run configuration so that identical runs produce identical layout, and MUST be recorded for reproducibility.

#### Scenario: ASLR replay
- GIVEN two identical runs
- WHEN guest code records the load addresses of its mappings
- THEN the addresses MUST match.

#### Scenario: Layout seed recorded
- GIVEN one completed run
- WHEN its receipt is inspected
- THEN the layout seed MUST be present.

### Requirement: Signal ordering is schedule-derived

r[chaoscontrol.guest_determinism.signals] Signal delivery order MUST derive from the deterministic vCPU schedule and MUST NOT depend on host signal timing.

#### Scenario: Signal order replays
- GIVEN a guest that records the order of two delivered signals
- WHEN two identical runs execute
- THEN the recorded order MUST match.

### Requirement: Determinism validation is a gate

r[chaoscontrol.guest_determinism.validation_fixture] A validation fixture guest MUST read every admitted surface, record the values, and require bit-exact equality across repeated identical runs before the profile may claim reproducibility.

#### Scenario: Bit-exact fixture passes
- GIVEN two identical runs of the validation fixture
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
