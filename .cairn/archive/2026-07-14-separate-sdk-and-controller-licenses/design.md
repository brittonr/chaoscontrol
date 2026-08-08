## Design

The license boundary follows process and authority ownership rather than crate size.

### Decisions

1. **Apache guest boundary.** `chaoscontrol-protocol`, `chaoscontrol-sdk`, guest networking/support crates, concrete guest fixtures, and copied workload templates remain Apache-2.0. Applications linking these surfaces may choose their own larger-work license.

2. **AGPL host boundary.** `chaoscontrol-fault`, `chaoscontrol-vmm`, `chaoscontrol-trace`, `chaoscontrol-explore`, `chaoscontrol-dashboard`, `chaoscontrol-replay`, and `chaoscontrol-evidence` declare AGPL-3.0-or-later.

3. **Dependency direction stays safe.** AGPL host crates may depend on Apache guest/protocol crates. Apache guest/SDK crates must not depend on AGPL host crates.

4. **Templates stay permissive.** Workload scaffolds copied into downstream repositories are Apache-2.0 and do not inherit the controller license merely because ChaosControl executes them.

5. **Third-party terms remain intact.** Bundled web assets, dependencies, and referenced kernels/workloads retain upstream licenses and notices.

6. **No evidence-identity change.** Package license metadata is distribution metadata, not deterministic VM, snapshot, replay, or evidence identity input unless a versioned evidence schema explicitly says otherwise.

### Validation shape

Positive checks assert every workspace package and copied template maps to the intended license. Negative fixtures reverse representative SDK/controller rows, omit a package, and introduce a forbidden Apache-to-AGPL dependency edge. Cargo-deny policy must accept both repository-owned expressions while retaining its existing third-party allowlist.
