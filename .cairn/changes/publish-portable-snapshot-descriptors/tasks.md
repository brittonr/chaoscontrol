## Phase 1: Descriptor core

- [x] [serial] Record baseline exact snapshot, codec, chunk manifest, replay-parent reference, restore preflight, poison, and bounded continuation tests. r[chaoscontrol.snapshot_descriptor.verification]
- [x] [serial] Inventory every behavior-relevant snapshot, topology, KVM, vCPU, device, backend, scheduler, time, entropy, guest-artifact, and payload-closure field. r[chaoscontrol.snapshot_descriptor.complete_cohort]
- [x] [depends:snapshot-descriptor-inventory] Define bounded descriptor, tagged digest, closure member, topology, destination observation, preflight, restore observation, and diagnostic DTOs. r[chaoscontrol.snapshot_descriptor.contract] r[chaoscontrol.snapshot_descriptor.closure]
- [x] [depends:snapshot-descriptor-dtos] Implement domain-separated BLAKE3 canonical hash material with deterministic inventory and closure ordering. r[chaoscontrol.snapshot_descriptor.contract]
- [x] [depends:snapshot-descriptor-dtos] Implement pure descriptor, completeness, closure, topology, and destination-compatibility validation. r[chaoscontrol.snapshot_descriptor.preflight]

## Phase 2: Projection and shell

- [x] [depends:snapshot-descriptor-core] Add Rust-owned JSON projection, exact schema snapshot, generated Nickel review contract, registry entry, and freshness checks. r[chaoscontrol.snapshot_descriptor.projection]
- [x] [depends:snapshot-descriptor-core] Emit descriptors for monolithic and ordered chunk-manifest snapshot artifacts with exact read-back. r[chaoscontrol.snapshot_descriptor.closure]
- [x] [depends:snapshot-descriptor-preflight] Add detached restore receipts for preflight, materialization, mutation start, phase result, poison, completion, and bounded continuation observations. r[chaoscontrol.snapshot_descriptor.restore_receipt]
- [x] [parallel] Add locator sidecars that cannot enter canonical descriptor identity or satisfy content availability. r[chaoscontrol.snapshot_descriptor.locator_boundary]
- [x] [parallel] Add a refs-only Molten-shaped consumer fixture without a direct Molten dependency or world-commit claim. r[chaoscontrol.snapshot_descriptor.consumer_contract]

## Phase 3: Verification and documentation

- [x] [parallel] Add positive exact monolithic, chunked, stable identity, matching destination, successful restore, and consumer-round-trip fixtures. r[chaoscontrol.snapshot_descriptor.verification]
- [x] [parallel] Add negative missing component, duplicate identity, wrong profile, stale schema, architecture mismatch, MSR drift, topology drift, device substitution, missing chunk, digest mismatch, unknown algorithm, path-only reference, locator substitution, unsupported destination, post-mutation failure, poison omission, and portability-overclaim fixtures. r[chaoscontrol.snapshot_descriptor.verification]
- [x] [serial] Document descriptor and payload versions, exact cohort, locator boundary, restore receipts, consumer use, algorithm tags, and non-claims. r[chaoscontrol.snapshot_descriptor.consumer_contract]
- [ ] [depends:snapshot-descriptor-verification] Run focused snapshot and evidence tests, contract generation and freshness checks, Octet, Clippy with warnings denied, Cairn validation and gates, lifecycle checks, and relevant portable and KVM Nix rails. r[chaoscontrol.snapshot_descriptor.verification]
  - Passed: focused descriptor, closure, evidence, exact KVM, contract-freshness, full workspace test, and Clippy-with-warnings-denied rails.
  - Blocked: configured Octet reports `warning-only` with 2,522 repository findings. The strict gate cannot accept this result; 151 findings name the new descriptor core or shell paths.
  - Blocked: the broad product-scope checker stops on the pre-existing active change `add-protocol-observation-cohorts`, which lacks a registry intent.
  - Blocked: broad Nix evaluation reaches the new descriptor check, then stops on the pre-existing invalid Mantle SpaceWasm store path.
