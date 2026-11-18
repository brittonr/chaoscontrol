## Tasks

- [x] [serial] Define the typed Nickel full-ABI differential profile: Mantle bundle reference, patched Wasmtime 49.0.5 embedding cohort, exact `molten.wasm.abi.v1` surface, scenario classes, named bounds and non-claims, with positive and negative profile fixtures. r[chaoscontrol.spacewasm_full_abi.profile]
  - See [first slice](evidence/first-slice-2026-11-18.md#profile-contract).
- [x] [depends:full-abi-profile] Implement pure full-ABI module admission for the exact hostcall imports, single bounded memory, absent table/start/`memory.grow` and the four ABI exports, with positive and negative module tests. r[chaoscontrol.spacewasm_full_abi.abi] r[chaoscontrol.spacewasm_full_abi.functional_core]
  - See [first slice](evidence/first-slice-2026-11-18.md#pure-admission).
- [ ] [depends:full-abi-profile] Implement the pure deterministic hostcall model: canonical request validation, declared replies, reply placement, pause requests and ordered transcripts. r[chaoscontrol.spacewasm_full_abi.host] r[chaoscontrol.spacewasm_full_abi.functional_core]
- [ ] [depends:full-abi-host-model] Add the Rust harness that embeds the patched Wasmtime 49.0.5 cohort and serves the hostcall model with explicit fuel and no other imports. r[chaoscontrol.spacewasm_full_abi.host]
- [ ] [depends:full-abi-host-model] Add the consumer-owned SpaceWasm full-ABI host, built offline from the bundle source closure, serving the same hostcall model. r[chaoscontrol.spacewasm_full_abi.host]
- [ ] [depends:full-abi-profile] Admit the Mantle bundle for evidence only through a release-authority publication receipt, and classify local builds as diagnostic-only. r[chaoscontrol.spacewasm_full_abi.materialization]
- [ ] [depends:full-abi-engines] Run the positive and negative ABI, hostcall, memory, trap, stack and cumulative-fuel scenarios against the exact bundle member bytes, with segmented-versus-uninterrupted fuel comparison. r[chaoscontrol.spacewasm_full_abi.scenarios] r[chaoscontrol.spacewasm_full_abi.fuel]
- [ ] [depends:full-abi-scenarios] Add full-ABI report contracts and overclaim rejection, separate from VM replay and assertion-readiness classes. r[chaoscontrol.spacewasm_full_abi.evidence]
- [ ] [depends:full-abi-evidence] Run focused tests, the full-ABI Nix check, evidence contracts, Cairn validation and the proposal, design and tasks gates. r[chaoscontrol.spacewasm_full_abi.validation]

## Blockers

- Evidence-bearing scenarios need release authority to publish the Mantle `materialize-molten-spacewasm-abi` revision and bind its bundle identity. That authorization has not been given; until then only pure and diagnostic results may be recorded.
