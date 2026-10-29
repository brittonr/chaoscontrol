## Why

The Antithesis event-set guide parses `event.output_text` as JSON and projects fields before filtering. ChaosControl's Map reads only structured event details; JSON stdout text cannot be projected without external preprocessing. Retained serial/log evidence already provides bounded canonical text.

## What Changes

Add MapText with the same input/field/pointer shape as Map. Parse each admitted row's text as JSON, project the selected scalar into a derived string using existing scalar conversion, and preserve every original source field including text, details and payload identity. Reject malformed/trailing/depth-excess JSON with InvalidJsonText rather than skipping observations. Valid JSON with a missing or non-scalar pointer leaves the existing derived map unchanged, matching Map. Existing query limits, canonical field precedence, ordering, moments and receipts remain effective.

## Impact

Owner: replay/query maintainers. Consumer: local operators selecting process/component fields from JSON emitted on stdout and retained in recordings or canonical envelopes. Immediate outcome: query-local JSON log projection usable by existing text filters. Durable capability: bounded reusable text-to-derived-field extraction over retained evidence. Projected numbers/booleans remain strings, as in Map; this does not add arbitrary JSON transformations or numeric comparisons over derived strings. Existing schemas and queries stay unchanged; new variants require upgraded readers. Retained-input behavior only, not capture completeness, live branches, full Notebook compatibility, VM/replay proof or release readiness.
