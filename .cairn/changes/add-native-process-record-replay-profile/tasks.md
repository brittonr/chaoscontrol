# Tasks: Publish a native-process record/replay profile

## Profile and records

- [x] [serial] Publish the bounded profile and descriptor types with canonical identity. r[chaoscontrol.native_process_profile.identity]
- [x] [serial] Publish ordered lifecycle records with executable, tick, exit, and evidence bindings. r[chaoscontrol.native_process_profile.records]

## Admission and boundary

- [x] [serial] Validate schema, version, cohort, non-claims, bounds, order, identities, ticks, and reference counts before use. r[chaoscontrol.native_process_profile.admission]
- [x] [serial] Keep descriptors detached from semantic observations, roots, authority, repair, voting, and equivalence. r[chaoscontrol.native_process_profile.detached_nonclaims]

## Verification

- [x] [parallel] Add positive complete-descriptor coverage and the declared negative matrix. r[chaoscontrol.native_process_profile.validation]
  - Evidence: [native-process profile evidence](evidence/2026-10-28-native-process-profile.md); `cargo test -p chaoscontrol-protocol --lib --all-features` → 47 passed, including six native-process cases.
- [x] [serial] Run formatting, strict Clippy, and Cairn validation.

## Verification Coverage

- `Scenario: Complete profile identifies` -> identity test
- `Scenario: Stale profile is denied` -> schema and version tests
- `Scenario: Sequence regresses` -> sequence test
- `Scenario: Order drifts` -> order test
- `Scenario: Lifecycle record is accepted` -> positive descriptor test
- `Scenario: Path or secret leaks` -> text-guard test
- `Scenario: Consumer validates a diagnostic member` -> identity test
- `Scenario: Non-claims are altered` -> non-claim test
- `Scenario: Focused suite runs` -> protocol crate suite
