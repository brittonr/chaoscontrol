## Phase 0: Prerequisites and inventory

- [ ] [serial] Complete the unified AGPL license boundary before shared publication and adoption. [depends:adopt-unified-agpl-license]
- [ ] [serial] Establish the accepted deterministic simulation repository and package compatibility policy. [depends:establish-deterministic-simulation-core]
- [ ] [serial] Complete full VM snapshot state before changing block snapshot ownership. [depends:complete-vm-snapshot-state]
- [ ] [serial] Complete explicit fault application outcomes before adapting storage faults. [depends:verify-fault-application-outcomes]
- [ ] [serial] Inventory block geometry, layers, read and write behavior, fault parameters, snapshots, file loading, virtio adapters, and evidence consumers. r[shared.deterministic_block.migration]

## Phase 1: Shared block core

- [ ] [serial] Add the independent `deterministic-block` AGPL package to the shared repository. r[shared.deterministic_block.repository]
- [ ] [serial] Define checked caller-owned geometry, capacity, transfer, dirty-page, and allocation limits. r[shared.deterministic_block.geometry]
- [ ] [parallel] Implement pure read, write, flush, reset, and fault planning with checked ranges and typed failures. r[shared.deterministic_block.planning]
- [ ] [serial] Implement the bounded in-memory base, durable overlay, volatile overlay, and one-commit application shell. r[shared.deterministic_block.layers]
- [ ] [parallel] Implement versioned complete snapshots and pure restore preflight. r[shared.deterministic_block.snapshot]

## Phase 2: Fault and I/O boundaries

- [ ] [parallel] Define explicit read failure, write failure, torn extent, and corruption plans without internal entropy or schedule selection. r[shared.deterministic_block.faults]
- [ ] [serial] Keep disk-image opening, bounded reads, decompression, mapping, and artifact lookup outside the block crate. r[shared.deterministic_block.shell_boundary]
- [ ] [serial] Keep virtio transport, guest memory, interrupt behavior, fault policy, and evidence decisions in ChaosControl. r[shared.deterministic_block.chaoscontrol_boundary]

## Phase 3: Migration and checks

- [ ] [parallel] Compare exact bytes and layer state for valid read, write, flush, crash, snapshot, and restore fixtures. r[shared.deterministic_block.migration]
- [ ] [parallel] Compare read error, write error, torn write, corruption, invalid range, overflow, geometry, capacity, and allocation failures. r[shared.deterministic_block.validation]
- [ ] [serial] Migrate the ChaosControl block backend only after parity and complete snapshot checks pass. r[shared.deterministic_block.migration]
- [ ] [serial] Run shared checks, focused block and virtio tests, snapshot continuation checks, workspace checks, dependency policy, and Cairn gates before sync or archive. r[shared.deterministic_block.validation]
