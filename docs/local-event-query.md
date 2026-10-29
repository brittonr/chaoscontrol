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

## Projecting JSON log text

`Map` projects scalar fields from canonical `details`. Use `MapText` when the
retained event's `text` is JSON instead, as in the
[Antithesis JSON log example](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/event_sets/).
For example, select `storage` records from JSON log lines:

```json
{
  "Matches": {
    "input": {
      "MapText": {
        "input": "All",
        "field": "component",
        "pointer": "/component"
      }
    },
    "field": "component",
    "text": "storage"
  }
}
```

An example event text is `{"component":"storage","used":80}`. The selected
scalar is added to `derived.component`; original text, details, source identity
and payload digest stay unchanged. The field is usable by existing text query
operators and predicates. Canonical field names still take precedence over
same-named derived values.

Strings, numbers and booleans become derived strings, matching `Map`. This is
not a typed numeric projection: `JsonNumber` still reads canonical details, not
derived strings. Missing pointers, null, objects and arrays in valid JSON leave
the derived map unchanged, including any existing value at the target field.
Pointers use the existing confined, nonempty JSON-pointer rules and `~0`/`~1`
escaping; field and pointer names are each at most 256 UTF-8 bytes.

Every candidate row must contain valid JSON text. Malformed JSON, trailing
garbage and excessive parser nesting fail with `InvalidJsonText`; no row is
silently dropped and the CLI publishes no partial result/receipt on evaluation
failure. Filter mixed logs to a JSON-producing process/source **before** MapText.
Parsing uses the existing 8192-byte canonical text cap and the JSON parser's
default recursion bound, with one logical projection operation per candidate row
in addition to input work. Existing derived-field, result-byte, node/depth,
moment and receipt bounds remain effective. `Map` is unchanged; new readers are
required to consume the `MapText` variant. No arbitrary transformations, live
branching or full Notebook compatibility are implied.


## Numeric JSON filters

`Filter` accepts a `JsonNumber` predicate over a JSON pointer into the original
event `details`. For example, save this as `query.json` to select memory usage
strictly greater than 50:

```json
{
  "Filter": {
    "input": "All",
    "predicate": {
      "JsonNumber": {
        "pointer": "/memory/used",
        "comparison": "GreaterThan",
        "value": 50
      }
    }
  }
}
```

Run `query-events query --events events.json query.json result.json --receipt
receipt.json`, then `query-events validate --events events.json query.json
result.json receipt.json`. Each event in `events.json` must be a canonical
envelope; an example matching details value is `{"memory":{"used":50.25}}`.

Comparisons are `LessThan`, `LessThanOrEqual`, `Equal`, `GreaterThanOrEqual`
and `GreaterThan`. Both operands use finite IEEE-754 binary64, like JavaScript
Number: integers outside its exact range (beyond ±2^53) may round. This is not
an exact arbitrary-width integer comparison. The threshold must be a JSON number,
not a string or null. Missing paths and non-number values never match; no string,
boolean or null coercion occurs. Pointers follow the existing nonempty `/`-prefixed,
control-free, 256-byte bound and JSON-pointer `~0`/`~1` escaping.

Filtering preserves original fields and canonical ordering. It uses the existing
per-row filter operation charge, scan/row/byte limits and receipt binding; a
threshold change invalidates a receipt even if it selects the same rows. Text
operators retain their existing string semantics. Old queries and receipt schemas
are unchanged; readers must upgrade before consuming the new predicate variant.

This closes the bounded local threshold-filtering gap illustrated by the
[Antithesis event-set memory example](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/event_sets/).
It does not add arbitrary JavaScript predicates, live branches, reactive sequences
or hosted Notebook compatibility.

## Compound predicates

`Filter.predicate` supports `All`, `Any` and `Not` around any existing predicate.
For example, select high-memory observations in VM 1 or 2, excluding maintenance
processes:

```json
{
  "Filter": {
    "input": "All",
    "predicate": {
      "All": [
        {"Any": [{"Vm": 1}, {"Vm": 2}]},
        {"JsonNumber": {"pointer": "/used", "comparison": "GreaterThan", "value": 50}},
        {"Not": {"Process": "maintenance"}}
      ]
    }
  }
}
```

Use the same `query` and `validate` commands above. Children evaluate in their
declared order: `All` stops on false, `Any` stops on true, and `Not` inverts its
child. Empty `All` matches every candidate event; empty `Any` matches none.
Negation inverts the leaf's complete result: `Not(JsonNumber(...))` also matches
missing or non-numeric values because the numeric predicate does not match them.
It is not an implicit field-presence or numeric-type guard.

Admission validates every descendant, even a branch that evaluation would skip
or a query over an empty source. Predicate descendants share the existing
`max_nodes` counter and `max_depth` with the query expression. A Filter accounts
for its predicate root, preserving the historical cost of atomic predicates;
each child adds one node and a depth step. The existing per-row Filter charge
covers its root check, and each visited child costs one additional operation.
Skipped children cost none. Budget exhaustion returns an error, never partial
rows. No new limit fields are required.

Canonical fields/order, atomic predicate semantics and receipt binding are
unchanged. A different expression invalidates a receipt even when it selects
the same rows. New variants require upgraded readers; existing flat queries
retain their admission and operation costs. This is a bounded boolean subset
of the upstream guide's arbitrary filter functions, not JavaScript execution
or full Notebook compatibility.

## Composable text selection

`Contains` and `Matches` are available as predicates inside `Filter`, including
within `All`, `Any` and `Not`. For example, select warnings or errors while
excluding heartbeat noise:

```json
{
  "Filter": {
    "input": "All",
    "predicate": {
      "All": [
        {"Any": [
          {"Contains": {"field": "text", "text": "[WARN]"}},
          {"Contains": {"field": "text", "text": "[ERROR]"}}
        ]},
        {"Not": {"Contains": {"field": "text", "text": "heartbeat"}}}
      ]
    }
  }
}
```

Contains is a literal, case-sensitive substring search. Matches is exact
case-sensitive string equality, **not** a regular expression. Unicode text is
supported without case folding or normalization. The field names `text`, `kind`,
`process` and `source_kind` select canonical values; other names select derived
strings, including fields produced by `Map`. Derived names cannot shadow those
canonical values. Missing fields never match a positive selector, so their
negation does match; add a positive condition if missing data should be excluded.

Both selectors reuse the existing nonempty, control-free field/needle rules:
fields are at most 256 UTF-8 bytes and needles at most 8192 bytes. Every leaf is
validated before evaluation, even when short-circuiting would skip it. Existing
node/depth/operation bounds and receipt binding apply. Changing a needle
invalidates its receipt even when selected rows happen to stay identical.

Standalone `EventQuery::Contains` and `EventQuery::Matches` remain supported
with their existing behavior. New predicate variants require upgraded readers;
schemas and existing query forms are unchanged. This supports a bounded subset
of the [Antithesis text-filter examples](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/event_sets/),
not arbitrary functions, regex evaluation or full Notebook compatibility.




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
