# Protocol-observation cohorts

## Scope

This experimental contract transports bounded protocol projections and assembles exact cohorts.
ChaosControl owns structural admission, source accounting, scheduler linkage, stable identity, and bounded receipts.
The consumer owns protocol meaning and the independent oracle.

The contract does not authenticate guest process claims.
The VMM supplies the VM identifier, executing vCPU, exit sequence, and scheduler-state identity.
The profile checks the declared process, participant, producer, and generation against that VM.
A guest process reference is a declared identity, not a host process-authentication proof.

## Profile

`contracts/protocol-observation/profile.ncl` owns the typed configuration contract.
The profile binds one execution, protocol, projection schema, logical-boundary schema, participant set, oracle adapter, and resource budget.
Rust rejects unordered sets, stale schemas, malformed references, impossible bounds, and missing non-claims.
An oracle must match the declared adapter and projection schema before a campaign starts.

The fixture at `contracts/protocol-observation/fixtures/profile.ncl` uses synthetic identities.
It is not a production profile or third-party provenance.
The Nix contract check exports the fixture and compares the exact committed JSON.
Its output includes BLAKE3 identities for the contract, fixture source, and projection.

## Source records

The SDK exposes `ProtocolObservationEmitter` under its `full` feature.
One emitter owns one producer stream.
The emitter checks its counters, payload, and wire capacity before a transport effect.
A transport error consumes the source sequence and increases the loss count.
A final-drain attempt closes the emitter, including a failed final send.
The default emitter requires a VMM transport. A local log cannot acknowledge a host-bound observation.

Inline payloads use compact canonical JSON with sorted object keys.
An external projection reference can omit the bytes.
The consumer must resolve external bytes before semantic evaluation.
ChaosControl does not infer that an external reference contains a supported projection.

`CMD_PROTOCOL_OBSERVATION` is a dedicated command.
The ordinary event command rejects the reserved protocol event name.
The host checks exact framing, profile binding, per-producer limits, total bytes, boundaries, and backlog before retention.
Host rejections remain sticky accounting facts, including rejected suffixes after a source final record.
Fault-run resets cannot clear the protocol journal or its loss count.
Only an admitted snapshot restore can replace that execution history.
Free-form `OracleEvent` records do not enter this collection.

## Cohorts

`assemble_cohort` returns a bounded result or a typed admission error.
`assemble_with_losses` also binds host rejection accounting.
The result retains a canonical source journal and the records for one selected logical boundary.
The oracle receives only the selected admitted projections.

Exact repeated records collapse by full equality.
Any changed field at the same source sequence is a conflict.
Missing participants, source gaps, losses, and absent final drains prevent completeness.
Records after a final drain also prevent completeness.
An unsupported projection cannot hide malformed or incomplete source evidence.

Cross-producer arrival order has no semantic authority.
Scheduler positions bind host observations, not a protocol total order or one wall-clock instant.
The full cohort identity includes its journal, classification, issues, profile, and host loss count.
Every oracle, receipt, and replay boundary revalidates that identity and its derived fields.

## Consumer composition

`chaoscontrol_explore::protocol_observation::Session` is an explicit library composition.
It does not replace the default explorer CLI or the existing assertion guidance path.
A consumer supplies an admitted profile and its reviewed `ProtocolOracle` implementation.
`Session::configure` admits each VM before guest execution.
The caller retains kernel, guest artifact, controller, storage, and fault authority.

`Session::collect` joins bounded collections without removing VM origin or host rejection counts.
`Session::evaluate` records the consumer result without interpreting its meaning.
Incomplete cohorts do not invoke the oracle.
The adapter receives an explicit work limit and must keep its implementation pure and bounded.
The trait is a reviewed code contract, not a sandbox for arbitrary plugins.

`enrich_coverage` validates a complete cohort before it changes `CoverageBitmap`.
It returns every full novelty identity, including identities that share one compact slot.
A slot collision does not prove state equality.
Novelty comparisons are scoped to the exact selection profile.

The first-party Raft fixture checks one leader identity per declared term boundary.
Its independent comparison ignores the runtime pass field.
A conflicting projection therefore fails even when both participants self-report success.
This fixture does not prove complete Raft correctness.

## Markers and replay

`bind_marker_snapshot` requires an observation that declares the exact marker.
The binding includes the boundary, projection, record, cohort, scheduler state, and parent snapshot reference.
A reference alone does not establish restorability.

`Session::replay` loads the parent through the existing `SnapshotStore` contract.
It restores through `SimulationController`, runs a bounded continuation, and compares the complete rebuilt cohort.
The snapshot store retains its established SHA-256 artifact format for compatibility.
The new protocol link uses domain-separated BLAKE3 over the checked snapshot descriptor.
Missing files, invalid descriptors, wrong markers, stale cohorts, and replay drift cannot produce a successful comparison.

A pure linkage fixture does not prove a real guest continuation.
KVM execution evidence remains separate from pure fixture evidence.

## Evidence and non-claims

Receipts bind profiles, bounds, protocol schemas, participants, records, cohorts, consumer results, novelty, markers, scheduler states, faults, and replay references.
The referenced cohort retains source accounting and gap details.
`build_status` reports participant coverage, gaps, conflicts, oracle verdict, novelty, and marker linkage.
Its `identity-linked` marker state does not assert snapshot reachability or successful replay.

Passing evidence does not establish protocol semantics, universal correctness, production readiness, release eligibility, a total order, or synchronized wall clocks.
Snapshot restoration does not expand these claims.

## Checks

Run the focused tests:

```console
nix develop -c cargo test -p chaoscontrol-protocol -p chaoscontrol-vmm -p chaoscontrol-explore --features chaoscontrol-protocol/std --test protocol_observation
```

Run the contract and portable Nix checks:

```console
nix build .#checks.x86_64-linux.protocol-observation-contracts -L
nix build .#checks.x86_64-linux.protocol-observation-tests -L
```

These commands do not claim a passing full repository release gate.
The active Cairn change retains final validation and closeout evidence.
