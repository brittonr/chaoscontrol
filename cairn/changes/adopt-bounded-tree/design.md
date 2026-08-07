## Context

`kernel_bundle_initrd.rs` collects absolute trees before `NewcWriter` emits a deterministic initrd. Tree collection and archive encoding currently share one module.

The future dependency can own bounded observation and path admission. ChaosControl must keep CPIO/Newc semantics and kernel evidence.

## Decisions

### Decision: Require completed shared publication

**Choice:** Block implementation until `bounded-tree` has archived its establishment change and published a reviewed immutable Radicle revision.

**Rationale:** A deterministic evidence path cannot depend on mutable sibling source.

### Decision: Share observation, not archive encoding

**Choice:** Replace local tree collection and file-kind admission with shared observations. Keep archive path mapping, parent insertion, inode assignment, headers, padding, mode normalization, duplicate policy, and byte accounting in `NewcWriter`.

**Rationale:** Those fields define the initrd archive format and ChaosControl compatibility.

### Decision: Preserve deterministic bytes

**Choice:** Compare complete archive bytes for valid fixtures. Also compare sorted entry order and normalized metadata before accepting the cutover.

**Rationale:** Equal extracted files do not prove byte-identical initrd identity.

### Decision: Preserve negative behavior

**Choice:** Compare unsupported files, invalid paths, duplicate archive paths, oversized links, source changes, entry limits, and output-byte limits.

**Rationale:** Fail-closed archive construction is part of kernel-bundle evidence.

### Decision: Keep VMM evidence local

**Choice:** Continue to assign boot, module, BPF, replay, readiness, and cleanup meaning in ChaosControl. Record `bounded-tree` only as a source-observation dependency.

**Rationale:** A shared tree mechanism does not prove guest behavior or deterministic execution.

## Risks / Trade-offs

- Byte-identical output constrains adapter ordering and metadata translation.
- The Newc writer still owns significant format-specific tree code.
- The first shared shell supports Unix only, matching current kernel-bundle hosts.
- Radicle acquisition failures remain visible.
