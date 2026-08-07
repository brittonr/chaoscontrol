## Context

`kernel_bundle_initrd.rs` maps absolute closure roots before `NewcWriter` emits a deterministic initrd. Bounded Tree now owns bounded observation, path admission, member facts, and source revalidation. ChaosControl keeps CPIO/Newc semantics and kernel evidence.

## Decisions

### Decision: Require completed shared publication

**Choice:** Pin the completed Bounded Tree revision `b0fd0103bc9eed2c1b6d852045959462d105d8f1` from its Radicle seed transport. Do not provide a sibling path or mutable fallback.

**Rationale:** A deterministic evidence path cannot depend on mutable sibling source. The producer archive and consumer rollback facts are recorded in `docs/bounded-tree-adoption.md`.

### Decision: Share observation, not archive encoding

**Choice:** Open each directory as a capability, prepare a shared observation plan, and copy it to bounded staging with source revalidation. Encode the staged members in shared plan order. Keep archive path mapping, parent insertion, inode assignment, headers, padding, mode normalization, duplicate policy, and byte accounting in `NewcWriter`.

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
- Staging adds bounded I/O and temporary disk use before archive encoding.
- Only relative symlinks that resolve to admitted members are accepted.
