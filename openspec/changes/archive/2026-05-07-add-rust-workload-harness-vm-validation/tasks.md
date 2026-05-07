## Phase 1: VM campaign evidence

- [x] [serial] Run a bounded Rust workload VM campaign with enough time/build-cache budget to completion.
- [x] [depends:vm-campaign] Inspect and record the output directory, classification receipt, and any replay/verdict artifact paths without promoting bounded campaign output to snapshot-backed replay proof.
- [x] [depends:verification] Run strict OpenSpec validation, focused Nix/Rust checks if source changes are needed, and `git diff --check`.
- [x] [depends:verification] Archive the completed VM validation OpenSpec change after evidence is captured.
