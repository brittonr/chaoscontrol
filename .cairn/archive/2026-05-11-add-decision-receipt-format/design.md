## Context

Fleet triage is currently static HTML over one or more replay-readiness receipts. Operators still need a structured way to record reproduce/minimize/defer/reject decisions without implying a hosted workflow or shared database.

## Goals / Non-Goals

**Goals:**
- Define one Rust-owned JSON receipt shape for bounded local triage decisions.
- Validate the receipt fail-closed against missing source receipts, duplicate decision IDs, unsupported actions, raw-log scraping, and shared-store overclaims.
- Package a sample/validation CLI through Cargo and Nix.

**Non-Goals:**
- No hosted service, UI backend, scheduler, cross-machine state synchronization, or shared database.
- No raw log scraping. Decisions link to replay-readiness receipts, fleet index artifacts, and committed bug/replay artifacts.

## Decisions

### 1. Rust-owned receipt model

**Choice:** Keep the receipt as Rust-validated JSON in `chaoscontrol-evidence`.
**Rationale:** Runtime/operator evidence is already Rust-owned; this keeps validation deterministic and available to CI/Nix.
**Alternative:** A Nickel-authored decision schema first. Rejected for this slice because the format is runtime/operator evidence rather than human-authored config.

### 2. Local artifact, not store

**Choice:** The CLI writes or checks one JSON receipt file and includes explicit anti-claims.
**Rationale:** This narrows the missing persistence/review seam without creating or implying a shared decision store.

## Risks / Trade-offs

**Overclaiming fleet parity** → The status remains an unpromoted hosted/fleet surface and requires shared/hosted evidence for promotion.

**Ad hoc receipt drift** → Model tests, CLI `--check`, selftest coverage, and Nix packaging keep the sample and validator aligned.
