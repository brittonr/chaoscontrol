## Why

ChaosControl's deterministic initrd builder performs sorted tree collection, file-kind classification, symlink reads, mode selection, duplicate detection, and bounded archive output.

This change records a future use of `bounded-tree` for product-neutral tree observation. It remains unstarted until the shared repository publishes a complete immutable Radicle revision.

## What Changes

- Pin one reviewed `bounded-tree` Radicle revision without a sibling path or fallback.
- Replace local tree collection mechanics with shared bounded observations and member facts.
- Keep Newc archive encoding, path mapping, inode assignment, mode normalization, output limits, and kernel-bundle evidence in ChaosControl.
- Compare deterministic archive bytes and all maintained rejection cases before removing local collection code.
- Retain a rollback to the current implementation and dependency state.

## Impact

- **Planned files**: `crates/chaoscontrol-evidence/src/kernel_bundle_initrd.rs`, dependency declarations, initrd fixtures, and evidence.
- **Testing**: byte-identical valid archives, links, duplicates, special files, size bounds, source changes, Octet, Cargo, Cairn, and focused kernel-bundle gates.
- **Boundary**: ChaosControl retains archive and VMM semantics. `bounded-tree` does not prove boot or replay correctness.
- **Current effect**: lifecycle planning only. No ChaosControl implementation changes are included.
