## Why

The workspace has focused KVM tests and local dogfood commands, but the main CI workflow does not produce one required KVM release verdict. Unsupported or skipped KVM behavior can remain hidden behind broad pure checks.

## What Changes

- Define a typed required KVM release matrix.
- Run the matrix on an admitted KVM-capable worker.
- Bind host capabilities, source, binaries, guests, profiles, commands, and results into one receipt.
- Keep pure CI and KVM behavior evidence separate.
- Block release eligibility when a required row is missing, stale, skipped, or failed.

## Impact

- **Configuration**: Nickel KVM matrix and worker capability contract.
- **CI**: a trusted KVM job with bounded artifacts and explicit retention.
- **Code**: pure matrix admission and receipt classification plus a thin runner shell.
- **Testing**: positive complete matrix and negative missing, stale, unsupported, skipped, tampered, and overclaim cases.

## Non-Goals

- No universal host or kernel equivalence claim.
- No hosted KVM service.
- No replacement for pure workspace validation.
