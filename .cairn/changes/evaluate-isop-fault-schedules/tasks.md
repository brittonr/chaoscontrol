## Tasks

- [x] [serial] Record the model, ordering, indeterminate-row boundary, prerequisite gate, and independent-check requirements. r[chaoscontrol.isop_experiment.cover_soundness]
- [x] [serial] Implement prime extraction, irredundant selection, and the independent sufficiency/coverage/irredundancy checker in a detached workspace. r[chaoscontrol.isop_experiment.cover_soundness]
- [x] [serial] Run controlled fixtures and fault-injected broken covers; verify all two-variable functions and a variable-permutation negative. r[chaoscontrol.isop_experiment.cover_soundness]
- [x] [serial] Retain the JSON report and compare checker rows, cube counts, and literals against the exact matrix. r[chaoscontrol.isop_experiment.cover_soundness]
- [x] [serial] Record the go/no-go decision: no operator surface now, conditional follow-up after verified branch outcomes. r[chaoscontrol.isop_experiment.gated_integration]
- [ ] [serial] Integrate the checker behind a typed contract that admits verified branch-local outcomes and refuses complete covers with indeterminate rows. r[chaoscontrol.isop_experiment.gated_integration]
- [ ] [serial] Add caps for variables, rows, replay evaluations, work, report size, and write the Nickel receipt/operator surface after the gate clears. r[chaoscontrol.isop_experiment.gated_integration]

## Evidence

Detached workspace `tools/isop-experiment` (excluded from the ChaosControl
workspace). Ten controlled cases, four fault-injected covers, a permutation
negative, and all 16 two-variable functions pass. Retained report:
`dogfood-results/isop-experiment-20261104/report.json`. Checker rows were at or
below matrix rows for most cases (or_two equals it; consensus 6 vs 8); the worst
case remains exponential.

Evidence class: controlled synthetic truth-table evaluation only. No branch
outcome, KVM row, replay verdict, receipt, operator surface, causality,
correctness, or release claim. Integration stays gated on
`add-counterfactual-replay-witnesses`.
