## Context

Recent replay hardening changed bug evidence so the in-memory `BugReport.snapshot` represents the parent snapshot used to start the failing branch. The next problem is persistence: a standalone replay process needs to locate and validate that snapshot after the original exploration process exits.

ChaosControl already commits concise JSON receipts and bug records while excluding raw runtime logs. That boundary should stay inspectable: public evidence should name stable artifact references and hashes, while snapshot bytes remain Rust-owned runtime data validated by contracts and tests.

The repository also contains `chaoscontrol-redb-guest`, a guest workload that uses redb for crash-consistency testing. Any host redb use must be named and isolated so reviewers do not confuse the guest database workload with the host evidence artifact store.

## Goals / Non-Goals

**Goals:**
- Persist parent snapshots needed by standalone replay without embedding large blobs in `bug_*.json`.
- Keep bug/receipt JSON transparent and hash-bound.
- Provide a baseline file-backed content-addressed store.
- Allow an optional host-side redb-backed store/index for transactional lookup and retention bookkeeping.
- Validate all public references with Nickel/Rust contract gates.

**Non-Goals:**
- redb is not the public evidence format.
- redb is not required for minimal replay correctness.
- Raw logs, secrets, cryptographic internals, and guest workload databases remain outside the evidence contract boundary.
- This change does not redefine snapshot semantics or VM restore behavior; it persists existing Rust-owned `SimulationSnapshot` values.

## Decisions

### 1. Public evidence uses hash-addressed snapshot references

**Choice:** Bug and receipt records name a `replay_parent_snapshot_ref` containing store kind, content digest, codec/schema version, and artifact path or logical key.

**Rationale:** Reviewers can inspect JSON evidence and verify hashes without parsing a database. This preserves the current receipt pattern and avoids treating a redb file as the only source of truth.

**Alternative:** Embed serialized snapshots directly in bug JSON. Rejected because snapshots can be large, noisy, and hard to diff or review.

**Implementation:** `SerializableBug`/receipt materialization emits a snapshot reference when parent context is required. Contract checkers verify the referenced artifact exists, decodes through Rust-owned snapshot serialization, and matches the digest.

### 2. Snapshot bytes remain Rust-owned runtime artifacts

**Choice:** Snapshot serialization/deserialization is implemented in Rust with explicit codec and schema version metadata.

**Rationale:** `SimulationSnapshot` is semantic runtime state, not human-authored configuration. Nickel should validate references and public shapes, not define VM memory/network/fault-engine internals.

**Alternative:** Model full snapshot internals in Nickel contracts. Rejected because it would create a competing source of truth and make large binary/runtime state unsuitable for contract review.

**Implementation:** The evidence registry classifies replay snapshot artifacts as `rust-derived`; Nickel contracts validate reference envelopes, digests, and receipt linkage.

### 3. File-backed store is the baseline; redb is optional host indexing

**Choice:** Implement a `SnapshotStore` trait with a deterministic file-backed store as the required baseline and an optional host-side redb store/index when transactional retention or lookup is needed.

**Rationale:** File-backed content-addressed artifacts are easy to inspect, copy, and validate in dogfood result directories. redb is useful for local durability, atomic updates, and GC metadata, but should not be mandatory to understand evidence.

**Alternative:** Make redb the only store. Rejected because it would obscure public evidence and increase the minimum replay/debug dependency surface.

**Implementation:** The baseline layout is `dogfood-results/<run>/snapshots/<sha256>.snapshot` plus JSON refs. A redb-backed implementation may store blobs or index file blobs by digest, but exported receipts remain JSON and hash-addressed.

### 4. Replay fails early on missing or invalid parent context

**Choice:** Standalone replay detects `replay_context = schedule-only-replay-insufficient` or nonzero `replay_parent_depth` and requires a valid parent snapshot ref before running VMs.

**Rationale:** A late “assertion did not fail” is ambiguous; a missing deterministic parent context should be reported as a missing artifact, not as a failed bug reproduction.

**Alternative:** Fall back silently to schedule-only replay. Rejected because it repeats the known gap and weakens evidence quality.

**Implementation:** Replay CLI loads the snapshot ref before executing the schedule. Missing, corrupt, wrong-hash, unsupported-codec, or incompatible-schema references produce distinct diagnostics and nonzero exits.

## Risks / Trade-offs

**Snapshot size and retention** → Add explicit output policy and GC tasks; keep raw logs excluded and snapshot artifacts content-addressed.

**Codec drift** → Version snapshot codecs and add Rust round-trip tests plus negative fixtures.

**redb naming confusion** → Use names such as `host-snapshot-store-redb` in docs/code and keep the redb guest workload spec untouched.

**Over-contracting runtime state** → Contracts validate references and hashes only; Rust remains the semantic owner of snapshot bytes.
