## ADDED Requirements

### Requirement: Guest process admission is typed and bounded

r[chaoscontrol.guest_seccomp_admission.typed_admission]

An optional admission declaration MUST bind an explicit uid/gid map and/or a
stable syscall-policy id. A map MUST NOT map container root to guest root, MUST be
non-overlapping, and MUST respect declared size limits. An unknown or unenforceable
policy MUST be rejected with a distinct typed outcome, never ignored. Admission
runs in a pure core; application stays in the launcher shell.

#### Scenario: Root mapping rejected
- GIVEN a declaration whose container id 0 maps to guest id 0
- WHEN the admission core evaluates it
- THEN it rejects with a typed map error and no bundle is lowered

#### Scenario: Unknown policy rejected
- GIVEN a declaration naming a policy id that is not in the vendored set
- WHEN the admission core evaluates it
- THEN it rejects rather than lowering with an unfiltered process

### Requirement: Admission stays gated on scope and replay configuration

r[chaoscontrol.guest_seccomp_admission.gated_scope]

No bundle field or runtime filter MAY be added before the maintainer resolves the
no-container-runtime non-goal and before the applied policy id is part of the
deterministic replay configuration. A policy change MUST invalidate replay reuse.
Workload filters MUST NOT be applied to SDK, supervisor, or assertion transport
processes.

#### Scenario: Policy change invalidates reuse
- GIVEN a recorded run with one policy id
- WHEN a replay uses a different policy id
- THEN the replay is rejected rather than compared as equivalent
