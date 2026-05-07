## MODIFIED Requirements

### Requirement: One-command bounded workload run [r[rust-workload-harness.run-command]]

ChaosControl MUST provide a single documented command or flake app that runs a bounded campaign for a harness workload from a downstream Rust project. The command MUST build the guest, run local or VM execution as requested, and write a report path that can be inspected after completion.

#### Scenario: Run command executes sample workload [r[rust-workload-harness.run-command.sample]]

- GIVEN a sample Rust workload using the harness
- WHEN the user runs the documented bounded workload command
- THEN ChaosControl builds the guest, runs the bounded campaign, and writes a report path

#### Scenario: Run command preserves replay evidence boundary [r[rust-workload-harness.run-command.evidence-boundary]]

- GIVEN a bounded workload run that finds a bug
- WHEN the run writes report output
- THEN the report distinguishes a local/dry-run finding, a schedule-only reproduction gap, and an accepted snapshot-backed replay verdict rather than promoting all findings equally

#### Scenario: VM validation command completes [r[rust-workload-harness.run-command.vm-validation]]

- GIVEN the Rust workload harness VM rail and a machine capable of running the campaign
- WHEN `.#explore-rust-workload` is run with a writable output directory and sufficient build/runtime budget
- THEN the command completes and writes inspectable VM campaign output and an evidence classification receipt
