# Validation evidence

Date: 2026-10-30. Source: worktree `cairn/molten-contracts-20261029` (published tree without the SpaceWasm rail) on base `1caeda2334f1d93e53ce22afbfa012b05458f606`, run in `nix develop` with build, temp, and cache directories under `/home/brittonr/.cargo-target/`.

## Checks

| Command | Result |
| --- | --- |
| `cargo test -p chaoscontrol-evidence --lib semantic` | exit 0; 21 passed, 0 failed (includes the provider contract mutation, version, and completion-metadata tests) |
| `semantic-history-contract selftest` | exit 0 |
| `semantic-history-contract fixtures DIR` | exit 0; wrote `valid`, `invalid`, `unknown-model`, `unknown-pending`, `unknown-bound` history/report pairs and `legacy-history.json`, `legacy-report.json`, `legacy-digest.txt` |
| `validate` on each retained fixture pair | exit 0 for all five classes and the legacy pair |
| `check` then `validate` on each fixture history | exit 0; verdicts `valid`, `invalid`, `unknown`, `unknown`, `unknown` |
| `import` then `check` of `crates/chaoscontrol-evidence/fixtures/semantic-history-register.json` | exit 0; verdict `valid` |
| `cargo fmt --all -- --check`; crate Clippy with `--deny warnings` | exit 0 |
| `cargo test --no-fail-fast` (workspace) | exit 0; 3104 passed, 0 failed, 51 ignored |

## Negative controls

| Control | Result |
| --- | --- |
| `invalid` report rewritten to `verdict = valid` with proofs removed | exit 1: `Outcome: report proof does not match deterministic bounded checker readback` |
| `valid` report with first witness `state_before` altered | exit 1: `Outcome: report proof does not match deterministic bounded checker readback` |
| `valid` history with changed `source_artifact` | exit 1: `Identity: invocation source_artifact must match the history BLAKE3 identity` |
| history with `schema_version` 3 | exit 1: `Schema: unsupported or missing history schema_version` |

The script that ran these controls was `/home/brittonr/.cargo-target/publish-chaos-20261030/semantic.sh`.

## Evidence boundary

Fixtures are synthetic checker inputs. They are not guest observations, execution receipts, or reference-tool execution evidence. A checked report applies only to the admitted finite history, model, and bounds.
