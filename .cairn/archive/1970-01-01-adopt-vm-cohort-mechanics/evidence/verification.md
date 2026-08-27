# Verification evidence

Date: 2026-08-27

## Source and ownership

ChaosControl pins VM Cohort at `ab123e3673b6dd616b3df5d044026b5e85755149`.

Cargo selects three packages from `rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3` with that exact revision.

`Cargo.lock` binds the same revision and commit for all three packages.

Nix selects `git+rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3?rev=ab123e3673b6dd616b3df5d044026b5e85755149`.

The dependency gate rejects branch, tag, path, and sibling fallback declarations.

The gate also rejects ChaosControl fault, replay, exploration, or evidence dependencies inside VM Cohort source.

The successful fetch proves only exact source access on this host. It does not prove independent seed replication or future availability.

## Reviewed baseline

The consumer branch starts from ChaosControl revision `7433557b85990f0f07a37ca44b97fef26c2a4c7e`.

The recorded baseline included six positive and eight negative portable descriptor cases.

It also included 42 focused snapshot tests, one ignored snapshot test, one exact KVM replay test, 13 copy-on-write tests, and two divergence tests.

The broad current workspace test command passed after the adapter work. The change did not remove those baseline cases.

## Adapter and parity

The adapter performs complete snapshot preflight before cohort planning.

It binds initialized memory, initialized disk, topology, vCPU state, memory layout, kernel, guest, disk format, runtime, adapter, and context facts.

It applies exact ChaosControl vCPU and in-kernel device state before endpoint binding and activation.

The active result retains the live `KvmCohortRuntime`.

Failed creation returns exact cleanup state. Unknown cleanup retains the runtime and obligations for operator resolution.

The bounded parity report contains five agreeing rows:

1. initial disk read;
2. write, snapshot, and restore;
3. clone divergence;
4. memory overlay;
5. out-of-range error classification.

The comparison uses normalized observations. It does not claim equal storage identities or implementation correctness.

## Positive and negative cases

The portable adapter suite passed nine tests and ignored three explicit KVM tests.

The admitted-host KVM run then passed all three ignored tests:

- exact vCPU state through VM Cohort descriptors;
- complete exact snapshot restore before activation;
- injected partial creation with confirmed cleanup and no activation.

Negative cases reject or preserve:

- incomplete snapshot metadata;
- profile drift;
- immutable base drift;
- execution snapshot drift;
- shared mutable surface tampering;
- partial creation;
- unknown effect outcomes;
- unknown cleanup selection status;
- consumer-policy leakage;
- parity overclaim;
- later authority escalation.

The typed Nickel profile passed. Five invalid profiles failed for moving source, sibling fallback, policy leakage, unknown cleanup, and receipt authority.

## Commands and results

Passed:

```text
nix develop --builders '' -c cargo test -p chaoscontrol-vm-cohort-adapter --all-targets
result: 9 passed, 0 failed, 3 ignored

nix develop --builders '' -c cargo test -p chaoscontrol-vm-cohort-adapter --lib tests::kvm:: -- --ignored --nocapture
result: 3 passed, 0 failed

nix develop --builders '' -c cargo clippy -p chaoscontrol-vm-cohort-adapter --all-targets -- -D warnings
result: passed

nix develop --builders '' -c cargo test --workspace --all-targets
result: passed, including live repository KVM tests

nix develop --builders '' -c cargo clippy --workspace --all-targets -- -D warnings
result: passed

nix build .#checks.x86_64-linux.vm-cohort-dependency --no-link -L --builders ''
result: passed

nix build .#checks.x86_64-linux.vm-cohort-adoption-contract --no-link -L --builders ''
result: passed

nix build .#checks.x86_64-linux.vm-cohort-adapter-octet-deny-all --no-link -L --builders ''
result: clean, 0 findings, 0 warnings, 0 errors
```

Attempted broad Nix rail:

```text
nix flake check -L --builders ''
result: blocked while fetching wasmparser 0.220.1 because crates.io returned HTTP 403
```

The full Nix failure occurred outside the VM Cohort checks. All three VM Cohort Nix checks passed.

Two inherited repository checks remain separate:

- product-scope generation stops at active change `add-protocol-observation-cohorts`, which lacks a registry intent;
- the license-boundary script stops at pre-existing `chaoscontrol-property-suite`, which lacks a package-policy row.

The adapter has an AGPL package row, local license text, Cargo-deny exception, documentation entry, and guest-to-host dependency prohibitions.

## Selection and non-claims

The selected shared mechanism is `vm-cohort`.

The old duplicate path is `diagnostic-rollback-only`. It is not an automatic or release fallback.

VM Cohort plans, observations, conformance results, and receipts do not grant fault, scheduler, assertion, coverage, exploration, replay, evidence, or release authority.

KVM smoke does not prove guest correctness, sandboxing, portability, cleanup erasure, universal determinism, or release eligibility.
