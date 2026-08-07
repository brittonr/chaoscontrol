## Why

ChaosControl has a typed operation-history fixture and a bounded single-register checker. The current checker orders operations by completion time and explicitly does not establish linearizability.

The active SMR chain change adds correct indefinite-outcome handling for one workload family. ChaosControl still lacks a generic event history and a real-time model checker for concurrent client operations.

A semantic history must preserve invocation, completion, uncertainty, and fault timing. A checker must return a bounded unknown result instead of reporting success when search or evidence is incomplete.

## What Changes

- Add a versioned semantic event-history format with separate invocation and completion events.
- Define `ok`, `fail`, `info`, and pending outcome semantics for distributed operations.
- Bind events, operations, clients, objects, workload profiles, and source artifacts with canonical BLAKE3 identities.
- Add a pure bounded linearizability checker with `valid`, `invalid`, and `unknown` verdicts.
- Add first-party read/write register and compare-and-swap models.
- Add independent-key decomposition, bounded search memoization, linearization witnesses, and reduced invalid histories.
- Add a compatibility reader for history v1 without promoting v1 reports into linearizability evidence.
- Add export and conformance fixtures for an independent Jepsen-compatible reference checker.
- Add semantic operation, fault, latency, and witness timelines to retained reports and the dashboard.

## Impact

- **Files**: `chaoscontrol-evidence`, workload adapters, fixtures, report generators, dashboard data, docs, and Nix checks.
- **Compatibility**: history and report v1 remain readable as legacy bounded evidence. New linearizability claims require v2.
- **Dependencies**: fault-event effect claims depend on `verify-fault-application-outcomes`. The SMR chain workload can adopt v2 after both packages stabilize.
- **Consumers**: OnixOS and Molten can invoke the checker as an external tool without linking host-side AGPL crates.
- **Claims**: a valid verdict means no model violation was found in the exact admitted finite history within declared bounds.

## Non-goals

- Do not replace deterministic replay, assertion checking, or the SMR chain validator.
- Do not implement transactional dependency-graph checking or reimplement Elle in this change.
- Do not claim checker soundness, system correctness, exhaustive schedule coverage, or production readiness.
- Do not treat fault selection as fault application or observation.
- Do not make Clojure or Jepsen a ChaosControl runtime dependency.
