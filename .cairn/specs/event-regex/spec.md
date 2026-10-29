# Event Regex Specification

## Purpose

Defines the `event-regex` capability.

## Requirements

### Requirement: Retained event strings support bounded regex selection

r[chaoscontrol.event_regex.selection]

Regex MUST select canonical or derived string fields using default case-sensitive Unicode pattern search, without canonical mutation or changing Contains/Matches semantics. Each distinct pattern MUST compile once per admission under named program-size, DFA-cache and nesting bounds, and be reused across candidate rows and query branches. Invalid syntax, unsupported constructs or compilation bound failures MUST return InvalidRegex even for skipped descendants or empty sources. Field/pattern byte admission, canonical precedence, missing-field non-match, compound short-circuit/Not behavior, operation/structural bounds, moments and source/query/result receipt binding MUST remain effective.

#### Scenario: Regex selects logs and projected values

- GIVEN retained events with canonical strings and projected derived strings
- WHEN regex patterns use anchors, character classes, alternation or Unicode
- THEN matching events retain all canonical fields, canonical field names retain precedence and existing literal/exact selectors remain unchanged

#### Scenario: Invalid or over-resource patterns are never skipped by admission

- GIVEN skipped regex descendants or an empty admitted source
- WHEN pattern syntax, expansion, nesting or field/pattern byte bounds fail
- THEN admission rejects the query before row evaluation and CLI evaluation failures publish no partial artifacts

#### Scenario: Bounds and receipts remain authoritative

- GIVEN admitted regex queries under declared work/structural limits and moments
- WHEN a work bound is exhausted or query/source/result facts drift
- THEN evaluation or receipt validation fails, including query changes with identical selected rows
