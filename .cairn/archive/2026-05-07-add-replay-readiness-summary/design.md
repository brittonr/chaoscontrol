## Context

The readiness receipt is intentionally JSON for automation. The next smallest dashboard seam is not a full web UI; it is a stable CLI consumer that lets CI emit one grep-friendly status line while retaining the full JSON artifact.

## Goals / Non-Goals

**Goals:** Provide a dependency-light receipt summarizer; make the output stable enough for CI log scanning; fail closed on malformed or incomplete receipts.

**Non-Goals:** Replace the JSON receipt, host a dashboard, or promote dogfood evidence automatically.

## Decisions

### 1. Python script plus Nix app

**Choice:** Add `scripts/summarize-replay-readiness-receipt.py` and expose it as `.#replay-readiness-summary`.

**Rationale:** Python is already used for repo-local evidence/readiness tooling, and the Nix app gives CI a pinned entrypoint.

### 2. One-line default output

**Choice:** Print exactly one line by default: status, static gate pass/total, dogfood state, failed phase, and scope token.

**Rationale:** CI logs and simple dashboards need a compact signal while the JSON receipt remains the detailed artifact.

### 3. Malformed receipts fail closed

**Choice:** Missing required fields or inconsistent gate shapes return nonzero with a concise diagnostic.

**Rationale:** A dashboard summary must not accidentally convert a malformed receipt into a green status.
