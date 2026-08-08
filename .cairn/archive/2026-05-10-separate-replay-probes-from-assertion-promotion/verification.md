# Verification

- `cargo test -p chaoscontrol-evidence --test models -- --nocapture` — pass.
- `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --check .` — pass.
- `cargo run -p chaoscontrol-evidence --bin check-assertion-readiness-promotion-gate -- .` — pass; all accepted workloads show `nonpassing=0 replay_probe_failures=1`.
- `cargo run -p chaoscontrol-evidence --bin check-assertion-readiness-promotion-gate -- --selftest .` — pass; includes hidden replay-probe signal negative case.
- `cargo clippy -p chaoscontrol-evidence --all-targets -- -D warnings` — pass.
- `openspec validate separate-replay-probes-from-assertion-promotion --strict --json` — pass.
- `openspec validate --all --strict --json` — pass.
- `git diff --check` — pass.
