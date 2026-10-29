## ADDED Requirements

### Requirement: Retained queries narrow synthetic projections without losing source evidence

r[chaoscontrol.narrow_query.projections]

Narrow MUST accept an input query and a list of at most MAX_DERIVED_FIELDS field names. Each name MUST satisfy existing field-name admission even when no rows are selected. Narrow MUST preserve only listed derived entries, ignoring missing names, duplicate names and list order; an empty list MUST remove all derived entries. Canonical event identity, position, tick, owner, kind, text, details and payload digest MUST remain unchanged. Intermediate child rows MUST be admitted before pruning, so invalid projections and parsing cannot be hidden by removing fields. Narrow MUST propagate the child's moment, charge one logical operation per candidate in addition to child work, share structural and result bounds, and bind the complete expression and supplied source events into receipts.

#### Scenario: Compact enriched events

- GIVEN admitted canonical events and a bounded projection query
- WHEN Map or MapText adds temporary and selected fields and Narrow lists only the selected field
- THEN only that derived entry remains, canonical evidence is unchanged, downstream filters can use it, and receipt validation succeeds

#### Scenario: Empty and repeated selections

- GIVEN admitted events with existing derived entries
- WHEN the list is empty, contains unknown names or repeats a valid name
- THEN it selects no entries, ignores missing names or selects the repeated entry once respectively, without dropping events

#### Scenario: Invalid discarded data and bounds

- GIVEN a narrowing query with declared limits and optional receipt
- WHEN the list or names exceed bounds, child data is invalid, work or result bounds are exhausted, moments conflict, or receipt inputs drift
- THEN the operation is rejected without concealing discarded invalid data or publishing partial CLI evaluation artifacts
