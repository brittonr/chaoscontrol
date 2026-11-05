## ADDED Requirements

### Requirement: Retained events support bounded temporal witness correlation

r[chaoscontrol.temporal_event_query.correlation]

Temporal MUST select unchanged candidate rows by existence or absence of independently queried witnesses under PrecededBy, NotPrecededBy, FollowedBy or NotFollowedBy. Witnesses MUST be strictly earlier/later in simulation ticks with an inclusive outer window edge and overflow-safe endpoints. Same-tick events MUST NOT witness each other. Zero-width positive windows MUST match none; negative forms MUST retain all candidates. Both child queries, witness envelopes, shared work/structural/result bounds and source/query/result receipts MUST remain admitted. Correlation MUST use in-place witness sorting and binary search rather than a Cartesian product, preserving candidate order and existing first-wins duplicate projections. Existing moment propagation and explicit UpTo expression scoping MUST remain unchanged.

#### Scenario: Nearby matching observations select candidates

- GIVEN unsorted retained candidates and matching witnesses in one caller-supplied tick domain
- WHEN a temporal relation evaluates within a declared tick window
- THEN only unchanged candidates satisfying strict tick direction and the outer boundary are selected in canonical order; negative forms invert witness existence

#### Scenario: Empty, equal and extreme tick boundaries

- GIVEN empty witness sets, equal ticks, zero windows or ticks near zero/u64 maximum
- WHEN positive or negative correlation evaluates
- THEN it follows the declared existence rules without overflow, fabricated ordering or duplicated candidates

#### Scenario: Bounds, moments and drift remain binding

- GIVEN both admitted expressions under declared limits and retained source/query/result receipts
- WHEN witness admission fails, a bound is exceeded, directly combined moments disagree or bound facts drift
- THEN evaluation or validation fails; CLI evaluation failures emit no partial artifacts and explicit UpTo scopes only the expression it wraps
