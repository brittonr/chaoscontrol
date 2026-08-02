## Phase 0: Foundation

- [ ] [serial] Complete the unified repository license change before adding AGPL shared dependencies to former Apache packages. [depends:adopt-unified-agpl-license]
- [ ] [serial] Inventory every ChaosControl bounded file, JSON, serialization, and decompression implementation with its callers and limits. r[shared.bounded_input.migration]
- [ ] [parallel] Record corresponding Cairn, OnixOS, and Mantle mechanisms as consumer evidence without importing their product policy. r[shared.bounded_input.repository]

## Phase 1: Shared repository and pure core

- [ ] [serial] Establish the `bounded-input` repository, AGPL package artifacts, independent crate manifests, and immutable publication workflow. r[shared.bounded_input.repository]
- [ ] [serial] Define caller-owned typed limits, typed violations, and pure counter and transition logic. r[shared.bounded_input.policy] r[shared.bounded_input.boundary]
- [ ] [parallel] Add plain positive and negative assertions for every byte, structure, allocation, and arithmetic boundary. r[shared.bounded_input.validation]

## Phase 2: JSON and file mechanisms

- [ ] [parallel] Implement iterative bounded JSON preflight with byte, depth, node, and string limits. r[shared.bounded_input.json]
- [ ] [parallel] Implement bounded JSON serialization that cannot publish oversized output as complete. r[shared.bounded_input.serialization]
- [ ] [serial] Implement already-open regular-file reads and an explicit directory-capability adapter with typed unsupported-platform behavior. r[shared.bounded_input.file]
- [ ] [serial] Implement streaming decompression with separate compressed and expanded limits plus no-success-on-partial-output behavior. r[shared.bounded_input.decompression]

## Phase 3: ChaosControl migration

- [ ] [parallel] Migrate evidence and SDK JSON admission and serialization to the shared crates. r[shared.bounded_input.migration]
- [ ] [parallel] Migrate explore JSON and snapshot decompression paths to the shared crates. r[shared.bounded_input.migration]
- [ ] [serial] Keep path authorization, tree walking, schema policy, artifact trust, and evidence decisions in ChaosControl adapters. r[shared.bounded_input.claim_boundary]
- [ ] [serial] Compare accepted inputs, typed failures, and exact boundaries before deleting duplicate local implementations. r[shared.bounded_input.migration] r[shared.bounded_input.validation]

## Phase 4: Publication checks

- [ ] [parallel] Add malformed UTF-8, symlink, non-regular file, truncation, growth, deep JSON, excessive node, oversized string, writer overflow, codec error, and expansion-bomb tests. r[shared.bounded_input.validation]
- [ ] [serial] Run shared repository checks, focused ChaosControl tests, workspace checks, dependency policy, and Cairn gates before sync or archive. r[shared.bounded_input.validation]
