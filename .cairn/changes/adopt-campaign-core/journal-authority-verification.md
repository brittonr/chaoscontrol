# Journal authority admission

The user approved rejection of no-output runs without journal authority.

## Implemented boundary

`Explorer::run` first calls the pure root admission function in `explorer/journal_authority.rs`. Missing, empty, and NUL-containing root names produce the existing `ExploreError::Config` error. Public method signatures and error variants remain unchanged.

Admission preserves valid caller names exactly. It does not choose a default directory, create files, open directories, or claim durability.

The shell guard precedes the run clock, bootstrap, metrics, and expansion. A regression test requires a journal-specific error. It also checks that no controller, rounds, branches, metrics sink, or metrics file exists after rejection.

## Verification

The Nix development shell supplied the pinned Cargo profile.

- Baseline library suite: 211 passed, one existing ignored placeholder.
- Updated library suite: 214 passed, one existing ignored placeholder.
- Strict Clippy for the library and tests passed with `-D warnings`.
- Positive fixtures preserve relative, absolute, current-directory, and spaced root names.
- Negative fixtures reject missing, empty, and NUL-containing root names.

Operator logs: `journal-baseline.log`, `journal-authority-final-tests.log`, and `journal-authority-final-clippy.log`.

## Full Nix attempt

The frozen `be740954711e015d240194167bc6dec19664bbfa` archive failed the strict license inventory. The existing guest determinism probe lacked a package rule. Commit `fe91e795bee4f639ebefdb9da3a2b32ef92e6497` adds its existing inherited AGPL license to that inventory. The checker and its positive and negative controls pass. No license changed.

The next frozen full check failed `dependency-policy`. Stock Cargo 1.98.0 panicked during offline metadata for `cargo deny`:

```text
src/tools/cargo/crates/cargo-util-schemas/src/core/package_id_spec.rs:248:40
called `Option::unwrap()` on a `None` value
```

The exact triggering package ID remains unverified. The earlier manifest errors from Campaign checker fixtures did not stop vendoring. They are not the terminal error. The unsupported transport-config warning is also not proof of the panic cause.

Logs: `journal-full-nix.log`, `journal-license-fixed.log`, and `journal-full-nix-retry.log`. The full Nix gate remains blocked. No policy check was disabled.

## Remaining work

This change admits a root name. It does not prove filesystem authority or durable selection publication. Explorer still uses the existing frontier policy. The durable journal, history adapter, Campaign selection, and runtime publication fence remain open.

Shared component review identifies `durable-file-publication` as a candidate for single-file synchronization and no-replace publication. ChaosControl must still own the journal format, root opening, generation checks, recovery, and event-to-branch transaction. No dependency or storage claim enters this checkpoint.
