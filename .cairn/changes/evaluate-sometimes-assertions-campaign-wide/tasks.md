## Tasks

- [x] [serial] Confirm that `extract_bugs` applies the cross-run verdict to one branch lineage and record the 2026-09-25 control-run counts. r[chaoscontrol.exploration_findings.safety_bugs]
- [ ] [depends:finding-baseline] Add the pure finding classifier with positive and negative cases for every assertion kind. r[chaoscontrol.exploration_findings.safety_bugs]
- [x] [depends:finding-classifier] Route bug extraction through the classifier and add campaign-wide liveness aggregation. r[chaoscontrol.exploration_findings.safety_bugs] r[chaoscontrol.exploration_findings.campaign_liveness]
  - Evidence: `chaoscontrol_explore::findings::classify` is the single per-record classifier: `always` false and `unreachable` reached are safety violations; `sometimes`/`reachable` non-satisfaction is a liveness candidate; unexercised records are distinct. `Explorer::extract_bugs` now creates bug records only for safety violations and records every branch report into campaign aggregation. `campaign_liveness_gaps()` returns candidates no branch satisfied (satisfaction removes the candidate), and `distinct_failing_assertions()` counts distinct safety fingerprints. `campaign_liveness_and_distinct_safety_counts` proves an unsatisfied `sometimes` yields no bug or gap after a satisfying branch, while an observed `always` violation yields one bug and one distinct count.
- [x] [depends:finding-aggregation] Add coverage gaps and distinct failing-assertion counts to the report, assertion summary, CLI exit decision, and `scripts/bug-hunt.sh`. r[chaoscontrol.exploration_findings.campaign_liveness] r[chaoscontrol.exploration_findings.distinct_counts]
  - Evidence: `ExplorationReport` carries `campaign_liveness_gaps` and `distinct_failing_assertions`; `format_report` prints the distinct count and up to 16 liveness gaps with kind and hit count, keeping them out of the bug count. `AssertionSummaryV2` adds optional `liveness_gaps`/`distinct_failing_assertions` fields (serde defaults preserve old summaries) populated by `from_exploration`, with a round-trip test. The CLI exit decision still keys only on safety bug records, so liveness gaps do not change exit status. `scripts/bug-hunt.sh` now surfaces both lines in its per-variant summary. `liveness_gaps_and_distinct_counts_render_without_changing_bug_count` passes.
- [ ] [parallel] Audit `sometimes` assertions in the maintained workloads for safety properties encoded with the wrong kind. r[chaoscontrol.exploration_findings.compatibility]
- [ ] [depends:finding-report] Rerun the Raft control and planted variants, replay an existing safety bug, and run explorer tests and Cairn validation. r[chaoscontrol.exploration_findings.compatibility]

## Remaining work

Reporter, assertion-summary, CLI, and bug-hunt script surfaces for liveness gaps
and distinct counts are not implemented yet; the workload kind audit and the Raft
rerun require the maintained guest artifacts.
