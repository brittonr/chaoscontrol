## Why

ChaosControl owns exact snapshot, fault, scheduler, assertion, coverage, exploration, replay, and evidence semantics. Its retained-base and private-overlay mechanics are now available from the product-neutral VM Cohort candidate.

The shared mechanism needs one real consumer before VM Cohort can publish a stable revision. ChaosControl needs behavioral parity and rollback evidence before it selects the shared path.

## What Changes

- Pin VM Cohort revision `ab123e3673b6dd616b3df5d044026b5e85755149` by Cargo and Nix.
- Add a narrow ChaosControl adapter that maps exact snapshot and initialized block facts into VM Cohort checkpoint, cohort, KVM, and cleanup contracts.
- Restore ChaosControl-owned vCPU and in-kernel device state through VM Cohort-owned live descriptors before clone activation.
- Run legacy and shared memory and disk paths over one bounded read, write, snapshot, restore, divergence, and error corpus.
- Keep ChaosControl fault, scheduler, assertion, coverage, exploration, replay, guest, and evidence types outside VM Cohort.
- Select the shared mechanism after parity. Keep legacy code only as named diagnostic rollback behavior.

## Impact

- **Crate**: new `chaoscontrol-vm-cohort-adapter` consumer shell.
- **Dependencies**: exact private Radicle VM Cohort candidate.
- **Behavior**: existing snapshot formats and ChaosControl policy remain owned and versioned here.
- **Testing**: positive parity, negative drift, authority-leak, cleanup, KVM smoke, Octet, Cargo, Cairn, and Nix checks.

## Out of Scope

This change does not transfer fault, replay, scheduler, assertion, coverage, exploration, workload, guest trust, evidence, or release authority. It does not claim that VM Cohort proves ChaosControl determinism or KVM correctness.
