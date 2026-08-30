# History Phenomena Checker

## Why

Workloads can detect raw state divergence but do not classify the consistency phenomena behind it. Jepsen and Antithesis define checkable phenomena with concrete check procedures: aborted reads, intermediate reads, garbage reads, stale reads, and lost writes. A pure checker over observed operation histories produces classified violations instead of raw divergence, which turns triage evidence into an actionable diagnosis.

## What Changes

- Add a pure core that accepts typed operation histories and builds a dependency graph.
- Add cycle detection over dependency edges to classify the named phenomena in linear time.
- Emit typed violations with the responsible operations attached.
- Add a shell that ingests round and log artifacts and validates history identities.

## Impact

- **Code**: a pure history model and checker core plus a shell over round artifacts.
- **Evidence**: classified phenomena enter the receipt flow as diagnosis evidence.
- **Testing**: positive fixtures that produce each named phenomenon and negative fixtures for clean histories and unclassifiable records.

## Non-Goals

- No network-level protocol analysis.
- No Byzantine-fault detection.
- No concurrent-history solver; cycle detection only, in the Elle pattern.
- No claim that a classified phenomenon identifies the code defect by itself.
