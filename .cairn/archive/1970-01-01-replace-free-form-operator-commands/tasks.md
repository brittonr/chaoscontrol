## Tasks

- [x] [serial] Define typed command facts, evidence identity, legacy handling, and the `bounded-exec` ownership boundary. r[chaoscontrol.typed_operator_commands.plan] r[chaoscontrol.typed_operator_commands.boundary]
- [x] [depends:typed-command-foundation] Pin and validate the exact published `bounded-exec` revision without sibling-path dependency use. r[chaoscontrol.typed_operator_commands.mechanism]
- [x] [depends:typed-command-foundation] Add the typed Nickel command-plan contract and deterministic Rust DTO projection. r[chaoscontrol.typed_operator_commands.plan]
- [x] [depends:typed-command-profile] Implement pure plan admission, path, environment, limit, identity, and outcome classification. r[chaoscontrol.typed_operator_commands.functional_core]
- [x] [depends:bounded-exec-adoption] Replace `sh -c` execution with direct typed `bounded-exec` requests. r[chaoscontrol.typed_operator_commands.execution]
- [x] [depends:typed-command-execution] Bind command facts, mechanism revision, truncation, timeout, teardown, exit, and artifact observations into receipts. r[chaoscontrol.typed_operator_commands.evidence]
- [x] [parallel] Add positive literal-argument and accepted-exit cases plus negative legacy, traversal, environment, missing identity, timeout, flood, signal, teardown, and overclaim cases. r[chaoscontrol.typed_operator_commands.validation]
- [x] [depends:typed-command-migration] Remove free-form execution fields after all admitted plans use typed requests. r[chaoscontrol.typed_operator_commands.legacy]
- [x] [depends:typed-command-validation] Run focused Rust, Nickel, mechanism, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.typed_operator_commands.validation]
