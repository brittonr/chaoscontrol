# Proposal: Assertion-hit checkpoints

## Why

A reached `sometimes` or `reachable` assertion is the only bounded signal that a workload entered a state its author considered rare or interesting. Antithesis uses that signal twice: as a replay breakpoint and as a checkpoint from which a fraction of later runs restart, so rare states do not have to be rediscovered by chance.

ChaosControl records assertion state and already hashes it into coverage, and `SUT-declared event branching` shows a path from a guest-declared marker to a retained frontier parent. The current paths are separate. `crates/chaoscontrol-explore/src/explorer.rs` calls `enrich_with_assertion_state` for coverage, while frontier entries come from fault, input, and marker paths. No assertion hit retains a snapshot, and no campaign policy scores assertion-hit states as branch roots. A rare state that is reached but not otherwise branched remains a one-off observation.

This change makes assertion hits a first-class, bounded checkpoint source without changing assertion verdicts or liveness classification. `evaluate-sometimes-assertions-campaign-wide` retains ownership of campaign-wide liveness and bug classification.

## What Changes

- Expose exact catalog-bound assertion-hit observations at a deterministic vCPU boundary, carrying assertion identity, kind, verdict, guest/process owner, virtual tick, and the absence of intervening guest progress. r[chaoscontrol.assertion_checkpoints.observation]
- Add a pure, seed-bound selection policy that chooses a bounded number of hit checkpoints per run and campaign, combines novelty and prior hit rarity, and orders candidates deterministically. r[chaoscontrol.assertion_checkpoints.policy]
- Capture a snapshot at the selected post-hit boundary and bind it to the exact assertion identity, tick, run, seed, fault schedule, topology, and admitted catalog. r[chaoscontrol.assertion_checkpoints.capture]
- Feed captured checkpoints into the existing frontier as branch proposals with assertion provenance, deduplication, and restore validation. r[chaoscontrol.assertion_checkpoints.frontier]
- Emit bounded receipts covering selected, captured, skipped, duplicate, over-budget, and failed checkpoints without claiming complete coverage. r[chaoscontrol.assertion_checkpoints.evidence]
- Preserve the distinction between a checkpoint, a bug, and a liveness gap; no assertion result changes meaning because a checkpoint was or was not captured. r[chaoscontrol.assertion_checkpoints.nonclaims]
- Add positive and negative fixtures for rare-hit amplification, duplicate hits, budget exhaustion, missing snapshots, identity drift, and overclaim. r[chaoscontrol.assertion_checkpoints.fixtures]

## Impact

- **Core**: hit observation admission, selection policy, deterministic ordering, scoring, and receipt payload construction remain pure deterministic logic with no clock, file, process, or network access.
- **Shell**: the VMM and explorer shell own assertion-boundary observation, snapshot capture, storage, and frontier insertion.
- **Existing owners**: `evaluate-sometimes-assertions-campaign-wide` keeps finding classification; `sut-declared-event-branching` keeps marker semantics; snapshot and replay formats keep their current meaning.
- **Consumers**: the explorer, campaign report, readiness evidence, and local operator receive new checkpoint provenance without a new evidence class.

## Lifecycle Prerequisites

None. The existing snapshot store, controller, and frontier can carry the new proposal source. A failed or unavailable snapshot blocks that checkpoint, not the campaign definition.

## Out of Scope

- Changing assertion macro identity, catalog admission, verdict semantics, or liveness classification.
- Automatic bug creation from `sometimes` or `reachable` assertions.
- Exhaustive state-space coverage, causality analysis, or a claim that a captured checkpoint was the cause of a later finding.
- Hosted services, cross-machine scheduling, non-Rust guests, or a new snapshot format.

## Affected Specs

- `assertion-hit-checkpoints`: observation, policy, capture, frontier, evidence, non-claims, and fixtures.
