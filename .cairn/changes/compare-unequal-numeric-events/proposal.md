## Why

Antithesis Logs Explorer includes numeric inequality (`≠`). Retained ChaosControl queries currently expose only five numeric comparisons. Negating Equal is not a substitute: it also selects missing and nonnumeric fields.

## What Changes

Add NumberComparison::NotEqual to the existing shared finite-binary64 comparison implementation used by JsonNumber and JsonTextNumber. Preserve strict numeric typing, source separation, parsing/admission failures, query bounds, canonical rows and receipts. No coercion, new parser, cache, limit or query variant.

## Impact

Owner: replay/query maintainers. Consumer: operators finding numeric state or metric deviations in retained structured details and JSON logs. Durable capability: a complete six-operator typed numeric comparison vocabulary. New comparison values require upgraded readers; existing query/result/receipt schemas stay unchanged. This is bounded local query behavior, not hosted Logs Explorer compatibility, upstream coercions, complete capture, VM execution, replay proof or release readiness.
