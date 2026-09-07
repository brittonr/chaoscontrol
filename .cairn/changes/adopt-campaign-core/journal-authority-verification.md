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

## Remaining work

This change admits a root name. It does not prove filesystem authority or durable selection publication. Explorer still uses the existing frontier policy. The durable journal, history adapter, Campaign selection, and runtime publication fence remain open.

Shared component review identifies `durable-file-publication` as a candidate for single-file synchronization and no-replace publication. ChaosControl must still own the journal format, root opening, generation checks, recovery, and event-to-branch transaction. No dependency or storage claim enters this checkpoint.
