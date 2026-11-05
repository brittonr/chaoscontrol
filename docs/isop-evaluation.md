# ISOP experiment for finite fault schedules

Controlled evaluation for issue [#2](https://github.com/brittonr/chaoscontrol/issues/2).
It asks whether a Minato–Morreale irredundant sum-of-products (ISOP) cover can
concisely explain interacting faults over schedule-entry inclusion bits, with an
independent check, before any operator surface exists.

This is a synthetic truth-table experiment. It does **not** execute a VM, admit a
replay row, or integrate with the minimizer. The production gate — independently
verified branch-local outcomes — is owned by
[`add-counterfactual-replay-witnesses`](../.cairn/changes/add-counterfactual-replay-witnesses/proposal.md)
and remains open.

## Model

One small, fixed ordered schedule and one parent/horizon define Boolean variables
`x_i` for inclusion of original entry `i`. Equal-time entries stay distinct
variables. `f(x)` is the admitted target outcome; only `true`/`false` rows are
modeled here. A production classifier must map timeout, early stop, unexercised
target, inherited-only observations, replay drift, missing artifact, and exhausted
budget to *indeterminate* and refuse a complete cover while any required row is
indeterminate.

## Method

Harness: [`tools/isop-experiment`](../tools/isop-experiment) (detached workspace,
excluded from ChaosControl).

1. Expand failing minterms into cubes, merge cubes that differ in exactly one
   constrained value, and keep never-merged cubes as prime implicants.
2. Select an irredundant cover greedily (most newly covered failures, fewer
   literals as tie-break) and drop any cube whose removal keeps coverage.
3. Check independently: every cube sufficient over all completions, every failing
   row covered, every cube uniquely owns at least one row. The checker shares only
   the cube match rule with the extractor.
4. Fault-inject: empty cover (coverage), non-implicant cube (sufficiency),
   redundant `TRUE` cube (irredundancy). Verify all 16 functions of two variables
   and a variable-permutation negative.

```bash
cd tools/isop-experiment
cargo test --release
cargo run --release   # JSON report
```

## Results

Retained report:
[`dogfood-results/isop-experiment-20261104/report.json`](../dogfood-results/isop-experiment-20261104/report.json).

| Case | Variables | Failing rows | Cubes | Literals | Checker rows |
|---|---:|---:|---:|---:|---:|
| single_fault | 3 | 4 | 1 | 1 | 4 |
| and_two_faults | 3 | 2 | 1 | 2 | 2 |
| or_two_faults | 3 | 6 | 2 | 2 | 8 |
| negative_literal_non_monotone | 2 | 1 | 1 | 2 | 1 |
| redundant_alternatives | 2 | 2 | 1 | 1 | 2 |
| three_way_consensus | 3 | 4 | 3 | 6 | 6 |
| constant_true | 2 | 4 | 1 (`TRUE`) | 0 | 4 |
| constant_false | 2 | 0 | 0 | 0 | 0 |
| duplicate_same_time_entries | 2 | 1 | 1 | 2 | 1 |
| no_single_fault_suffices | 4 | 7 | 2 | 4 | 8 |

Every cover passed all three checks. For most cases the checker's cube-completion
cost is below the exact matrix row count; `or_two_faults` equals it and
`three_way_consensus` is 6 rows vs 8. The report is smaller than the row matrix
whenever cubes merge, but the worst case is exponential, and verification reruns
must be charged when real rows are used.

## Decision

**No-go for a normal operator surface now.** The algorithm and checker behave as
specified on controlled tables, but a cover without admitted branch-local rows
would explain an oracle that does not exist yet.

**Conditional go** for a gated follow-up once the counterfactual witness plan
lands: reuse the checker behind a typed contract that admits verified per-row
outcomes, refuses a complete cover when any required row is indeterminate, caps
variables/rows/replay evaluations/work/report size, and writes the Nickel receipt.
That follow-up owns the operator surface and the cost comparison on real
schedules.

## Non-claims

Synthetic tables only. Indeterminate outcomes, timeouts, replay drift, and resource
exhaustion are not modeled. The cover is irredundant within one finite inclusion
model, not a unique cause, minimum test set, causality claim, or statement about
other timings, configurations, or interleavings. No KVM, schedule, replay, or
release-readiness evidence is produced.
