# Simulator and campaign profile boundary

ChaosControl uses Nickel for reviewed pre-run intent. Rust owns admission, construction, execution, observations, and evidence.

## Ownership

| Family | Nickel owns | Rust owns |
| --- | --- | --- |
| VM run | Artifact references, BLAKE3 identities, guest command line, seed, topology, scheduler, exploration limits, coverage mode, and log policy | `VmConfig`, `ExplorerConfig`, KVM setup, VM exits, snapshots, bugs, and reports |
| In-process simulator | Workload, scheduler, clock, RNG, simulated I/O profiles, schedule reference, SHA-256 interoperability digests, artifacts, seed, and scope | `SimulatorConfig`, steps, operations, observations, comparisons, and receipts |
| Campaign | Unique seeds, run profile, workers, mutation ranges, havoc limits, scenario reference, metrics, output layout, and resource limits | `CampaignConfig`, threads, progress, failures, checkpoints, reports, and resume state |
| Finite schedule | Ordered closed fault descriptors, targets, partitions, parameters, labels, topology, and source identity | `FaultSchedule`, applicability, attempts, application, observation, and effects |

`contracts/evidence/registry.ncl` is the authority registry. Runtime records cannot become Nickel-authored profile fields.

## Run conversion inventory

`RunProfile::try_into_explorer_config` maps these fields without ambient input:

- `artifacts.kernel`, `artifacts.initrd`, and `artifacts.disk` map to runtime paths after path and BLAKE3 checks.
- `guest_cmdline` maps to `VmConfig.extra_cmdline`.
- `seed` is explicit. Campaign conversion replaces it only with an admitted seed from the profile.
- `topology.num_vms`, `num_vcpus`, and `memory_mib` map with checked integer conversions.
- `topology.scheduling` maps to `SchedulingStrategy` and schedule diversity.
- All exploration limits map to `ExplorerConfig` fields.
- Coverage mode and address must agree. Blind mode requires address `0`.
- Determinism-log policy, output, register interval, and memory hashing map together.

Rust defaults remain for CPU feature policy, rare-edge scoring, and fields that the profile does not claim to configure. Host CPU affinity remains an operator/runtime setting. It is not profile authority.

## Campaign and resume boundary

The campaign profile maps workers, mutations, havoc, metrics, output, and the complete run profile. `SerializableCampaignConfig`, `CampaignProgress`, checkpoints, completed seeds, failed seeds, elapsed time, coverage, bugs, and reports remain Rust-derived.

A scenario profile field is a BLAKE3-bound artifact reference. Conversion also requires a separately prepared `ScenarioConfig`. Reference presence and prepared-value presence must agree.

## Projection workflow

Run one of these explicit commands before runtime admission:

```text
cargo run -p chaoscontrol-evidence --bin check-profile-projections -- --root . --write
cargo run -p chaoscontrol-evidence --bin check-profile-projections -- --root .
```

The workflow evaluates Nickel outside runtime hot paths. It canonicalizes JSON and records BLAKE3 identities for the source, imports, evaluator, profile, and projection.

Use `check-profile-admission <kind> <projection> <receipt>` to test the Rust boundary without starting a VM, thread, or simulator. Admission rechecks source, contract, import, evaluator, profile, and canonical projection identities. Rust uses bounded regular-file reads, closed serde shapes, and repeated safety checks.

## Non-claims

A valid profile does not prove KVM availability, guest correctness, deterministic replay, fault application, fault observation, campaign completion, report correctness, receipt acceptance, or product readiness.
