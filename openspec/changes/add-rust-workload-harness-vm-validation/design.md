## Context

`add-rust-workload-harness` added the downstream-shaped Rust guest and the Nix apps/packages needed for local and VM rails. Local instrumentation proof passes; the VM app was started from the repo root but remained inside `nix run` for more than 30 minutes with no output and no materialized report directory.

## Goals / Non-Goals

**Goals:**

- Run the bounded Rust workload VM campaign to completion.
- Capture the output directory and classification receipt.
- Keep replay-proof language conservative unless replay/minimization artifacts are present.

**Non-Goals:**

- Change the harness API unless the VM run exposes a concrete bug.
- Broaden ChaosControl toward non-Rust or container-first onboarding.

## Decisions

### 1. Runtime validation is separated from implementation closure

**Choice:** Treat the slow/hung VM campaign as follow-up runtime validation rather than blocking archive of the already implemented harness/local/Nix rail.

**Rationale:** The code and Nix package/initrd/local report rails are verified; the only missing evidence is an expensive runtime proof that can run under a longer-lived queue or after cache warming.

**Alternative:** Keep retrying in the parent change. Rejected because the command exceeded the local drain budget without producing intermediate evidence.

### 2. Evidence classification remains explicit

**Choice:** The VM validation receipt must preserve `bounded-vm-campaign` separately from snapshot-backed replay proof.

**Rationale:** This avoids overclaiming Antithesis-equivalent replay evidence from a schedule-only or incomplete VM run.

## Validation Plan

- Run `nix run .#explore-rust-workload -- /tmp/cc-rust-workload-vm-validation` or an equivalent built app command with build logs enabled.
- Inspect the output directory and `evidence-classification.json`.
- Run strict OpenSpec validation and whitespace checks before archive.
