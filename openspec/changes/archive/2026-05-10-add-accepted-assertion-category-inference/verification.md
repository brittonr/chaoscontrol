# Verification

Captured 2026-05-10 for `add-accepted-assertion-category-inference`.

- `cargo test -p chaoscontrol-evidence --test models -- --nocapture` — pass, 35 tests.
- `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --check .` — pass.
- `cargo run -p chaoscontrol-evidence --bin check-assertion-readiness-promotion-gate -- .` — pass; accepted workload summaries show uncategorized=0 while unhit/non-passing gaps remain.
- `cargo run -p chaoscontrol-evidence --bin check-assertion-readiness-promotion-gate -- --selftest .` — pass.
- `cargo clippy -p chaoscontrol-evidence --all-targets -- -D warnings` — pass.
- `openspec validate add-accepted-assertion-category-inference --strict --json` — pass.
- `openspec validate assertion-catalog --strict --json` — pass.
- `git diff --check` — pass.
