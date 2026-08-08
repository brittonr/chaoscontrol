## Why

ChaosControl's deterministic initrd builder performs sorted tree collection, file-kind classification, symlink reads, mode selection, duplicate detection, and bounded archive output.

The shared repository has completed its establishment change. This change adopts its immutable Radicle revision for product-neutral tree observation and source revalidation.

## What Changes

- Pin one reviewed `bounded-tree` Radicle revision without a sibling path or fallback.
- Replace local tree collection mechanics with shared bounded observations and member facts.
- Keep Newc archive encoding, path mapping, inode assignment, mode normalization, output limits, and kernel-bundle evidence in ChaosControl.
- Compare deterministic archive bytes and all maintained rejection cases before removing local collection code.
- Retain a rollback to the current implementation and dependency state.

## Impact

- **Files**: `crates/chaoscontrol-evidence/src/kernel_bundle_initrd.rs`, dependency declarations, initrd fixtures, lifecycle artifacts, and adoption documentation.
- **Testing**: deterministic valid archives, internal links, escaping links, path bounds, output bounds, strict Clippy, Octet, Cargo, Cairn, and focused kernel-bundle tests.
- **Boundary**: ChaosControl retains archive and VMM semantics. `bounded-tree` does not prove boot or replay correctness.
- **Current effect**: production tree traversal is replaced by capability-based Bounded Tree observation and revalidated staging.
