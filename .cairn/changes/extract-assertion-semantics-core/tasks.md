## Phase 0: Prerequisites and inventory

- [ ] [serial] Complete the unified AGPL license boundary before adding the shared dependency to protocol or SDK crates. [depends:adopt-unified-agpl-license]
- [ ] [serial] Complete strict assertion conflict rejection and freeze its accepted identity behavior before extraction. [depends:reject-assertion-identity-conflicts]
- [ ] [serial] Inventory model, catalog, oracle, merge, wire, persistence, and readiness responsibilities across current crates. r[shared.assertion_semantics.migration]

## Phase 1: Shared repository

- [ ] [serial] Establish the `assertion-semantics` AGPL repository with model, catalog, and oracle crates plus immutable publication. r[shared.assertion_semantics.repository]
- [ ] [serial] Extract the `no_std` plus `alloc` descriptor, normalization, canonical bytes, domain-separated BLAKE3 fingerprint, and token types. r[shared.assertion_semantics.model]
- [ ] [parallel] Extract pure catalog insertion, completion, collision, event resolution, and merge logic. r[shared.assertion_semantics.catalog]
- [ ] [parallel] Extract pure run transitions, oracle state transitions, snapshots, and report aggregation. r[shared.assertion_semantics.oracle]

## Phase 2: Boundaries and adapters

- [ ] [serial] Keep command IDs, wire framing, guest macros, KVM dispatch, codecs, file output, and readiness policy in ChaosControl adapters. r[shared.assertion_semantics.chaoscontrol_boundary]
- [ ] [parallel] Add a narrow Valence export adapter without transferring Evidence IR, role, promotion, provenance, or release ownership. r[shared.assertion_semantics.valence_boundary]
- [ ] [serial] Define version compatibility checks between shared crates and ChaosControl protocol and report schemas. r[shared.assertion_semantics.compatibility]

## Phase 3: Parity and migration

- [ ] [parallel] Compare exact canonical bytes, fingerprints, catalog identities, and tokens over the maintained positive corpus. r[shared.assertion_semantics.migration]
- [ ] [parallel] Compare conflict, malformed descriptor, forced collision, stale token, unknown event, legacy, and merge rejection outcomes. r[shared.assertion_semantics.validation]
- [ ] [serial] Migrate protocol, SDK, oracle, explore, replay, and evidence callers only after parity passes. r[shared.assertion_semantics.migration]
- [ ] [serial] Remove duplicated semantics while retaining ChaosControl adapters and compatibility fixtures. r[shared.assertion_semantics.chaoscontrol_boundary]

## Phase 4: Publication checks

- [ ] [parallel] Add shared positive and negative tests for model, catalog, oracle, snapshot, and merge behavior without mocks. r[shared.assertion_semantics.validation]
- [ ] [serial] Run shared repository checks, guest target checks, focused ChaosControl tests, workspace checks, dependency policy, and Cairn gates before sync or archive. r[shared.assertion_semantics.validation]
