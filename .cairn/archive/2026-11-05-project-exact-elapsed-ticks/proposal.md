## Why

Antithesis event expressions support time-oriented annotations. Local MapTick exposes exact canonical ticks, but subtracting them through binary64 MapDifference loses adjacent-tick durations near large ticks. Retained restart/error timing needs exact durations across the u64 domain.

## What Changes

Add MapElapsedTicks { input, field, since_field }. Parse a named prior tick as a complete JSON u64 integer, subtract it from the canonical tick with checked arithmetic, and retain unsigned decimal elapsed text. Undefined/future ticks clear stale output without dropping evidence, following numeric projection conventions. Consumer: exact retained restart/error timing; owner: chaoscontrol-replay maintainers.

## Non-Goals

No clock or tick-to-seconds conversion, arbitrary arithmetic interpreter, counter-wrap inference, implicit VM/process/source partitioning, causality or multiverse lineage, complete capture, hosted search, CPU/RSS/deadline guarantee, VM execution or replay proof. FieldNumber/other downstream floating-point consumers remain binary64. Preserve the user's uncommitted pointer regression.
