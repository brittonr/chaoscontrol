# SpaceWasm bundle remeasurement

Date: 2026-08-10

## Pinned producer

- Mantle revision: `a141fcbaafe41f9a413a81275a33fe915bfca370`
- SpaceWasm revision: `e24cf09355a90497148eb5029fdb8e3400bd63e3`
- Nix output: `/nix/store/fz5gj0rc5v2mq1psbdmyygg11a16mim7-mantle-spacewasm-reference-bundle-e24cf09355a90497148eb5029fdb8e3400bd63e3`
- Mantle materialization report disposition: `complete`
- Mantle source admission: `true`
- Mantle support-profile match: `true`
- Mantle check diagnostics: empty

## Measured identities

- Manifest BLAKE3: `39e4790a7b9d0b14fcafffe5810e268cd8af342d38d7e952a6ede923e33882b2`
- Bundle identity: `c4826bb63fa9eef1fa619e0f0c4c2c35dd10ca92a8d4999fec10c55e92b692b7`
- Host runner BLAKE3: `be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221`

The host runner identity and producer revisions did not change. The old consumer profile expected manifest `4ff6a779...` and bundle `cee7190f...`. The focused Nix rail rejected that stale pair before execution.

These facts admit only the selected bounded diagnostic cohort. They do not prove SpaceWasm correctness, runtime equivalence, WebAssembly conformance, sandbox effectiveness, or release eligibility.
