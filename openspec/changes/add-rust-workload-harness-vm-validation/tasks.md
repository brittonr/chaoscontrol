## Phase 1: VM campaign evidence

- [ ] [serial] Run a bounded Rust workload VM campaign with enough time/build-cache budget to completion.
- [ ] [depends:vm-campaign] Inspect and record the output directory, classification receipt, and any replay/verdict artifact paths without promoting bounded campaign output to snapshot-backed replay proof.
- [ ] [depends:verification] Run strict OpenSpec validation, focused Nix/Rust checks if source changes are needed, and `git diff --check`.
- [ ] [depends:verification] Archive the completed VM validation OpenSpec change after evidence is captured.
