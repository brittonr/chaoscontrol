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
- Different source-visible content at one `(source_id, source_sequence)` is
  rejected. Repeated source positions collapse under existing deduplication,
  retaining the first candidate's query-local projections.
- Directly combined `Union` or `Temporal` children must not span more than one
  `UpTo` moment.

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

## Numeric JSON stdout filters

`JsonTextNumber` uses the same pointer, comparison and numeric threshold as
`JsonNumber`, but reads JSON from original event `text` rather than `details`.
It does not coerce `MapText`'s derived strings into numbers. For example, select
Chromium observations above 50 from a JSON-emitting memory monitor:

```json
{
  "Filter": {
    "input": {
      "MapText": {
        "input": {
          "Filter": {"input": "All", "predicate": {"Process": "memory-monitor"}}
        },
        "field": "app",
        "pointer": "/process_name"
      }
    },
    "predicate": {
      "All": [
        {"Matches": {"field": "app", "text": "chromium"}},
        {"JsonTextNumber": {
          "pointer": "/mem_usage", "comparison": "GreaterThan", "value": 50
        }}
      ]
    }
  }
}
```

Example text: `{"process_name":"chromium","mem_usage":80}`. Save the query
and run the same `query`/`validate` commands above. Wrap it in `UpTo` to select
only retained history up to a tick. Conflicting `details.mem_usage` or derived
strings do not affect this predicate; canonical fields remain unchanged.

The five comparisons use the same finite binary64 semantics, rounding and
256-byte confined pointer rules as `JsonNumber`. Missing pointers and selected
strings, booleans, null, arrays or objects are false, with ordinary `Not`
negation. Only visited leaves parse text. Invalid JSON, trailing garbage,
out-of-range JSON numbers and excessive parser nesting fail with
`InvalidJsonText`, not a non-match. Filter mixed plain-text logs before parsing,
or put a source guard first in an `All`; an `Any` may also short-circuit parsing.
Skipped predicate descendants still undergo pointer/threshold admission.

Each parse is bounded by the existing 8192-byte canonical text cap and default
parser recursion limit. Visited numeric leaves use the existing logical filter
operation charge; compound predicates charge visited children as before. No
parsed-tree cache or new limits are introduced. Evaluation failures publish no
partial CLI result/receipt. Existing node/depth, scan/result, moment and receipt
rules remain effective; even a same-output threshold or pointer change invalidates
a receipt. `JsonNumber`, `Map` and `MapText` retain their existing semantics.
New predicate variants require upgraded readers; schemas are unchanged.

This supports the retained-history portion of the
[Antithesis memory-monitor example](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/event_sets/).
It does not implement arbitrary transforms, numeric coercion of derived strings,
reactive branches, complete capture or full Notebook compatibility.


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

## Structured field presence and absence

`JsonPresent` tests whether a confined JSON pointer resolves in original
`details`; `JsonTextPresent` tests the same condition in JSON parsed from
original event `text`. Both take `{"pointer":"/path/to/field"}`. Neither reads
derived strings. A resolved value is present even when it is null, false, zero,
an empty string, an array or an object. Missing/untraversable paths are absent;
missing details are absent for `JsonPresent`. Use `Not` for absence:

```json
{
  "Filter": {
    "input": "All",
    "predicate": {"Not": {"JsonTextPresent": {"pointer": "/memory/used"}}}
  }
}
```

Use the same `query`/`validate` commands above. To exclude missing values from
a negative numeric query, compose an explicit presence guard:

```json
{
  "Filter": {
    "input": "All",
    "predicate": {"All": [
      {"JsonTextPresent": {"pointer": "/memory/used"}},
      {"Not": {"JsonTextNumber": {
        "pointer": "/memory/used", "comparison": "GreaterThan", "value": 50
      }}}
    ]}
  }
}
```

This excludes absent metrics but includes explicit null and other nonnumeric
values, because presence is **not** a numeric-type guard. For numeric values at
most 50, use `JsonTextNumber` with `LessThanOrEqual` instead. Replace the text
variants with `JsonPresent`/`JsonNumber` for canonical details. No truthiness,
string/number conversion or upstream boolean/numeric search coercion is added.

Presence leaves reuse the existing 256-byte confined pointer rules, including
JSON-pointer escaping and array paths. `JsonPresent` does not parse event text;
generic canonical text admission still applies. Visited `JsonTextPresent`
leaves strictly parse bounded text just like `MapText` and `JsonTextNumber`:
invalid/trailing/over-deep JSON fails with `InvalidJsonText`, even under `Not`.
All/Any short-circuiting can skip parsing, but every descendant's pointer is
validated before evaluation, even over an empty source. A visited presence leaf
uses the existing logical filter charge; no new limits or parsed cache are added.

Canonical rows, ordering, node/depth/text/operation bounds, moments and receipt
binding remain unchanged. The source variant and pointer are receipt-bound even
when another query selects identical rows. Evaluation failures produce no
partial CLI result/receipt. New predicate variants require upgraded readers;
schemas and existing predicate semantics are unchanged.

This supplies bounded local presence/absence selection analogous to the custom
field operators in [Antithesis Logs Explorer search](https://antithesis.com/docs/reference/logs_explorer_search/),
not its query syntax, type coercions, hosted interface, temporal search, complete
event capture or full compatibility.


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
not arbitrary functions or full Notebook compatibility. For regex search, use
the separate `Regex` predicate below.

## Bounded regular-expression search

`Regex` searches a canonical or derived string field with a Rust regular
expression. For example, select error codes while excluding heartbeat logs:

```json
{
  "Filter": {
    "input": "All",
    "predicate": {"All": [
      {"Regex": {"field": "text", "pattern": "ERR[0-9]+"}},
      {"Not": {"Regex": {"field": "text", "pattern": "heartbeat"}}}
    ]}
  }
}
```

Run the same `query`/`validate` commands above. Search is case-sensitive and
unanchored by default: `ERR[0-9]+` matches `prefix ERR42 timeout`. Use `^`/`$`
for whole-field matching. Unicode classes, alternation and Rust inline flags
such as `(?i)` are supported. Backreferences and look-around are not; this is
not JavaScript/PCRE syntax compatibility. JSON strings must escape backslashes:
the pattern `\d+` is written as `"\\d+"` in a query file.

Field resolution is identical to Contains/Matches: `text`, `kind`, `process`
and `source_kind` select canonical values; other names select derived strings,
including Map/MapText projections. Derived names cannot shadow canonical names.
Missing fields never match even a zero-width regex; an existing empty derived
string can match `^$`. `Not` inverts the complete result, including a missing
field's non-match. This deliberately retains existing predicate semantics, not
the upstream Logs Explorer negative-operator rule that ignores missing fields.
Contains remains literal substring search; Matches remains exact equality.

Admission validates all descendants and compiles each distinct pattern once per
admission, including skipped branches and queries over empty sources. A
query-local cache borrows pattern strings and reuses compiled programs across
rows, fields and branches; there is no persistent/global cache. Receipt
construction independently admits the query. Queries without regex allocate no
regex cache entries.

Fields retain the 256-byte bound; patterns are nonempty, control-free and at
most 8192 UTF-8 bytes. Each compiled pattern has a 256 KiB program-size limit,
a 256 KiB lazy-DFA cache limit and syntax nesting depth 64. Existing query-node
admission bounds the number of distinct programs. Invalid syntax, unsupported
constructs and compilation expansion/nesting failures return `InvalidRegex`;
field/pattern byte failures retain `InvalidField`/`InvalidText`. These engine
limits are not total process RSS or a wall-clock deadline. Logical operation
charges cover visited predicates, not regex engine instructions; searches also
remain bounded by admitted string sizes. Evaluation failures publish no partial
CLI result or receipt.

Canonical rows/order, structural/work/result bounds, moments and receipt binding
remain effective. Pattern and field changes invalidate receipts even when rows
stay identical. New readers are required for the variant; existing schemas and
queries stay unchanged. This supplies bounded local regex selection analogous
to [Antithesis Logs Explorer search](https://antithesis.com/docs/reference/logs_explorer_search/),
not a hosted search interface, arbitrary code, complete capture, live branching
or full Notebook compatibility. Combine it with `Temporal` below for retained
tick correlation.





## Temporal witness correlation

`Temporal` selects candidate events by the existence or absence of independently
queried witnesses. For example, select errors within five simulation ticks of
an earlier restart, using the caller's `error` and `restart` kind labels:

```json
{
  "Temporal": {
    "input": {"Filter": {"input": "All", "predicate": {"Kind": "error"}}},
    "witness": {"Filter": {"input": "All", "predicate": {"Kind": "restart"}}},
    "relation": "PrecededBy",
    "within_ticks": 5
  }
}
```

Run `query` and `validate` with the same commands above. For candidate tick `c`,
witness tick `w`, and nonnegative `within_ticks = d`:

| Relation | Candidate is selected when |
| --- | --- |
| `PrecededBy` | Some witness satisfies `c - d <= w < c` |
| `NotPrecededBy` | No witness satisfies `c - d <= w < c` |
| `FollowedBy` | Some witness satisfies `c < w <= c + d` |
| `NotFollowedBy` | No witness satisfies `c < w <= c + d` |

Endpoints saturate safely at zero and `u64::MAX`. Same-tick events do not
establish ordering, even if their source positions differ. A zero-width window
has no positive matches; negative forms retain every candidate. An empty
witness set has the same existence behavior. Candidate rows and projections
are unchanged; witness rows and their projections are not emitted or merged.
Candidate order is preserved before existing canonical sorting and first-wins
duplicate removal, including conflicting query-local projections of a repeat.

Both children can use existing filters, projections, regexes, windows, unions
and nested temporal expressions. Both are evaluated and admitted even when
the candidate set is empty or the window is zero. Invalid witness projections,
malformed JSON, skipped invalid regexes and exhausted bounds remain errors,
not evidence of witness absence. Evaluation failures emit no partial CLI result
or receipt. Receipts bind both expressions, the relation/window and all supplied
source events, including witnesses that never select a candidate.

Witnesses are sorted by tick in place; each candidate binary-searches its
nearest strict witness without a Cartesian join, extra tick arrays or copied
candidate rows during correlation. In addition to both child evaluations,
charge one logical temporal operation per candidate and per witness. Existing
node/depth/work/result bounds apply. Logical counts are not comparison counts,
total process RSS limits or a wall-clock deadline.

Directly combined differing child moments return `AmbiguousMoment`. An outer
`UpTo` determines its reported moment and caps only the rows of the expression
it wraps; it does not retroactively cap hidden witnesses. To exclude future
witnesses, wrap the `witness` expression itself in `UpTo` at the intended tick.

Callers must supply a coherent tick domain and restrict both child queries when
VM/process/source matching is required; there is no implicit partition or join
key. This is a bounded local analogue of the temporal operators in
[Antithesis Logs Explorer search](https://antithesis.com/docs/reference/logs_explorer_search/),
using simulation ticks rather than seconds. It does not infer causality, admit
branch lineage, establish complete capture, or provide live branching, hosted
search, VM/replay proof or full upstream query-language compatibility. Existing
queries and schemas are unchanged; new variants require upgraded readers.

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
