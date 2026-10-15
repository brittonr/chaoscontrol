# Explorer Repeatability Specification

## Purpose

Make an exploration run repeatable for identical inputs, and make any divergence locatable.

## ADDED Requirements

### Requirement: Branches record comparable digests

r[chaoscontrol.explorer_repeatability.branch_digest] Every completed branch MUST record a BLAKE3 digest over its coverage bitmap, canonical oracle report, per-VM exit counts, total ticks, and schedule fingerprint. Round history MUST keep the digests in branch-index order.

#### Scenario: Branch results differ in one exit count

- GIVEN two branch results that differ only in one VM exit count
- WHEN their digests are computed
- THEN the digests MUST differ.

### Requirement: Explorer runs are gated for repeatability

r[chaoscontrol.explorer_repeatability.gate] Two runs with the same seed, configuration, kernel, initrd, and explorer build MUST produce equal per-round branch digests, frontier selections, and bug identities. The gate MUST report the first mismatching round and branch.

#### Scenario: Identical runs match

- GIVEN two fresh-process runs of a maintained gate case with identical inputs
- WHEN the gate compares their histories
- THEN every round MUST match.

#### Scenario: One branch diverges

- GIVEN two histories that differ in round 1 branch 2
- WHEN the gate compares them
- THEN it MUST fail and name round 1 branch 2 and the differing digest components.

### Requirement: The divergence cause is removed

r[chaoscontrol.explorer_repeatability.cause] Every host-dependent input found to change guest-visible execution between identical runs MUST be removed or made an explicit recorded input, with a focused regression for each.

#### Scenario: Diagnosed cause has a regression

- GIVEN a diagnosed host-dependent input
- WHEN it is fixed
- THEN a regression MUST fail before the fix and pass after it.

### Requirement: Operational runs are excluded

r[chaoscontrol.explorer_repeatability.operational_exclusion] A run with host watchdog firings, discarded branches, or failed branches MUST NOT support a repeatability claim. The gate MUST report it as operational rather than as a match or mismatch.

#### Scenario: Watchdog fired in one run

- GIVEN two runs in which one recorded a host watchdog firing
- WHEN the gate evaluates them
- THEN the result MUST be operational, not a repeatability pass.
