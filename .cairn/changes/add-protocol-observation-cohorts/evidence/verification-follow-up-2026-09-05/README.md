# Protocol replay follow-up

## Publication boundary

The user requested an unfinished checkpoint commit and push, then continued work.
Checkpoint `c50e01cfca4b9e441587a570bd7ed9fc37bdb558` is on `origin/drain/protocol-observation-cohorts-20260904`.
`origin/main` remains at `31300fa1a2d29c7496e8316f065c156f80343143` at the checkpoint fetch.
The change remains active. This evidence does not authorize archive or main integration.

## Executed continuation

The new fixture uses the production `SimulationController`, `FileSnapshotStore`, and `Session::replay`.
Each guest copies an SDK-produced opaque frame into its shared page and emits port I/O.
It then increments a guest-memory counter. The bounded controller slice stops in NOP padding before HLT.

The positive case changes the live counter before each replay.
Replay must load the stored parent, restore memory, resume guest instructions, and reproduce the exact cohort.
The counter must return to one, and the instruction pointer must remain inside the declared stop range.
This distinguishes actual restore and continuation from stale journal reuse.

The negative cases reject stale markers, wrong cohorts, stale parents, zero ticks, incomplete expected cohorts, missing snapshots, corrupted snapshots, and malformed ELF input.
They also run a restorable parent whose guest frame has invalid flags.
That continuation remains incomplete and cannot produce a successful replay result.

The explicit KVM invocation passed all four cases in `replay-kvm.log`.
The ordinary package test target does not run these ignored KVM cases automatically.
The fixture does not establish Linux boot, in-guest SDK initialization, protocol correctness, general halt behavior, or release readiness.

## Fixture corrections

The first execution bound stopped before the frame copy because the VMM also single-steps single-vCPU guests.
Byte-copy, PIT-state, and word-copy attempts then exceeded the external deadline.
The retained step trace reached the frame copy, port I/O, counter increment, and HLT.
The next step stalled after the instruction pointer passed HLT.

The final fixture uses word copies and a bounded pre-halt stop range.
It removes the diagnostic stepping and unsuccessful PIT-state override.
No VMM halt, interrupt, scheduler, or watchdog behavior changed.
Failed and timed-out attempts remain under `../attempts/replay-follow-up/`.

## Repository checks

The guest-probe constructor now pins every field of its reviewed CPU, memory, boot, and scheduler configuration.
Both probe entry points retain the complete seed and hide TSC.
Four probe tests pass, including malformed input, seed-width boundaries, and exact comparison with the previous configuration.

The product-scope validator found ten missing intents.
Each new intent comes from its existing active proposal and retains publication, parity, and authority limits.
No capability state or guard rule changed. The generated document checks pass.

| Check | Status |
| --- | --- |
| Four explicit KVM replay cases | Passed |
| Guest-probe tests | Passed |
| Product-scope documents and registry | Passed |
| Seven-package all-target, all-feature tests | Passed, `all-target-tests.log` |
| Seven-package strict Clippy | Passed, `clippy.log` |
| Scoped Nix tests and contracts | Passed, `nix-focused.log` |
| Pinned Octet check | Warning-only, 2,458 warnings and zero errors, `octet.log` |
| Cairn validation and gates | Passed, all three gate verdicts are `PASS` |

The final replay rerun also passes all four cases in `replay-final.log`.
Each final command has a separate `.exit` file so queue-history cleanup cannot remove its exit status.
The Octet result is not a strict error-gate acceptance result. The quality task remains open.

## Cargo metadata blocker

The locked offline metadata call reproduces the panic at `package_id_spec.rs:248:40`.
The local package-ID control succeeds. The `vm-cohort-core` control fails with the same panic.
Cargo commit `797e8a9bc` formats package IDs through an unchecked URL path-segment lookup:

<https://github.com/rust-lang/cargo/blob/797e8a9bc/crates/cargo-util-schemas/src/core/package_id_spec.rs#L248>

The VM Cohort dependencies use the pinned pathless URL `rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3`.
The trailing-slash transport probe fails with `invalid remote url: namespace: invalid multibase string: Invalid base string`.
Thus a consumer-only slash change is not a valid repair for this Cargo and Radicle pair.

The VM Cohort revision remains `ab123e3673b6dd616b3df5d044026b5e85755149`.
The dependency policy remains enabled. No pin, source authority, or `flake.lock` entry changed.
A compatible formatter or a verified published transport must resolve this boundary before the full dependency gate can pass.
