# Verification

Implementation evidence captured for assertion gap details:

- `cargo fmt --package chaoscontrol-evidence` — pass.
- `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --write .` — pass; regenerated `docs/assertion-readiness-status.md` with `## Gap details`.
- `cargo test -p chaoscontrol-evidence renders_committed_assertion_readiness_status -- --nocapture` — pass; 1 test passed.
- `cargo test -p chaoscontrol-evidence validates_committed_assertion_readiness_promotion_gate -- --nocapture` — pass; 1 test passed.

Additional final checks are run before archive/commit and reported in the session summary.
