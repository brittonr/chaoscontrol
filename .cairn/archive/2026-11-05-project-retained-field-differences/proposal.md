## Why

Antithesis map/add_fields accepts arithmetic expressions. Local MapRatio retains utilization but cannot retain a difference between current and projected prior measurements. Metric triage needs numeric deltas after JSON extraction or WithLastValue.

## What Changes

Add MapDifference { input, field, minuend, subtrahend } to annotate finite binary64 subtraction results. Both operands are named canonical/derived fields, not literal numbers. Preserve input evidence and remove stale output for undefined arithmetic, following MapRatio. Consumer: retained metric-delta triage; owner: chaoscontrol-replay maintainers.

## Non-Goals

No arbitrary map interpreter, JavaScript coercion, exact integer/decimal arithmetic, automatic counter-wrap detection, implicit source partitioning, causality, multiverse lineage, complete capture, hosted search, CPU/RSS/deadline guarantee, VM execution or replay proof. This change does not implement upstream with_next: that operator returns matched future events and synthetic timeouts, unlike candidate-preserving scalar annotation. Preserve the user's pointer regression.
