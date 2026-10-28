# Storage-fault campaign suite — 2026-10-28

Each directory entry is one bounded request and its receipt from
`storage-recovery-campaign`. The schedule identity is the BLAKE3 of the
canonical schedule JSON; the cohort receipt binds the final node identities,
safety decision, participants, non-claims, and oracle version.

| Scenario | Exit | Verdict | Receipt |
|---|---|---|---|
| flush-error | 0 | pass | `blake3:564ec1c0ed2e282c38d181ef7b154f1a0e86baaa65a895ff950ebfa953ac0f64` |
| write-error | 0 | pass | `blake3:2ebed8142c3ac36cc307f9a3312b5a272380de6634ab0f21ea847fa282524686` |
| corruption | 0 | pass | `blake3:532df488a51f2a6be5391c4aa75724c537c518bb375ecc85a726bffe54ab6a22` |
| restart | 0 | pass | `blake3:842cfa8bfb95ab7676789b64cf32454d05b3a6bec0fe831b80ee4c391cc642e4` |
| local-only | 0 | pass | `blake3:b65c729ee47e1e622620c8fa96b450452f08ca2e47cdf80c8d5da4b52aed1fca` |
| peer-repair | 0 | pass | `blake3:fa365b0c78a1c916d6ad8a5d20d90e2b9625ea8b5452c2f8ad49d13c43975315` |
| global-absence | 0 | pass | `blake3:490194b4376b743aa9a3e3276e5263bfb021815c026416f9b6755ecea3b5e60b` |
| incomplete-observation | 1 | incomplete | `blake3:c256cc980f82119e0211cf903086000b0fa4d770a94e10bd1a3d2985aedf4152` |

Flush-error and write-error receipts record a refused write with
`storage-flush-failed` and `storage-write-failed` respectively, then a valid
restart of the prior published record. Corruption records
`denied(storage-record-schema)` at restart. Incomplete observation exits 1 with
an `incomplete` verdict instead of a false pass.

Cache retention is a block-device property, not a node-disk campaign step; it
is covered by `chaoscontrol-vmm` `devices::block::tests::
stale_cache_survives_application_restart_but_not_reboot` and
`restart_profiles_distinguish_retained_cache_eviction_and_reboot`.

All results are deterministic model observations over supplied facts. They do
not prove guest kernel, filesystem, Redb, Raft, or Molten behavior, repair
authority, or release readiness.
