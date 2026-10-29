## Why

Antithesis event expressions can read event.moment.vtime. Local canonical envelopes retain a tick but projection operands only expose selected canonical strings and derived values. Queries cannot copy the actual tick for annotation, correlation or elapsed-time composition without manually duplicating it into payloads.

## What Changes

Add MapTick { input, field } to copy the exact canonical u64 tick as unsigned decimal text into a derived field. Compose with WithLastValue and MapDifference for bounded elapsed-time triage. Consumer: retained restart/error timing analysis; owner: chaoscontrol-replay maintainers.

## Non-Goals

No tick-to-seconds conversion, new clock, source partition inference, causality or multiverse lineage, arbitrary map engine, exact downstream binary64 arithmetic, complete capture, hosted search, CPU/RSS/deadline guarantee, VM execution or replay proof. Preserve the user's uncommitted pointer regression.
