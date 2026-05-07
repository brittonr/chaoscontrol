## Context

The readiness command is orchestration glue around existing Python checks and selected Nix dogfood apps. A receipt should not become a new evidence promotion path; it should summarize one invocation for operator automation.

## Goals / Non-Goals

**Goals:** Emit stable JSON for checks-only and selected-dogfood readiness invocations; include enough fields for CI/dashboards to distinguish static gate failure from optional dogfood failure.

**Non-Goals:** Automatically curate or commit dogfood evidence; claim universal determinism; replace the generated readiness status report.

## Decisions

### 1. Receipt is explicit opt-in

**Choice:** Add `--receipt <path>` rather than always writing into the repository.

**Rationale:** CI jobs can choose an artifact path, while local checks remain side-effect-light by default.

### 2. Shell wrapper owns orchestration receipt

**Choice:** Keep the receipt in the existing Nix shell application and use Python only to serialize JSON safely.

**Rationale:** The wrapper already knows the selected static gates and exact dogfood app paths. Moving all orchestration into a separate source script would require threading Nix store app paths through another layer for little gain.

### 3. Failure receipts are best-effort

**Choice:** Write a failure receipt before exiting when a static gate or selected dogfood command returns nonzero.

**Rationale:** Dashboards need failure phase/status without scraping logs. Serialization failure should fail closed rather than hiding the command failure.
