## Phase 1: Admit the shared dependency

- [x] [serial] Confirm that `bounded-tree` has archived its establishment change and published a passing immutable Radicle revision. r[chaoscontrol.bounded_tree_adoption.prerequisite]
- [x] [serial] Pin that Radicle revision without a sibling path, mutable branch, or fallback. r[chaoscontrol.bounded_tree_adoption.prerequisite]
- [x] [parallel] Record the pre-adoption ChaosControl revision, dependency state, and rollback command. r[chaoscontrol.bounded_tree_adoption.rollback]

## Phase 2: Adapt initrd tree collection

- [x] [serial] Replace local source-tree collection with shared bounded observations and member facts. r[chaoscontrol.bounded_tree_adoption.tree_observation]
- [x] [parallel] Keep archive path mapping, Newc encoding, modes, inode assignment, padding, duplicate policy, and output limits local. r[chaoscontrol.bounded_tree_adoption.archive_boundary]
- [x] [parallel] Keep kernel-bundle, boot, replay, and readiness evidence semantics local. r[chaoscontrol.bounded_tree_adoption.evidence_boundary]

## Phase 3: Prove parity and cut over

- [x] [parallel] Compare complete Newc archive bytes, entry order, paths, modes, and member payloads for valid fixtures. r[chaoscontrol.bounded_tree_adoption.parity]
- [x] [parallel] Compare invalid paths, links, duplicates, special files, source changes, entry bounds, and output-byte failures. r[chaoscontrol.bounded_tree_adoption.parity]
- [x] [serial] Remove only duplicated observation mechanics after positive and negative parity passes. r[chaoscontrol.bounded_tree_adoption.parity]
- [x] [serial] Run focused Cargo, Octet, Cairn, and kernel-bundle validation before completing adoption. r[chaoscontrol.bounded_tree_adoption.evidence_boundary]
