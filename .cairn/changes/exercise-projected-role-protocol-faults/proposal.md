# Proposal: Exercise projected role-protocol faults

## Summary

Add bounded deterministic campaigns for one frozen Choregraph and Lattice role-protocol cohort. Exercise transfer, choice, restart, partition, loss, duplication, reordering, corruption, delay, and heal behavior while checking protocol-specific safety assertions.

Use independent expected-outcome fixtures. Do not use the Lattice runtime under test as the only oracle.

## Motivation

Choregraph can establish projectability and exact local protocol shape. Lattice can persist and execute those local protocols. Neither result establishes behavior under transport faults, process death, stale artifacts, or incomplete observation.

ChaosControl already owns deterministic network transitions, process and VM fault schedules, snapshots, replay, assertion catalogs, and evidence classification. It needs a protocol-specific campaign that keeps compiler, runtime, and simulator claims separate.

## Scope

- Consume immutable Choregraph global and local artifacts plus the matching Lattice runtime cohort through frozen adapters.
- Define a typed Nickel campaign profile for roles, placements, protocol cases, fault schedules, assertions, observations, bounds, artifacts, and non-claims.
- Maintain independent expected-outcome fixtures for valid transfer, valid choice, stale label, duplicate message, wrong role, wrong step, uncertain dispatch, and recovery.
- Exercise deterministic loss, delay, duplication, reordering, corruption, partition, heal, role restart, and selected crash windows.
- Assert that wrong-session, wrong-role, wrong-step, duplicate, reordered, stale-label, and former-owner actions never advance forbidden protocol state.
- Separate expected blocking, explicit unknown outcome, terminal failure, assertion violation, partial observation, unsupported case, and successful bounded completion.
- Bind exact campaign, producer, runtime, guest, schedule, observation, snapshot, replay, and receipt identities with BLAKE3.
- Add a cheap pure and in-process rail plus a separate KVM behavior rail.

## Success Criteria

- One valid transfer and one labeled choice complete without injected faults.
- Duplicated or reordered envelopes do not advance a role twice.
- Stale labels do not select another branch.
- A former owner cannot use a value after an admitted committed transfer.
- A partition or crash produces bounded blocked or unknown evidence, not false success.
- A healed path can complete only through the runtime recovery rules selected by the campaign.
- Snapshot-backed replay reproduces at least one selected fault outcome with exact artifact identities.

## False Completion

Packet loss without protocol assertions is not completion. A workload that only checks process exit status is not completion.

A campaign that derives expected outcomes only from the Lattice runtime under test is not completion. Missing KVM or replay artifacts cannot be reported as passing behavior evidence.

## Dependencies

- Choregraph change `add-role-choreography-projection` must publish immutable protocol artifacts and projection receipts.
- Lattice change `execute-projected-role-protocols` must publish one frozen runtime, persistence, envelope, and outcome cohort.
- Existing ChaosControl network, process-restart, snapshot, replay, assertion, observation, and evidence contracts remain authoritative.

## Non-Goals

- Defining choreography semantics, Lattice runtime policy, OnixOS placement, transport security, or external-role authority.
- Proving universal deadlock freedom, exactly-once delivery, external process correctness, physical network behavior, production availability, or release eligibility.
- Treating simulator or KVM evidence as proof of arbitrary hosts, kernels, NICs, switches, or transports.

## Affected Specs

- `role-protocol-fault-campaign`: profiles, frozen adapters, independent outcomes, assertions, faults, observations, replay, evidence, boundaries, and validation.
