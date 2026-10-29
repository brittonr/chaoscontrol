## ADDED Requirements

### Requirement: Retained JSON text supports bounded numeric comparison

r[chaoscontrol.json_text_number.comparison]

JsonTextNumber MUST compare a confined JSON pointer's numeric value from admitted event text against a finite JSON numeric threshold using the existing binary64 comparison semantics, without coercion or canonical row mutation. It MUST preserve compound short-circuit evaluation, admission of skipped descendants, logical operation/structural/text bounds, moment handling and receipt binding. Visited invalid, trailing or over-deep JSON MUST fail with InvalidJsonText rather than become a non-match.

#### Scenario: JSON stdout exceeds a memory threshold

- GIVEN admitted retained JSON stdout and a finite numeric threshold
- WHEN a process-selected JSON stdout record contains a numeric memory value above the threshold
- THEN numeric text filtering selects the unchanged canonical event, independently of conflicting details or derived values, with receipts binding query/source/result.

#### Scenario: Values are absent or not numbers

- GIVEN admitted valid JSON text and a confined pointer
- WHEN JSON is valid but the pointer is absent or selects a string, boolean, null, array or object
- THEN the numeric leaf does not match; Not negates that result normally, without coercing values.

#### Scenario: Parsing, admission or budget fails

- GIVEN numeric text predicates under declared query and parser bounds
- WHEN visited text is invalid/trailing/over-deep JSON, or a pointer/threshold/structural/work bound is invalid
- THEN evaluation fails explicitly, and the CLI emits no partial result/receipt on evaluation failure; skipped text is not parsed but invalid skipped predicate structure is still rejected.
