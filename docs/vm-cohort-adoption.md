# VM Cohort adoption

ChaosControl uses VM Cohort for retained initialized bases, private overlays, clone creation, KVM descriptors, lifecycle reduction, and cleanup.

ChaosControl keeps exact snapshot, fault, scheduler, assertion, coverage, exploration, replay, guest, and evidence meaning.

## Immutable dependency

Cargo and Nix select VM Cohort revision `ab123e3673b6dd616b3df5d044026b5e85755149`.

The supported path has no branch, tag, sibling path, or moving fallback.

`contracts/vm-cohort-adoption/adoption.ncl` records the reviewed source, mapping, verification, selection, authority, and non-claims.

The Nix checks compare the Cargo manifest, Cargo lock, Nix input, source files, and contract projection.

## Mapping

`chaoscontrol-vm-cohort-adapter` validates the complete ChaosControl snapshot before planning.

The adapter maps these exact facts:

- materialized initialized memory bytes and their BLAKE3 identity;
- materialized initialized disk bytes and their BLAKE3 identity;
- vCPU state, topology, memory layout, kernel, guest, disk format, runtime, and adapter identities;
- the requested clone count, resource limits, and product-owned context identity.

The adapter rejects incomplete metadata, invalid scheduler or assertion state, profile drift, adapter drift, and HLT-latch topology drift.

VM Cohort does not parse ChaosControl fault, scheduler, assertion, coverage, exploration, replay, or evidence types.

## Restore and activation

VM Cohort prepares private memory, private disk, one KVM VM, in-kernel devices, and vCPUs for each clone.

ChaosControl then applies `VmSnapshot::restore_devices_only` through the exact live descriptors.

The restore occurs before endpoint binding and activation.

The active result retains `KvmCohortRuntime`. Dropping a temporary runtime cannot represent an active cohort.

If an effect or restore fails, the adapter records the failed operation before cleanup.

Confirmed cleanup returns a failed, cleaned state. Unknown cleanup retains the runtime and exact cleanup obligations for operator resolution.

An unknown effect or cleanup outcome never becomes success.

## Behavioral parity

The bounded corpus compares legacy and shared behavior for:

- initial reads;
- writes, snapshots, and restores;
- clone divergence;
- memory overlays;
- dirty-page behavior;
- bounded out-of-range errors.

The comparison uses normalized BLAKE3 observations. It does not require equal internal storage identities.

A mismatch blocks shared-mechanism selection. A match does not prove either implementation correct.

## Selection and rollback

The supported shared mechanism is `vm-cohort` after every required bounded case passes.

The existing duplicate path is `diagnostic-rollback-only`. It is not an automatic fallback or a release fallback.

The selection gate fails on source drift, failed parity, unknown verification, or consumer-policy leakage.

## Authority and evidence boundaries

A VM Cohort plan, observation, conformance result, or receipt does not grant product authority.

ChaosControl still owns fault choice, schedule choice, assertion evaluation, coverage, exploration, replay, product evidence, and release decisions.

Cleanup observations report bounded release results. They do not prove data erasure.

KVM smoke reports one admitted host run. It does not prove guest correctness, sandboxing, portability, or universal determinism.

## Verification

Run the focused portable checks:

```bash
nix develop -c cargo test -p chaoscontrol-vm-cohort-adapter --all-targets
nix develop -c cargo clippy -p chaoscontrol-vm-cohort-adapter --all-targets -- -D warnings
nix build .#checks.x86_64-linux.vm-cohort-dependency --no-link -L
nix build .#checks.x86_64-linux.vm-cohort-adoption-contract --no-link -L
nix build .#checks.x86_64-linux.vm-cohort-adapter-octet-deny-all --no-link -L
```

Run the admitted-host KVM cases explicitly:

```bash
nix develop -c cargo test -p chaoscontrol-vm-cohort-adapter --lib tests::kvm:: -- --ignored --nocapture
```
