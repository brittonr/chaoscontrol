## ADDED Requirements

### Requirement: Local product determinism matrix rows [r[vm-determinism-drift.local-product-matrix]]

The bounded determinism matrix MUST support named rows for the single-machine multi-hypervisor product profile and MUST keep row evidence scoped to listed kernel, initrd, device, clock, controller, and workload profiles.

#### Scenario: Local product rows bind multi-hypervisor profile [r[vm-determinism-drift.local-product-matrix.rows]]

- GIVEN an operator runs the local product determinism matrix
- WHEN the matrix receipt is emitted
- THEN it records row IDs for selected single-machine multi-hypervisor profiles, including worker count, workload identity, kernel/initrd fingerprints, device profile, clock profile, and controller configuration
- AND it states that unlisted profiles and arbitrary guests remain unproven

#### Scenario: Failing or unsupported rows remain visible [r[vm-determinism-drift.local-product-matrix.negative-rows]]

- GIVEN a matrix row fails drift comparison or is unsupported by the current local profile
- WHEN the matrix summary is rendered
- THEN it preserves the row with pass/fail/unsupported status and bounded mismatch details instead of omitting the row
