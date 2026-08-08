## Context

The checks-only readiness rail is intentionally static by default; slow KVM dogfood remains opt-in. CI should not duplicate dogfood promotion or curate evidence automatically.

## Decisions

### Check-owned artifacts

**Choice:** Expose `checks.x86_64-linux.replay-readiness` as a `runCommand` that invokes `replay-readiness --receipt`, then `replay-readiness-summary`, and leaves `replay-readiness-receipt.json` plus `replay-readiness-summary.txt` in `$out`.

**Rationale:** The check surface is reproducible, easy to build locally, and lets GitHub Actions upload a bounded artifact without inventing a second receipt writer.

### CI upload is static-only

**Choice:** GitHub Actions builds the check, prints the saved summary line, and uploads the two check outputs as `replay-readiness-receipt`.

**Rationale:** The default CI rail should prove committed replay/evidence readiness while preserving anti-claims and avoiding implicit KVM/kernel dogfood runs.

## Risks / Trade-offs

- The check repeats static evidence gates already covered by `evidence-contracts`; this is acceptable because the new output is operator artifacts, not just pass/fail.
- Nix check outputs live in the store; CI links them under `target/` only for upload.
