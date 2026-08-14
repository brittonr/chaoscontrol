## Why

ChaosControl has deterministic virtual time, seeded entropy, scheduling, fault timing, input-choice, snapshot, and in-process simulation mechanisms across VMM, explore, fault, and evidence crates. Aspen has related deterministic runtime abstractions. Copying either product model into another repository would create competing semantics.

A shared core is useful only after ChaosControl and Aspen identify one lower-level contract that both can wrap.

## What Changes

- Establish a product-neutral `deterministic-sim` repository under AGPL-3.0-or-later after an explicit Aspen comparison.
- Provide pure virtual clock, ChaCha20 entropy stream, runnable-task scheduling, scheduled-event, and recorded-choice state machines.
- Version every algorithm, domain, stream, ordering rule, and snapshot schema that affects replay.
- Use checked transitions and typed failures instead of silent overflow or wall-clock fallback.
- Keep guest progress measurement, KVM control, fault meaning, workload adapters, persistence, and evidence claims in consumer shells.
- Migrate suitable ChaosControl mechanisms through compatibility adapters and exact replay fixtures.
- Do not extract the current small xorshift-based in-process simulator unchanged.

## Impact

- **Source candidates**: VMM scheduler and entropy device, fault schedule, explore input tree, and evidence in-process simulator mechanisms.
- **New repository**: `deterministic-sim` with a `no_std` plus `alloc` core and optional standard-library adapters.
- **Cross-repo coordination**: Aspen owns Molten runtime policy. ChaosControl owns VMM execution and replay evidence.
- **Compatibility**: schedule choices, entropy bytes, virtual ticks, event order, and snapshots need explicit version gates.
- **Claims**: the shared core is deterministic over supplied facts. It does not prove host, VM, workload, or distributed-system determinism.
