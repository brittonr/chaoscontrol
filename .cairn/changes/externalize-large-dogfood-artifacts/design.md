## Context

Chunking keeps individual Git blobs below service limits, but it does not reduce repository weight or repeated cohort retention. Proof identity and storage location need separate contracts.

## Decisions

### 1. BLAKE3 object identity is storage-neutral

Each large object reference contains a domain, BLAKE3 digest, exact byte length, media type, and logical evidence role. Storage locators are separate and cannot change identity.

### 2. Git keeps reviewable evidence facts

Git retains manifests, receipts, summaries, assertion coverage, replay classes, artifact roles, and non-claims. Snapshots, raw logs, and other large payloads use object references.

### 3. Materialization fails closed

A shell fetches or locates a candidate object into bounded staging. The pure core validates reference shape, size, digest, role, and manifest linkage before evidence code can consume it.

### 4. Storage adapters are explicit

The first adapter can use an admitted local or published content store. Each adapter names availability and transport limits. Adapter success does not prove storage durability or trust.

### 5. Migration is two-phase

First, add object references and validate them while existing blobs remain. Second, remove tracked payloads only after every live reference materializes and all readiness gates pass.

### 6. Retention is policy-driven

A typed Nickel policy selects current accepted cohorts, diagnostic exemplars, expiration classes, maximum tracked artifact size, and duplicate rules. Raw run and reproduction logs remain local unless summarized.

### 7. Historical digest fields remain readable

Legacy SHA-256 snapshot names can remain compatibility metadata. New stack-owned object identity uses BLAKE3 and typed algorithm fields.

## Risks

External objects can disappear. Missing materialization blocks promotion and replay. The repository must keep enough receipt data to report the exact missing object.
