## ADDED Requirements

### Requirement: Text selectors compose as bounded event predicates

r[chaoscontrol.text_event_predicate.selection]

Retained event filters MUST support literal case-sensitive substring Contains and exact-string Matches predicates within All/Any/Not. They MUST reuse existing canonical/derived field resolution and nonempty/control-free field/text byte bounds. Canonical fields MUST retain precedence over derived names; missing fields MUST not match either positive selector. Negation MUST invert the complete result, including missing-field non-matches. Every leaf MUST be admitted even when short-circuiting would skip it or the source is empty. Existing node/depth/operation bounds, canonical fields/order, standalone query operators and query/source/result-bound receipts MUST remain effective.

#### Scenario: Warning selection with exclusions

- GIVEN warning/error text, heartbeat noise and other messages
- WHEN a compound predicate selects warnings or errors and negates heartbeat matches
- THEN only complete matching original events are returned in canonical order

#### Scenario: Field and string semantics

- GIVEN Unicode text, missing fields, projected strings and derived names colliding with canonical fields
- WHEN substring or exact matching runs
- THEN comparison is case-sensitive, missing fields do not match, projected strings are usable and canonical fields cannot be shadowed

#### Scenario: Admission, budgets and receipt identity

- GIVEN text predicates at exact field/needle limits and a valid receipt
- WHEN a leaf is malformed/over-bound, its operation budget is insufficient, or its needle changes even without changing selected rows
- THEN admission/evaluation or receipt validation rejects the input instead of accepting an unbound or partial result
