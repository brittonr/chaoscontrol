# Protocol-observation verification

## Status

The change remains active and unarchived.
The user requested an unfinished checkpoint commit and branch push on 2026-09-05.
Five tasks remain open, including snapshot-backed session replay and publication of the accepted contract.
The checkpoint does not authorize lifecycle completion or integration into `main`.

The worktree starts at `31300fa1a2d29c7496e8316f065c156f80343143`.
The branch is `drain/protocol-observation-cohorts-20260904`.
The dirty primary checkout and Mantle remain unchanged by this work.

## Checked implementation

- Typed Nickel profiles bind finite limits, schemas, producers, participants, generations, oracle authority, novelty selection, and non-claims.
- The pure core admits opaque records and derives domain-separated BLAKE3 identities.
- Cohorts retain canonical source journals, host losses, classifications, and full novelty identities.
- The SDK checks counters and framing before transport effects.
- The VMM supplies the VM identity, executing vCPU, exit sequence, and scheduler identity.
- The independent Raft fixture ignores runtime pass fields and detects conflicting leaders at its declared term boundary.
- Receipts revalidate cohort, oracle, marker, and context bindings.
- Fault-run resets preserve the protocol journal and loss count. An admitted snapshot restore can replace that execution history.
- Marker status reports `identity-linked`, not snapshot reachability.

## Baselines and negative controls

The initial relevant package suites passed before core implementation.
The original baseline log is `/tmp/chaoscontrol-protocol-baseline-tests.log`.

`evidence/attempts/adversarial-baseline.log` retains the first accounting counterexamples.
They cover excessive limits, changed duplicates, post-final records, and unsupported projections that hid malformed input.

`evidence/attempts/reset-status-before.log` retains two later failing controls.
A fault-run reset erased a recorded host loss.
A pure marker binding produced the unsupported status `reachable`.
Both controls pass after the corrections.

The no-default SDK build initially failed because the guest-supervisor binary lacked its `full` feature requirement.
`evidence/attempts/sdk-no-default-before-bin-gate.log` retains that failure.
The corrected build succeeds. Two inherited coverage-helper warnings remain in that configuration.

## Validation

Final logs use `evidence/verification-2026-09-05/`.

| Check | Result | Evidence |
| --- | --- | --- |
| Six relevant packages, all targets and all features | Passed | `all-target-tests.log` |
| Strict six-package Clippy, all targets and all features | Passed | `clippy.log` |
| Protocol-observation integration cases | 23 passed within the package suites | `all-target-tests.log` |
| No-default protocol and SDK build | Passed with two inherited warnings | `no-default-features.log` |
| License boundary | Passed, 59 rules | `license-boundary.log` |
| Final scoped Nix tests and contracts | Passed, 23 cases and seven invalid profiles | `nix-focused.log` |
| Final KVM shared-page snapshot case | Passed, one explicit ignored-test invocation | `kvm-page.log` |
| Changed Rust sources and Nix formatting | Passed | `rustfmt.log` and `nixfmt.log` |
| Final Octet check | Blocked by two inherited guest-probe errors | `octet.log` |
| Product scope | Blocked by the missing `adopt-campaign-core` intent | `product-scope.log` |
| Contract registry | Passed | `registry.log` |
| Lifecycle validation and three gates | Passed | `cairn-validate.json`, `cairn-proposal.json`, `cairn-design.json`, and `cairn-tasks.json` |
| Full flake check | Blocked by the Cargo metadata panic in `dependency-policy` | `flake.log` |

The initial scoped Nix check passed 18 integration cases before the later regression fixtures.
The Nickel check admitted the exact exported profile and rejected all seven invalid profiles.
Earlier KVM evidence covers host dispatch and snapshot retention, not an executed protocol-aware guest continuation.

## Build repairs

Crane already adds all-target scope to its dependency check.
The focused dependency arguments no longer duplicate that flag or request a named integration test from the dummy source.
The actual Nix test still runs the named protocol-observation suites.

Optional Serde fields retain missing-field compatibility without implicit external defaults.
The existing process queue and process-identity set use explicit empty constructors.
Positive and negative compatibility fixtures cover omitted fields, wrong types, and missing required fields.
The license policy now includes the existing guest-determinism probe with its existing workspace license.
No license grant or third-party notice changed.

## Remaining boundaries

`Session::replay` still lacks a test that restores a stored parent and executes a real protocol-aware guest continuation.
The marker and snapshot completion tasks remain open until that evidence exists.
Pure identity equality and host-only dispatch cannot replace this test.

The Octet check now reaches `crates/chaoscontrol-evidence/src/guest_determinism.rs`.
It rejects `VmConfig::default()` at lines 35 and 75.
That file is unchanged by this change. A separate repair must establish explicit guest-probe configuration.
The output also contains warnings, including naming and owner-path findings in the new code.
No clean Octet claim is available. No lint catalog, warning budget, baseline, or severity changed.

The product-scope projection now matches its Nickel source.
The next guard rejects the unrelated active `adopt-campaign-core` package because it lacks an admitted intent.
The guard remains unchanged. This change does not invent that package's product authority.
The final stable-input flake check failed in `checks.x86_64-linux.dependency-policy`.
Cargo metadata panicked at `package_id_spec.rs:248:40` during the locked offline Cargo deny check.
The dependency policy remains enabled. No release-wide validation pass is available.

Task 559 passed the scoped Nix checks, KVM case, and formatting.
Task 560 retained the Octet failure. Task 573 retained the product-scope blocker after projection export.
Task 574 retained the full-flake failure. Task 538 passed lifecycle validation and all three gates.
`product-inputs.b3` and `build-inputs.b3` bind the checked product files and build inputs.

Tasks 449 and 454 did not run their checks because `/tmp` rejected log creation with `No space left on device`.
Task 444 stopped during Nix garbage collection without usable checker output.
Task 455 failed with exit 102 after the lifecycle task file changed during its delayed evaluation.
That attempt is invalid validation evidence. The next attempt uses unchanged inputs throughout evaluation.
Private-home and `/var/tmp` build-directory overrides also failed Nix security checks.
The retained attempts explain those failures. No home permission or Nix security rule changed.
The configured default build directory worked after the storage pressure cleared.
Final retries use that default and retain logs in this change package.
Neither the storage failures nor the changed-input attempt are validation passes.

## Non-claims

Guest process references are declarations, not process authentication.
The oracle trait is a reviewed pure-code contract, not a plugin sandbox.
The consumer retains protocol semantics and oracle authority.
Snapshot links do not prove reachability, restorability, or guest continuation.
The evidence does not prove universal correctness, production readiness, release eligibility, a protocol total order, or synchronized wall clocks.
