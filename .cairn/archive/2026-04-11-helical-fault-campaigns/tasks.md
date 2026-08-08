## 1. Scenario model and persistence

- [x] 1.1 Add `ScenarioFamily`, `ScenarioConfig`, and phase-summary types plus deterministic materialization helpers that compile to ordinary `FaultSchedule`s
- [x] 1.2 Thread scenario config and phase summaries through run/campaign config structs and checkpoint formats with backward-compatible serialization
- [x] 1.3 Add unit tests proving identical config+seed materializes the same schedule and different seeds change the materialization

## 2. Built-in helical families

- [x] 2.1 Implement `network-ring` with rotating partition and restart phases
- [x] 2.2 Implement `volatile-write-ring` with `DiskFsyncLie`, a destructive boundary, and an explicit recovery window
- [x] 2.3 Implement `degraded-io-ring` with `DiskSlow` or `DiskPartialRead` plus restart or partition pressure and recovery windows
- [x] 2.4 Add tests asserting turn rotation, required fault presence, and recovery-window insertion for each family

## 3. CLI, reporting, and replay integration

- [x] 3.1 Add `--scenario`, `--scenario-phase-ticks`, and `--scenario-turns` to `run`, `campaign`, and `campaign resume`
- [x] 3.2 Record scenario metadata and phase summaries in human-readable reports, machine-readable reports, bug artifacts, and campaign checkpoints
- [x] 3.3 Preserve helical scenario provenance through replay, reproduce, and minimization flows while still replaying the concrete schedule
- [x] 3.4 Add integration tests for resume, report, replay, and minimization metadata preservation

## 4. Documentation and validation

- [x] 4.1 Document the built-in helical scenario families and their knobs in CLI/help text or docs
- [x] 4.2 Run targeted tests for `chaoscontrol-explore`, `chaoscontrol-fault`, and `chaoscontrol-replay`
- [x] 4.3 Run `cargo clippy --all-targets -- -D warnings`
- [x] 4.4 Run `cargo fmt --all --check`
