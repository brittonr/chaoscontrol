## Decision

Model the fixed parent/horizon as `f: {0,1}^n -> {true,false}` over ordered
inclusion variables, where `x_i = 1` means original schedule entry `i` is
included. Duplicate same-time entries remain distinct variables by construction.
Only admitted `true`/`false` rows are represented here; a production integration
must first classify indeterminate rows (timeout, early stop, unexercised target,
inherited-only observation, replay drift, missing artifact, budget exhaustion) and
refuse a complete cover while any required row is indeterminate.

Cover extraction: expand every failing minterm into a cube, merge cube pairs that
share the care mask and differ in exactly one constrained value, deduplicate, and
retain cubes that never merge as prime implicants. Selection is greedy over the
remaining primes (most newly covered failing rows, fewer literals as tie-break),
then each selected cube is dropped if the rest still cover every failing row. The
result is irredundant, not globally minimum.

Independent checker: for each cube, enumerate all completions in the domain and
require the frozen table to fail; require every failing row to be covered; require
each cube to own at least one failing row not covered by the rest. The checker
shares only the cube representation and `matches`, never the merge or selection
logic.

Fault-injected checks confirm the checker rejects an empty cover (coverage), a
non-implicant cube (sufficiency), and a redundant `TRUE` cube beside a full cover
(irredundancy). A permutation test confirms a wrong variable mapping changes the
outcome table, and all 16 functions of two variables produce verified covers.

## Measured result

Ten controlled cases, variables 2–4, matrix rows 4–16:

| Case | Failing rows | Cubes | Literals | Checker rows |
|---|---:|---:|---:|---:|
| single_fault | 4 | 1 | 1 | 4 |
| and_two_faults | 2 | 1 | 2 | 2 |
| or_two_faults | 6 | 2 | 2 | 8 |
| negative_literal_non_monotone | 1 | 1 | 2 | 1 |
| redundant_alternatives | 2 | 1 | 1 | 2 |
| three_way_consensus | 4 | 3 | 6 | 6 |
| constant_true | 4 | 1 (`TRUE`) | 0 | 4 |
| constant_false | 0 | 0 | 0 | 0 |
| duplicate_same_time_entries | 1 | 1 | 2 | 1 |
| no_single_fault_suffices | 7 | 2 | 4 | 8 |

For most cases the checker's cube-completion cost is below the exact matrix row
count; `or_two_faults` equals the matrix and `three_way_consensus` is 6 vs 8 rows.
The report is smaller than the row matrix whenever cubes merge, but the worst case
is exponential. The retained report is
`dogfood-results/isop-experiment-20261104/report.json`.

## Go/no-go

No-go for a normal operator surface now. The algorithm and checker are sound on
controlled tables, but the issue's own gate — independently verified branch-local
outcomes over a real schedule — is still open, and a cover without admitted rows
would be a diagnostic over an oracle that does not yet exist.

Go, conditional, for a gated follow-up after the counterfactual witness plan:
reuse this checker behind a typed contract that admits per-row outcomes from a
verified replay, refuses a complete cover when any required row is indeterminate,
and caps variables, rows, replay evaluations, work, and report size. That follow-up
owns the operator surface, Nickel receipt, and cost comparison on real schedules.

## Boundaries

Synthetic tables only. No branch evidence, KVM row, replay verdict, correctness,
causality, or release claim. Indeterminate outcomes and resource limits are not
modeled by this experiment. Cover irredundancy is within one finite inclusion
model; other timings, configurations, or interleavings are out of scope.
