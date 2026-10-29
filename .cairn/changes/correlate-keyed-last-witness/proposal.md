## Why

Retained restart/error triage currently needs separate WithLastValue queries per process or service. A globally nearest restart can belong to another owner. Antithesis with_last supplies per-timeline prior-event context; ChaosControl needs an explicit local key instead of inferring lineage or conflating owners.

## What Changes

Add WithLastValueBy with input, witness, field, witness_field, key_field and within_ticks. Copy a scalar from the closest strictly earlier witness whose selected key equals the candidate's key. Missing keys never match; present empty keys do. Reuse the existing unkeyed annotation engine and its admission/receipt/bound rules. Consumer: multi-process and projected-service restart/error correlation; maintenance owner: chaoscontrol-replay maintainers.

## Non-Goals

No automatic source/VM/history partitioning, causality, multiverse ancestry, composite keys, nested witness objects, future matching or synthetic timeouts, hosted search, arbitrary joins/map execution, complete capture, CPU/RSS/deadline guarantees, VM execution or replay proof. Preserve existing queries and the user's uncommitted pointer regression.
