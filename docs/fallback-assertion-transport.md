# Fallback assertion transport

The fallback transport lets a guest process emit assertions and lifecycle events without the Rust SDK.

It does not provide code coverage. It does not make a process-local fact apply to the complete guest.

## Record format

Each sink entry is one JSON line with schema version `1`.

```json
{
  "schema_version": 1,
  "sequence": 0,
  "process": {
    "guest": "guest-a",
    "process": "wal-worker"
  },
  "namespace": "org.example.store",
  "logical_key": "wal-reset-safe",
  "record_type": "always",
  "condition": true,
  "message": "WAL reset preserves committed state",
  "details": {
    "phase": "checkpoint"
  }
}
```

`record_type` is `always`, `sometimes`, `reachable`, `unreachable`, or `lifecycle`.

`always` and `sometimes` require `condition`. Other record types reject that field.

The guest and process identifiers are mandatory. The logical key is stable within its namespace.

## Deterministic ingestion

The sink admits each sequence number once and in order. The first record has sequence `0`.

The sink has an explicit record limit. When a process reaches the limit, the sink records one typed overflow event. It keeps the accepted prefix unchanged.

A BLAKE3 sink identity binds the limit, accepted record order, record bytes, and overflow event. Replay rejects reordered or modified evidence.

## Catalog admission

Assertion records derive normal ChaosControl assertion descriptors. The descriptor keeps the namespace and stable logical key.

The descriptor guest field contains both the guest and process identity. Its category is `fallback-process`.

Catalog admission compares fallback descriptors with SDK descriptors. A logical-key conflict reports the candidate fingerprint, the existing fingerprint, the process, and the conflict class.

The oracle activates the combined catalog before a run. It then ingests a validated sink as one transactional update.

## Bug and replay evidence

A fallback failure carries `fallback_scope` in the bug report and replay verdict. The scope binds:

- guest and process identity;
- sink sequence;
- BLAKE3 record identity;
- BLAKE3 sink identity;
- assertion fingerprint.

Validation rejects a fallback assertion without this scope. It also rejects fallback scope on a normal SDK assertion.

This evidence identifies the process that emitted the fact. It does not prove a whole-guest, whole-service, or code-coverage claim.
