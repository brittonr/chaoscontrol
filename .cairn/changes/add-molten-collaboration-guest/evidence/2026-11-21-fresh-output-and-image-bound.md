# Fresh signed output and guest image bound (2026-11-21)

## Fresh output

Aspen rebuilt and signed a consistent source binding:

```
/nix/store/9kh84csrlczijhwr04gg0xkvg0dxrl9y-molten-node-source-binding-v1
```

`molten node run` authenticated the full recorded closure and published:

```
node run startup=blake3:de93d08cc73c09e4c151f3526a9eecd83a45ea7975684004b0b688c98b7daa48 adapters=12
```

The real three-process driver then retained:

```
baseline observations=blake3:de2c103b33c9e0420972903386822e2477450bfe1f1d5dc4d5e3ecc19707434a
```

This clears the stale-signature blocker from the previous attempt. It is host
execution with an experimental signer, not a guest campaign.

## Guest image assembly bound

`scripts/build-molten-guest-image` was run against the fresh output with the
guest Nix daemon bootstrap. After more than 50 minutes it had copied about 37 GB
and 14,500 of the 21,247 top-level closure paths; the closure contains well over
a million files. The assembly was stopped rather than claim a completed image.

The image recipe preserves every recorded closure path because source admission
re-hashes all of them. A bounded guest campaign therefore needs an Aspen signed
output whose measured closure is runtime-only, or a materially faster store
provisioning path than per-file tar extraction.

## Boundary

No guest image or KVM campaign was completed. The guest Nix daemon, database
dump, store mount, and baseline-only driver are implemented but unexecuted in a
VM.
