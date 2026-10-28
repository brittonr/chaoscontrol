# Local event queries and static timelines

`query-events`, built from `chaoscontrol-replay`, is a bounded local tool for
querying retained execution evidence. It performs no network, hosted, or
arbitrary-code work.

## Inputs

Pass exactly one source:

- `--events FILE`: a JSON array of canonical event envelopes.
- `--recording FILE`: a retained recording; recording events and the canonical
  fault-stage trace are adapted together.
- `--dlog FILE`: raw determinism-log bytes.
- `--markers FILE`: retained `MarkerEvidenceContext` records.

Inputs are regular files opened with `O_NOFOLLOW` and bounded to 64 MiB. A dlog
whose record count exceeds the admitted scan bound is rejected rather than
partially read.

## Envelope rules

- Recording events use one source identity; fault-stage events use a distinct
  domain-separated identity so positions cannot collide.
- Fault-stage ticks come from round deltas, or from the attempt selection tick
  for legacy traces without deltas.
- Branch markers retain their observed tick, owner, identity, and payload
  digest.
- Two different records at one `(source_id, source_sequence)` are rejected.
  Byte-identical repeats collapse only when a union produces them.
- Query unions must not span more than one `UpTo` moment.

## Commands

```bash
query-events query --recording recording.json query.json result.json \
  --receipt receipt.json

query-events validate --recording recording.json query.json result.json receipt.json

query-events timeline --format markdown facts.json timeline.md \
  --receipt timeline-receipt.json

query-events validate-timeline --format markdown facts.json timeline.md \
  timeline-receipt.json
```

## Limits

`EventQueryLimits` defaults and caps are:

| Field | Default |
| --- | ---: |
| `max_nodes` | 256 |
| `max_depth` | 64 |
| `max_scanned_events` | 1,000,000 |
| `max_result_rows` | 100,000 |
| `max_result_bytes` | 64 MiB |
| `max_operations` | 10,000,000 |

Older limits JSON without newer fields receives those defaults.

## Receipts

A query receipt binds the query, limits, source IDs, a digest of every
source-visible event field, scanned/matched/row counts, truncation, and the
result-row digest. Validation re-evaluates the query, checks canonical row
order and duplicate positions, and rejects source, result, query, counter, or
digest drift.

A timeline receipt binds the declared format, canonical fact digest, node and
byte counts, and rendered-output digest. Validation must first supply exactly
the canonical rendering.

## Non-claims

These commands describe only admitted inputs, limits, and computed rows. They
do not claim complete event capture, causal attribution, source correctness,
hosted service support, arbitrary command execution, replay promotion, or
release eligibility. Guest observation and future-of-moment restore are not
implemented here.
