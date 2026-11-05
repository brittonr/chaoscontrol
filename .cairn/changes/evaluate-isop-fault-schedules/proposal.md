## Why

Interacting faults need a concise explanation, not only a reduced schedule. A
Minato–Morreale ISOP cover over schedule-entry inclusion bits can state which
combinations of faults are sufficient for one admitted failure in a fixed
parent/horizon, with positive and negative literals. GitHub issue
[#2](https://github.com/brittonr/chaoscontrol/issues/2) asks for a controlled
evaluation before any operator surface exists.

The production prerequisite is not met: branch-local outcome verification
([`add-counterfactual-replay-witnesses`](../add-counterfactual-replay-witnesses/proposal.md))
still owns classification, horizon accounting, and independent verification, and
its implementation tasks are open. This change therefore evaluates the algorithm
and checker on synthetic truth tables only and does not integrate with replay.

## What Changes

Add a detached experiment workspace
(`tools/isop-experiment`, excluded from the ChaosControl workspace) that:

1. Freezes a small ordered variable domain and a Boolean target outcome table.
2. Extracts prime implicants with Quine–McCluskey merging and selects an
   irredundant cover (not claimed minimum).
3. Checks the cover independently: cube sufficiency over all completions, coverage
   of every failing row, and irredundancy by removal.
4. Runs controlled fixtures for single/AND/OR faults, a negative literal with a
   non-monotone outcome, redundant alternatives, three-way consensus, constant
   true/false, duplicate same-time entries as distinct variables, and a function
   where no single fault suffices.
5. Injects broken covers (dropped cube, non-implicant, redundant cube) and asserts
   the checker rejects them, and verifies all 16 two-variable functions.
6. Retains a JSON report with per-case rows, covers, checker results, and costs.

## Consumer and Ownership

Consumer: future fault-interaction explanations attached to a retained failure.
Durable capability: a checked cover algorithm and independent verifier, reusable
once branch-local outcomes are admitted. Adoption path: no operator surface; a
gated follow-up may connect the checker to admitted schedule rows after the
counterfactual witness plan lands. Maintenance owner: chaoscontrol-explore.
Repeatability: the detached binary and its tests reproduce the report.

## Non-Goals

No KVM run, schedule execution, branch outcome, replay verdict, minimizer
replacement, causality claim, or coverage-reduction claim. No indeterminate-row
model, timeout/drift classification, budget enforcement, Nickel receipt, or
operator CLI. The cover is irredundant within one finite inclusion model, not a
unique cause, minimum test set, or claim about other timings.
