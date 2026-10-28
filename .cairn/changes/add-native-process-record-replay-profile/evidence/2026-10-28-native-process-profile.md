# Native-process profile evidence — 2026-10-28

## Published surface

`crates/chaoscontrol-protocol/src/native_process.rs` adds:

- `NativeProcessProfile` — schema `chaoscontrol.native-process-replay-profile.v1`,
  version 1, producer cohort, record schema, replay class, fixed non-claims,
  and BLAKE3 identity over the canonical serialized profile.
- `NativeProcessRecord` — process key, generation, strictly increasing
  sequence, executable BLAKE3, start/end logical ticks, exit classification,
  and up to four BLAKE3 evidence references.
- `NativeProcessDescriptor` — descriptor schema
  `chaoscontrol.native-process-descriptor.v1`, the profile, and the ordered
  record set with BLAKE3 identity.

Bounds: 64 records, 256 text bytes per field, 4 evidence references per record.
Text fields reject host path fragments, secret-bearing words, and non-printable
bytes. Admission rejects stale schema/version/record-schema, a changed
non-claim set, empty or over-bound sets, non-BLAKE3 identities, reversed ticks,
duplicate or decreasing sequences, and drifted process order.

## Executed

```sh
nix develop -c cargo test -p chaoscontrol-protocol --lib --all-features
nix develop -c cargo clippy -p chaoscontrol-protocol --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Results: 47 passed, 0 failed (six native-process cases); strict Clippy and
formatting clean.

## Consumer boundary

Molten may attach a descriptor as an opaque diagnostic member. The descriptor
carries no semantic observation, does not become a logical world root, and
grants no authority, repair, voting, or equivalence claim. Validation proves
only bounded internal consistency and canonical identity over supplied facts.
