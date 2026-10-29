## Why

Antithesis Logs Explorer provides structured custom-field presence and absence operators. ChaosControl's negated numeric predicates include missing fields, so callers cannot express an explicit field-presence guard without external preprocessing. Map/MapText omit null and nonscalar values and cannot substitute for existence checks.

## What Changes

Add JsonPresent { pointer } over canonical details and JsonTextPresent { pointer } over JSON event text. A resolved pointer is present regardless of its JSON value, including null, false, zero, empty strings and containers. Missing paths are false; Not expresses absence. Text predicates retain strict parsing errors and short-circuit behavior. Reuse all existing pointer, operation, structural, moment and receipt rules.

## Impact

Owner: replay/query maintainers. Consumer: operators distinguishing missing fields from explicit values during retained-event triage. Durable capability: composable bounded presence guards for existing numeric/text query workflows. New variants require upgraded readers; existing variants and schemas are unchanged. No claim of upstream search syntax/coercion compatibility, capture completeness, live branches, hosted Logs Explorer, VM correctness or replay readiness.
