## Why

ChaosControl now has accepted snapshot-backed replay proofs for Raft and redb, but operator-facing status still depends on prose spread across README/docs. Antithesis-alternative claims need a generated readiness boundary that separates supported, experimental, and unproven surfaces from the accepted workload manifest.

## What Changes

- Add a generated replay readiness status report sourced from the accepted workload proof manifest.
- Add a check that fails if the report is stale or overclaims unsupported surfaces.
- Wire the check into the existing evidence-contracts gate so proof coverage and readiness status move together.

## Non-Goals

- No new workload proof.
- No mathematical determinism proof.
- No broad production readiness claim for arbitrary guests, device models, or all replay paths.

## Verification

Run the generated report check, existing proof coverage check, evidence contract checks, and the Nix evidence-contracts check.
