# Cleanup and resumed verification

## Operator cleanup

The user authorized cleanup after quotas blocked edits, logs, and Git commits. Inspection found no ZFS snapshot usage for either affected dataset.

`/tmp/mantle-source-observation-target/debug` contained about 40 GiB of disposable Cargo output. No process held a file open beneath that cache. Cleanup acquired its Cargo lock before removal. Source, worktrees, evidence archives, and active builds remained intact.

After cleanup, `/tmp` had about 38 GiB free. The Git dataset separately recovered about 196 GiB. This operation does not claim responsibility for that separate recovery. Concurrent builds continue to consume space.

This was a bounded recovery action, not a retention service. The operator owns cleanup. Revisit build-output retention at the next quota failure. No automatic deletion policy was introduced.

## Published package closeout

VM Cohort main now contains `d03cb382f3d5e5ad18f2cb914d97c3830c8fb482`. The owner synced accepted specs and archived `2026-09-07-package-conformance-profile`. All eight Nix checks passed again from an immutable archive. The teardown guard passed before removal of the clean owner worktree. The dirty primary remained unchanged.

ChaosControl pins implementation revision `0953ab17d2f4e318567a57925dc8fe30669d5b68`. The actual vendored test passes from frozen consumer `c5448745d66fc3945aadce83da9064abe961e13c`. Its nine portable tests pass, with three existing ignored KVM tests. The exact-source, transport, dependency-policy positive and negative controls, and Nix formatting checks pass.

## Resumed consumer checks

After the protocol and fault repairs:

- Explorer library tests pass: 214 tests and one existing ignored KVM placeholder.
- Fault library tests pass: 105 tests, no ignores.
- Protocol tests pass: 38 with all features and 15 without default features, no ignores.
- Strict Clippy passes for the changed protocol feature configurations and the fault and Explorer library/test scopes.
- Workspace Rust formatting and Cairn validation pass.

Logs remain under `campaign/.pi/complete-20260906/`. The final local rail uses `cleanup-resume-explore-tests.log`, `cleanup-resume-clippy.log`, `cleanup-resume-format.log`, and `cleanup-resume-cairn.json`.

## Focused workspace lockfile

The full frozen check of `e89a258639ffbc07d11ca7ba70342d5ddf25b183` found a stale lockfile in the VM Cohort Octet workspace. The existing mutation guard correctly rejected it.

Cargo metadata regenerated this separate lockfile from the exact Nix-assembled workspace. The only changes add existing `serde_json` edges for `chaoscontrol-protocol` and `chaoscontrol-sim-core`. No package version changed. The product lockfiles remained unchanged.

The `vm-cohort-octet-workspace` package now exposes the existing assembly for repeatable regeneration. Its README documents the review and generation steps. The strict `vm-cohort-adapter-octet-deny-all` check passes with zero findings and its mutation guard intact. Nix formatting also passes.

Logs: `cleanup-resume-full-nix.log`, `vm-cohort-octet-lock.diff`, and `vm-cohort-octet-lock-fixed.log`.

## Remaining blocker

The full Nix check ran again from an archive of `77ccefa7fb00f267f3693c26b9c536b96d7b61dd`. Its terminal failure is `checks.x86_64-linux.tigerstyle-chaoscontrol-focused`: two implicit external `VmConfig::default()` calls at `chaoscontrol-evidence/src/guest_determinism.rs:35` and `:75`. See `cleanup-lock-final-full-nix.log`. This confirms the same failure from `fault-default-octet.log` after the separate lockfile repair.

The check also reports broader existing warnings. These results are not full strict acceptance. The VM Cohort adapter's strict check passes separately.

The durable journal, actual Campaign runtime adapter, Campaign-backed KVM proof, fleet integration, and Campaign lifecycle closure remain open. No live host configuration changed during this recovery.
