# Bounded Tree adoption

ChaosControl uses Bounded Tree for deterministic initrd source-tree observation and source revalidation.

## Pinned source

- Radicle repository: `rad://zqhtZvsteJhxCJE96dMAZSZ9y1PX`
- Git transport: `https://seed.radicle.garden/zqhtZvsteJhxCJE96dMAZSZ9y1PX.git`
- Revision: `b0fd0103bc9eed2c1b6d852045959462d105d8f1`
- Producer archive: `cairn/archive/1970-01-01-establish-bounded-tree`
- ChaosControl pre-adoption revision: `095a172bf9c65c529ff47ceb08c5ebf62e583017`

`Cargo.lock` binds the two Bounded Tree crates to the exact revision. There is no sibling path or mutable branch fallback.

## Boundary

The adapter opens each closure directory as a capability. Bounded Tree observes its members under explicit limits, computes BLAKE3 member facts, and revalidates the source while it copies to bounded staging.

ChaosControl retains these rules:

- absolute source paths map to relative Newc paths;
- Newc headers, inode allocation, modes, padding, and duplicate handling stay local;
- initrd entry and output limits stay local;
- kernel-bundle, boot, module, BPF, replay, readiness, and cleanup claims stay local.

Only relative links that resolve to admitted members are accepted. Unsupported entries, escaping links, changed sources, and exceeded bounds fail before Newc encoding uses the staged tree.

A successful Bounded Tree observation does not prove boot success, guest correctness, deterministic replay, publication durability, or release eligibility.

## Rollback

If the adoption changes an accepted initrd identity or failure class, revert the complete adoption commit. Do not remove only the dependency or only the adapter.

```console
git revert ':/Revalidate initrd trees before preserving archive identity'
```

The revert must restore `Cargo.toml`, `Cargo.lock`, `kernel_bundle_initrd.rs`, tests, lifecycle artifacts, and this document together. The pre-adoption source revision is recorded above for comparison.
