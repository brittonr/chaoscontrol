## Tasks

- [x] [serial] Confirm that `extract_bugs` applies the cross-run verdict to one branch lineage and record the 2026-09-25 control-run counts. r[chaoscontrol.exploration_findings.safety_bugs]
- [ ] [depends:finding-baseline] Add the pure finding classifier with positive and negative cases for every assertion kind. r[chaoscontrol.exploration_findings.safety_bugs]
- [ ] [depends:finding-classifier] Route bug extraction through the classifier and add campaign-wide liveness aggregation. r[chaoscontrol.exploration_findings.safety_bugs] r[chaoscontrol.exploration_findings.campaign_liveness]
- [ ] [depends:finding-aggregation] Add coverage gaps and distinct failing-assertion counts to the report, assertion summary, CLI exit decision, and `scripts/bug-hunt.sh`. r[chaoscontrol.exploration_findings.campaign_liveness] r[chaoscontrol.exploration_findings.distinct_counts]
- [ ] [parallel] Audit `sometimes` assertions in the maintained workloads for safety properties encoded with the wrong kind. r[chaoscontrol.exploration_findings.compatibility]
- [ ] [depends:finding-report] Rerun the Raft control and planted variants, replay an existing safety bug, and run explorer tests and Cairn validation. r[chaoscontrol.exploration_findings.compatibility]
