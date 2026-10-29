## Context

EventPredicate already supports All/Any/Not, confined JSON pointers, strict parsing of bounded canonical text and source/query/result-bound receipts. JsonNumber is false on missing and nonnumeric values; Not negates both. Map scalar projection is not a presence probe because it drops null and containers.

## Decisions

Add JsonPresent and JsonTextPresent leaves, each with a pointer string. Presence means Value::pointer resolves to Some, not truthiness or type conversion. An absent details object or an untraversable path is absent. Explicit null at a resolved path is present. JsonPresent never parses text. JsonTextPresent parses only when visited, rejects malformed/trailing/over-deep JSON using InvalidJsonText and borrows the parsed tree without cloning canonical data. Do not add a cache, wrapper data, dependencies or limits. All/Any may skip parsing; admission still validates every descendant, even with an empty source. Each visited presence leaf uses the existing logical filter operation charge and shared node/depth counters. Not never converts parse failures into matches.

## Verification

Reproduce unknown-variant deserialization first. Regress both sources over all JSON types, absence and explicit null, conflicting data, nested/escaped/Unicode/array paths, presence guards with negation, visited parse failures versus skipped branches, pointer/text/operation/structural boundaries, UpTo and receipt drift. Exercise actual CLI positive/negative query/validate paths. Run replay tests, Clippy, workspace compilation, formatting, Nickel product-scope/evidence checks and Cairn gates. Preserve the user's existing confined-pointer test and report unrelated global traceability failures.
