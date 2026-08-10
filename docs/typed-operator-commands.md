# Typed operator commands

ChaosControl executes replay-readiness plans as typed process requests. It does not pass plan text to a command interpreter.

## Plan boundary

`contracts/evidence/operator-command.ncl` owns the human-authored plan contract. The contract requires these facts:

- an absolute executable path, BLAKE3 identity, and byte bound
- an ordered argument array
- a capability-relative working directory without parent traversal
- a cleared environment and explicit environment entries
- null or identity-bound input
- finite timeout, stream, polling, and teardown limits
- accepted exit codes and truncation policy
- process-group termination for evidence-eligible work

The checked example is `contracts/evidence/examples/typed-operator-command.ncl`. Its deterministic JSON projection is `contracts/evidence/fixtures/valid/operator-command.valid.json`.

Rust deserializes the projection into `typed_operator_command::CommandPlan`. The pure admission core checks the schema, mechanism revision, paths, environment, input identity, limits, exit policy, and evidence policy before process creation.

Executable plans use the `command_plan` field. Legacy `command` strings can remain in receipts as diagnostic text. They cannot enter execution. ChaosControl does not split them into arguments.

The `replay-readiness-scheduler-receipt` Rust tool materializes packaged CI plans and individual command plans. Nix and the existing KVM smoke caller supply paths and ordered arguments. They do not own command-policy defaults or construct the typed DTO.

## Process mechanism

ChaosControl pins `bounded-exec` from the canonical Radicle Git endpoint at revision `29dac88ecded94457572db3fdfaaaab95fa91525`.

The reviewed revision has these properties:

- project ID: `rad:z2CpqLFpdP36fZXYUK5ZNWxMibpCo`
- source: `https://git.onix.computer/z2CpqLFpdP36fZXYUK5ZNWxMibpCo.git`
- license: `AGPL-3.0-or-later`
- Rust requirement: 1.85
- Unix process-group teardown support
- explicit limits for input, output, deadlines, polling, and teardown
- positive tests for environment, input, and accepted exits
- negative tests for malformed requests, floods, deadlines, cancellation, and descendant-held pipes

`deny.toml` admits only the reviewed bounded-exec crate names, AGPL license, and canonical Git source. `Cargo.lock` retains the exact revision.

`bounded-exec` owns process mechanics only. ChaosControl owns executable admission, identity checks, policy, diagnostics, receipts, and lifecycle decisions.

## Receipt facts

Each new scheduler run keeps the existing string `command` field as a non-executable display value. It also records `command_plan` and `command_observation`. The typed fields bind:

- the mechanism revision and command identity
- the observed executable identity
- completion and disposition classes
- exit code or signal
- cancellation state
- successful owned teardown
- bounded stdout and stderr sizes, retained BLAKE3 identities, and truncation facts

A successful disposition does not prove sandboxing, hermeticity, executable trust, child correctness, platform equivalence, orchestration correctness, or release eligibility.

## Checks

Run these focused checks from the repository root:

```text
nickel export contracts/evidence/examples/typed-operator-command.ncl
cargo test -p chaoscontrol-evidence typed_operator_command
cargo test -p chaoscontrol-evidence --test typed_operator_execution
cargo test -p chaoscontrol-evidence --test models
cargo run -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .
```

The tests cover literal shell metacharacters, accepted nonzero exits, legacy text, traversal, ambient environment, missing identity, malformed limits, output floods, timeouts, signals, cancellation classification, teardown policy, and evidence overclaim rejection.
