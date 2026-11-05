## Why

Antithesis Rust SDK 0.3.0 accepts arbitrary `Serialize` assertion details, including JSON null, scalars and arrays. ChaosControl exposes that surface but its bound transport silently discards every non-object value; strict local evidence also rejects such records. Valid assertions can therefore disappear or fail report admission solely because of their diagnostic payload shape.

## Change

Preserve every bounded JSON value through unscoped bound assertion emission and strict/legacy diagnostic report admission. Keep guest/category/adoption metadata extraction limited to top-level objects; accepted catalog identity and verdicts remain authoritative. Preserve the existing object requirement for process-scoped transport, since process ownership is carried in object metadata and must not be silently lost or invented.

## Consumer and ownership

Current consumer: Rust workloads using `Serialize` scalar/sequence/optional details and their local report tooling. Adoption: update SDK/readers; no caller or wire/schema migration. Owner: ChaosControl SDK/evidence maintainers. Immediate outcome: valid non-object assertion observations survive SDK-to-report quality checks. Durable contribution: shape-independent bounded assertion diagnostics with positive and adversarial regressions. No new process-scoped representation, KVM campaign, replay, hosted-platform equivalence or release claim.
