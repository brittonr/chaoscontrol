## Why

Automatic SDK assertion IDs are 32-bit FNV-1a hashes, and explicit-ID macros accept the same unnamespaced `u32`. The SDK catalog carries message, kind, source file/line, guest, and category, but the host oracle keys only by that integer, drops source metadata, and uses `entry(id).or_insert_with(...)` for catalog registration and runtime events. Multi-VM report merging and local JSONL reporting also merge by ID without proving descriptor equality. A hash collision, reused explicit ID, or cross-guest overlap can silently combine different properties, counts, verdicts, and evidence under one assertion.

Assertion evidence must bind runtime hits to a validated catalog descriptor and reject ambiguity before a run can produce an accepted report.

## What Changes

- Define a versioned structured assertion logical key and complete canonical descriptor containing namespace, kind, message, source metadata, guest, and category.
- Use BLAKE3 over canonical descriptor bytes as the compact wire/report fingerprint while retaining canonical bytes for collision and conflict checks.
- Validate a complete catalog before runtime events. Exact duplicate registrations are idempotent. A logical-key conflict, fingerprint collision, malformed descriptor, or `LegacyU32` descriptor rejects strict admission.
- Bind every runtime assertion event to a validated catalog fingerprint/token and reject unknown or mismatched events instead of auto-creating oracle records.
- Merge per-VM and local reports only for exact descriptor matches; keep distinct namespaces separate and surface conflicts.
- Remove public `u32` assertion APIs, compatibility wire aliases, unbound guidance state, and the live oracle integer-record path. Keep only bounded historical-input parsing that rejects or quarantines legacy identity.
- Reject demoted merge sources, unsafe compatibility-selector reports, forged active-run snapshots, setup-linkage mismatches, and incomplete catalogs at run boundaries.
- Add positive idempotence/aggregation tests and negative metadata-conflict, forced-digest-collision, unknown-event, cross-guest, local-report, and legacy tests.

## Impact

- **Files**: `chaoscontrol-protocol`, SDK assertion catalog/macros/transport, fault oracle and snapshots, controller report merge, local evidence reporting, report schemas/contracts, and assertion tests.
- **Compatibility**: old integer-ID source APIs and wire forms are unsupported. Historical serialized input can be identified for bounded diagnostics, but it cannot satisfy strict evidence gates.
- **Usability**: automatic source identities remain convenient but are build-scoped; callers needing continuity across source movement use an explicit stable logical key and namespace.
- **Ownership**: runtime catalogs/events/reports remain Rust-owned; Nickel validates compact review-boundary schemas where those reports enter readiness evidence.
- **Scope boundary**: this package does not own replay artifact references, path validation, or general replay DTO extraction.
- **Claims**: BLAKE3 is a compact fingerprint, not a uniqueness proof; canonical descriptor comparison and conflict rejection are what prevent silent merging.
