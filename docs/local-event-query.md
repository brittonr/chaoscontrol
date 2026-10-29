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
- Directly combined `Union`, `DistinctByTick`, `Temporal`, `Intersect` or
  `Difference` children must not span more than one `UpTo` moment.

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


## Keeping selected projections

`Narrow` removes unlisted query-local `derived` fields after projection. For
example, keep only `component` from an enriched input:

```json
{
  "Narrow": {
    "input": {
      "MapText": {
        "input": "All",
        "field": "component",
        "pointer": "/component"
      }
    },
    "fields": ["component"]
  }
}
```

This implements a bounded local analogue of
[Antithesis narrow](https://antithesis.com/docs/reference/event-set-reference/#narrow).
Unlike upstream selection over general synthetic event fields, only the
`derived` map is restricted. All canonical fields, including text, details,
source identity, tick, owner and payload digest, remain unchanged. It is not
a payload redaction feature. Canonical names such as `kind` still take precedence
over same-named derived entries when filtering.

Names are exact and case-sensitive. Missing names are ignored, duplicates and
list order do not change selection, and an empty list clears all projections
without dropping events. Selected values remain unchanged and usable by later
filters; later `Map`/`MapText` operations may add fields again. Selection does not
change encounter order or first-wins duplicate-source precedence.

At most 256 list entries are admitted, counting duplicates; oversized lists
return `QueryBound`. Each name must be nonempty, contain no control characters,
and fit 256 UTF-8 bytes (`InvalidField` otherwise), even on empty input. Every
child row is admitted before pruning, so clearing fields cannot conceal invalid
or oversized projections or bypass child JSON parsing. Selection uses one bounded
set of borrowed field names and prunes maps in place, with one logical operation
per candidate row in addition to child work. Existing scan/node/depth/result
bounds, canonical-prefix truncation and moment handling apply.

Receipts bind the complete field list, including order, duplicates and missing
names, even if those choices yield identical rows. Source binding still includes
every canonical source-visible field, including discarded/out-of-moment events;
query-local input `derived` maps are not source identity. Result binding and
re-evaluation protect retained projections. Evaluation failures publish neither
result nor receipt. Existing queries and schemas are unchanged; upgraded readers
are required for the new variant. No JavaScript, complete capture, VM execution,
replay proof or hosted event-search claim is added.

## Counting matching prefixes

`RunningCount` annotates every selected observation with the number of matching
observations at or before it. This is a bounded local analogue of the counting
example in [Antithesis fold](https://antithesis.com/docs/reference/event-set-reference/#fold),
not an arbitrary reducer. Count severity-20 logs and then select severity-30 logs:

```json
{
  "Filter": {
    "input": {
      "RunningCount": {
        "input": "All",
        "predicate": {
          "JsonNumber": {
            "pointer": "/severity",
            "comparison": "Equal",
            "value": 20
          }
        },
        "field": "severity20_seen"
      }
    },
    "predicate": {
      "JsonNumber": {
        "pointer": "/severity",
        "comparison": "Equal",
        "value": 30
      }
    }
  }
}
```

Rows are ordered by `(tick, source_id, source_sequence)` before counting.
Repeated source positions count once, retaining their first encountered derived
projection. Tied ticks follow source ID and sequence order; they are not treated
as simultaneous. The counter starts at zero and includes the current row when
it matches. `{"All":[]}` counts all unique positions, `{"Any":[]}` counts none,
and empty input produces no rows. Nonmatching rows remain annotated, rather than
being dropped. Filters inside the input restrict what is counted; downstream
filters select observations without recomputing their prefixes.

The checked u64 count is a decimal string in the named `derived` field. Use
`FieldNumber` below for numeric thresholds, `Matches` for exact strings, or
`Narrow` to retain it; `JsonNumber` still reads canonical details, not derived
strings. Existing target values are replaced
after the predicate reads the original row. All other projections and canonical
fields remain unchanged; canonical names still take precedence in text filters.
For JSON log text, use `JsonTextNumber` instead of `JsonNumber`, or project a
field with `MapText` and use an existing text/regex predicate.

The field uses existing nonempty, control-free, 256-byte UTF-8 admission even
on empty input. Predicates reuse Filter's structural accounting, regex cache and
short-circuit semantics. Every materialized input row, including duplicates,
costs one logical operation beyond child work; visited predicate children add
their existing charges. All child rows are admitted before deduplication, and
each annotated row is admitted immediately, so downstream filtering/narrowing
cannot conceal capacity overflow. Malformed visited JSON fails; short-circuited
JSON leaves need not parse, but their pointers/patterns must still be admitted.
Sorting reuses the child vector and is skipped when already ordered. No prefix
copies or additional event collection is made.

Existing scan/node/depth/operation/result-byte bounds, canonical-prefix truncation,
moment propagation and receipt checks apply. Receipts bind the complete input,
predicate and field, even when an alternative expression yields identical rows,
plus every canonical source-visible field and the annotated results. Evaluation
errors publish neither result nor receipt. Existing schemas/queries remain
unchanged; new readers are required for this variant.

Callers must supply a coherent tick domain and scope the input to one retained
history. Counts span the selected sources/VMs without automatic partition/reset.
This ordering does not infer causality, branch lineage, complete capture or a
multiverse fold. No JavaScript, generic reducer execution, hosted search, CPU/RSS/
deadline guarantee, VM execution or replay-proof claim is added.

## Numeric thresholds on projected fields

`FieldNumber` compares a canonical/derived string as a finite JSON number. It
lets downstream queries numerically select fields produced by `Map`, `MapText`
or `RunningCount`, rather than enumerating exact strings or comparing them
lexically. For example, retain observations after at least two matching events:

```json
{
  "Filter": {
    "input": {
      "RunningCount": {
        "input": "All",
        "predicate": {"Kind": "error"},
        "field": "errors_seen"
      }
    },
    "predicate": {
      "FieldNumber": {
        "field": "errors_seen",
        "comparison": "GreaterThanOrEqual",
        "value": 2
      }
    }
  }
}
```

This supplies bounded local numeric filtering over synthetic fields, as in the
[Antithesis map/filter example](https://antithesis.com/docs/reference/event-set-reference/#map).
It does not add arithmetic or JavaScript `Number()` coercions. The six existing
relations and finite binary64 rules apply, including signed zero, rounding past
±2^53 and numeric underflow. The threshold must be a JSON number, not a string.

The entire selected field must be JSON-number text: decimal/exponent forms and
surrounding JSON whitespace are accepted. Empty/missing fields, quoted strings,
booleans, null, containers, trailing text, leading `+`, leading-zero integers,
hexadecimal, NaN, Infinity and overflowing numbers do not match **any** relation,
including `NotEqual`. `Not` around `Equal` still selects missing/nonnumeric values
as well as numeric inequalities. Invalid field text is a nonmatch, not a JSON
parse error. `JsonTextNumber` is unchanged and still rejects malformed visited
JSON documents; use it for a pointer inside canonical JSON log text.

Lookup follows existing text operators: canonical `kind`, `text`, `process` and
`source_kind` take precedence over same-named derived entries. All other names
address derived entries; this is not general envelope/JSON-pointer lookup.
An absent canonical process does not fall back to a derived process value.
Names are exact, case-sensitive, nonempty, control-free and at most 256 UTF-8
bytes. Name and finite-threshold admission applies even on empty input and in
skipped compound branches. Parsing is bounded by existing admitted canonical text
and aggregate derived-byte limits; no extra cache, source mutation or per-field
cap is introduced.

The leaf reuses existing row and compound-predicate operation charges, node/depth,
scan/result-byte bounds, canonical ordering/prefix truncation, moments and receipt
checks. Complete field/relation/threshold expressions remain receipt-bound even
when changed expressions select identical rows. Canonical source evidence and
result projections remain protected by existing receipt checks. Evaluation
failures publish neither result nor receipt. Old queries/schemas are unchanged;
upgraded readers are required for this new predicate. No exact arbitrary-width
numeric comparison, complete capture, hosted search, VM execution or replay-proof
claim is added.

## Comparing utilization ratios

`FieldRatio` compares a finite quotient of two selected numeric string fields.
It implements bounded local filtering for the
[Antithesis storage-utilization example](https://antithesis.com/docs/reference/event-set-reference/#map),
without executing a general map expression. For JSON log text containing
`{"used":"20","total":"100"}`, project the operands and select utilization
above ten percent:

```json
{
  "Filter": {
    "input": {
      "MapText": {
        "input": {
          "MapText": {
            "input": "All",
            "field": "used",
            "pointer": "/used"
          }
        },
        "field": "total",
        "pointer": "/total"
      }
    },
    "predicate": {
      "FieldRatio": {
        "numerator": "used",
        "denominator": "total",
        "comparison": "GreaterThan",
        "value": 0.1
      }
    }
  }
}
```

Use `Map` instead for canonical details. Existing projections and running counts
can also be operands. The predicate preserves complete rows; it does not emit a
new ratio field. Both names follow `FieldNumber` lookup, canonical precedence,
JSON-number grammar and nonempty/control-free/256-byte UTF-8 admission. The
threshold is a finite JSON number. All names/thresholds are admitted even in
skipped branches or empty input.

Division uses finite binary64 operands and the existing six comparison relations.
Negative operands and signed-zero quotients are valid; underflow may produce
zero, and wide integers may round. Missing/nonnumeric/nonfinite operands,
positive or negative zero denominators, and overflowing quotients never match,
including `NotEqual`. `Not` around a ratio predicate still complements the
nonmatch. This deliberately excludes JavaScript coercions and Infinity/NaN
comparisons. The denominator is checked first so undefined division need not
parse the numerator; malformed scalar text is a nonmatch, not a query error.
No cross-multiplication or exact rational comparison is performed.

Existing admitted text/derived-byte caps, row/compound-operation charges,
node/depth/scan/result bounds, canonical-prefix truncation and moment handling
apply. Receipts bind both operand names, relation and threshold even for
equivalent results, plus all canonical source facts and result projections.
Evaluation failures publish neither result nor receipt. Old queries/schemas are
unchanged; upgraded readers are required. No general arithmetic engine, ratio
projection, complete capture, hosted search, CPU/RSS/deadline guarantee, VM
execution or replay-proof claim is added.

## Retaining computed utilization

`MapRatio` adds the finite quotient as a derived JSON-number string so the
utilization can be displayed, narrowed or reused by downstream predicates. This
supplies the annotation step from the
[Antithesis map/add_fields example](https://antithesis.com/docs/reference/event-set-reference/#map)
without a JavaScript interpreter. Given `derived.used = "20"` and
`derived.total = "100"` (for example, from the preceding Map/MapText projections):

```json
{
  "Filter": {
    "input": {
      "MapRatio": {
        "input": "All",
        "field": "utilization",
        "numerator": "used",
        "denominator": "total"
      }
    },
    "predicate": {
      "FieldNumber": {
        "field": "utilization",
        "comparison": "GreaterThan",
        "value": 0.1
      }
    }
  }
}
```

This retains `derived.utilization = "0.2"` alongside the original canonical
evidence and other projections. Wrap the operand-producing query in place of
`All`; `RunningCount` projections can also be operands. `Narrow` can then keep
only utilization while preserving canonical evidence.

Arithmetic and operand lookup are shared with `FieldRatio`: complete finite
JSON-number text, denominator-first binary64 division, canonical-name precedence,
signed-zero/underflow and wide-integer rounding. Both operands are read before
the destination is changed, so output names may alias either input. Finite
results replace the destination using JSON-number text (`1.0`, `-0.0`, etc.).
For missing/nonnumeric/nonfinite operands, either zero denominator or an
overflowing quotient, the row remains but the destination projection is removed.
This deliberately differs from missing-pointer Map/MapText extraction: a stale
ratio must not survive recomputation and pass a later numeric filter. Canonical
fields are never overwritten; projecting a name such as `kind` changes only
`derived.kind`, and canonical lookup still wins.

All three names require existing nonempty/control-free/256-byte UTF-8 admission,
even with empty input. Each candidate costs one operation in addition to its
input work. Intermediate and generated envelopes must remain within existing
field-count/derived-byte capacity before downstream narrowing; replacing an
existing field does not consume a new field slot. Existing structural, scan,
serialized-result, canonical-prefix, deduplication, moment and complete-query/
source/result receipt rules apply. Evaluation failures emit neither result nor
receipt. Old variants/schemas are unchanged; new queries require upgraded readers.
This is one bounded arithmetic projection, not a general map/fold engine,
JavaScript coercion, exact rational arithmetic, complete capture, hosted search,
CPU/RSS/deadline guarantee, VM execution or replay proof.

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

Comparisons are `LessThan`, `LessThanOrEqual`, `Equal`, `NotEqual`,
`GreaterThanOrEqual` and `GreaterThan`. Both operands use finite IEEE-754
binary64, like JavaScript
Number: integers outside its exact range (beyond ±2^53) may round. This is not
an exact arbitrary-width integer comparison. The threshold must be a JSON number,
not a string or null. Missing paths and non-number values never match; no string,
boolean or null coercion occurs. Pointers follow the existing nonempty `/`-prefixed,
control-free, 256-byte bound and JSON-pointer `~0`/`~1` escaping.

Use `"comparison": "NotEqual"` to select numeric deviations from the threshold.
Unlike `Not` around an `Equal` predicate, `NotEqual` does not select missing or
nonnumeric values. For example, against 50, numeric 49 matches; 50, `"49"`,
`false`, null and an absent path do not. Signed zero equals zero, and wide
integers rounded to the same binary64 value are not unequal. The same comparison
is available in `JsonTextNumber` below; visited invalid JSON remains an error.
This adds a typed local analogue of the numeric `≠` operator in
[Antithesis Logs Explorer](https://antithesis.com/docs/reference/logs_explorer_search/),
not its boolean/number or string coercions. Readers must support `NotEqual`
before consuming the new comparison value; schemas and older comparisons are
unchanged.

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

All six comparisons use the same finite binary64 semantics, rounding and
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


## Intersection and difference

`Intersect { input, other }` keeps input events whose source position is also
selected by `other`. `Difference { input, other }` keeps input events absent
from `other`. For example, retain VM 0 errors:

```json
{
  "Intersect": {
    "input": {"Filter": {"input": "All", "predicate": {"Vm": 0}}},
    "other": {"Filter": {"input": "All", "predicate": {"Kind": "error"}}}
  }
}
```

Replace `Intersect` with `Difference` to retain VM 0 observations excluding
the error set. Use the `query`/`validate` commands above. Both operands can
independently project fields, filter, combine temporal queries or other set
operations. Nest `Intersect` for more than two operands; difference is ordered
and means `input - other`.

Identity is `(source_id, source_sequence)`, matching existing `Union`. Equal
ticks or sequence numbers from different sources do not imply membership.
Derived values do not affect membership: a right-hand projection may disagree
with the left-hand value without changing the result. Only left-hand rows and
projections survive; fields are not merged. Left order is preserved before
existing canonical sorting and first-wins duplicate removal. An empty right
set yields an empty intersection and leaves a difference unchanged; identical
position sets yield the left intersection and an empty difference.

Both children are admitted and evaluated even when the left side is empty.
Invalid right-side projected envelopes, malformed visited JSON and invalid
regexes remain errors rather than silently becoming an empty set. The owned
right vector is sorted in place by source position and binary-searched for
each left row, without extra key collections, Cartesian joins or copied left
rows during combination. Beyond child evaluation, charge one logical operation
per materialized left/right row. Existing node/depth/work/result limits apply:
row caps retain a canonical prefix with `matched`/`truncated` facts; byte/work
failures emit no partial CLI result or receipt. Logical counts are not CPU
comparison counts, wall-clock deadlines or total RSS bounds.

Differing direct child moments are ambiguous. An outer `UpTo` retains its
existing expression scope; it does not retroactively cap the operands. Receipts
bind both expressions, the operator and every supplied source event, including
nonselected right-side observations. New variants need upgraded readers; existing
queries and schemas are unchanged.

These implement bounded local analogues of
[Antithesis event-set intersection and difference](https://antithesis.com/docs/reference/event-set-reference/#set-operations).
ChaosControl deliberately uses source-position identity, not upstream full-field
synthetic-event equality. No arbitrary JavaScript, branch-lineage admission,
capture completeness, hosted event-search service, VM execution or replay-proof
claim is added.

## One observation per tick

`DistinctByTick` combines a nonempty ordered `inputs` list, retaining only the
first encountered row at each simulation tick. Prefer enriched observations
before falling back to all retained events:

```json
{
  "DistinctByTick": {
    "inputs": [
      {"Map": {
        "input": {"Filter": {"input": "All", "predicate": {"Kind": "preferred"}}},
        "field": "chosen",
        "pointer": "/label"
      }},
      "All"
    ]
  }
}
```

Run the same `query`/`validate` commands above. Earlier inputs win collisions;
within one input, its first encountered row wins. Winner selection precedes
final canonical sorting, so it is not a minimum-source-ID or earliest-position
selection. Reordering branches or tied source rows can change winners. Complete
winning envelopes/projections survive and remain usable by downstream filters;
losing fields are not merged. An empty `inputs` list is rejected, but one empty
branch or an empty source is valid. Zero and `u64::MAX` ticks need no conversion.

The key is the tick alone across all supplied sources and VMs. Callers must
supply a coherent tick domain and filter the scope they intend to collapse.
Use `Union` instead to retain distinct source positions at the same tick;
Union's existing identity and behavior are unchanged.

All descendants and materialized branch rows remain admitted, including later
branches whose ticks already have winners. Malformed visited JSON, invalid
regexes and invalid projected collisions are errors, not harmless duplicates.
One logical deduplication operation is charged per materialized branch row in
addition to child evaluation. A seen-tick set follows the existing Union
pattern; shared node/depth/work/scan/result bounds apply. Row caps retain the
canonical prefix with correct matched/truncated facts; byte/work failures emit
no partial CLI result or receipt. Logical counts are not CPU/RSS/deadline bounds.

Moments follow Union's rules: directly conflicting child moments are ambiguous;
an outer `UpTo` keeps its existing expression scope. Receipts bind branch order,
every expression, limits, every supplied source event (including losers), and
the complete winning rows. New variants require upgraded readers; schemas and
existing queries are unchanged.

This is a bounded local analogue of
[Antithesis `distinct_by_moment`](https://antithesis.com/docs/reference/event-set-reference/#distinct_by_moment),
using integer simulation ticks rather than upstream vtime/Moment objects. It
does not establish session or branch-lineage equivalence, automatic per-source
partitioning, complete capture, hosted event search, VM execution or replay proof.

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
