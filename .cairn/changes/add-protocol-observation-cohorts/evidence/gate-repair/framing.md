# Exact integer framing

## Contract

Remove fabricated fallback lengths from selected existing hash and accounting helpers.
Keep all representable values, framing bytes, identity domains, and network accounting behavior unchanged.
This pass covers marker fields, fault fields and platform-sized values, and retained packet lengths.
It does not change the catalog-count or device-register policies that need separate admission review.

The selected helpers convert a platform-sized value to `u64`.
All supported target pointer widths fit that canonical field.
If a future target exceeds that width, the compile-time assertion rejects it.
The conversions also carry explicit invariant messages instead of substituted maxima.
No new fallible public API or identity version is necessary for the supported targets.

The preceding seven-package test run is the source baseline.
The budget covers these four conversions, direct positive and negative framing cases, and the affected regression checks.
Success requires exact framing, distinct field boundaries, unchanged empty accounting, and strict Clippy without a lint exception.
The pinned report must show the remaining findings rather than conceal them.

## Results

The seven-package tests and strict Clippy pass across all targets and all features.
The new cases preserve the exact little-endian prefix, distinguish ambiguous field boundaries, and retain zero-byte accounting for empty packets.
The fault cases also distinguish zero, the platform maximum, omitted fields, and framed sequences.
`framing-tests.log` and `framing-clippy.log` retain these results.

`framing-nix.log` passes the focused protocol, contract, vendor, dependency, and source-filter checks.
The same pinned Octet scope decreases from 1,766 to 1,762 findings, with zero errors.
Its config and profile hashes remain unchanged. The warning-only result still blocks strict acceptance.
