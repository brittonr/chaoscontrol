## Why

`dogfood-results/` contains hundreds of megabytes of tracked snapshots and repeated historical cohorts. Most retained workload proofs are diagnostic-only, so Git carries large blobs without current promotion value.

## What Changes

- Define a content-addressed object reference for large dogfood artifacts.
- Keep small manifests, receipts, summaries, and claim facts in Git.
- Materialize required blobs through explicit storage adapters and validate exact bytes before use.
- Migrate retained proof references without changing historical claim meaning.
- Add repository size and duplicate-retention gates.

## Impact

- **Code**: artifact reference core, materializer shell, evidence validators, and migration tools.
- **Configuration**: Nickel retention and storage-adapter policy.
- **Repository**: large snapshots and raw debug outputs leave normal Git history after safe migration.
- **Testing**: exact materialization plus missing, corrupt, truncated, wrong-size, unsafe-path, duplicate, and deletion-blocked cases.

## Non-Goals

- No claim that object storage is durable, trusted, or always available.
- No deletion before every live manifest validates against materialized bytes.
- No rewrite of historical evidence meaning.
