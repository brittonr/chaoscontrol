## Tasks

- [x] [serial] Confirm the existing replay, snapshot, and fault-schedule ddmin surfaces that this change extends. r[chaoscontrol.causality.minimization_core]
- [x] [depends:causality-baseline] Add the pure interleaving-minimization core over replay outcomes. r[chaoscontrol.causality.minimization_core]
- [x] [depends:causality-minimize] Add the pure candidate-ranking core for attribution. r[chaoscontrol.causality.attribution]
- [x] [depends:causality-rank] Add the shell that reads artifacts, drives candidate executions, and enforces budgets. r[chaoscontrol.causality.budget] r[chaoscontrol.causality.boundary]
- [x] [depends:causality-shell] Bind analysis artifacts to replay verdicts and snapshot identities with fail-closed validation. r[chaoscontrol.causality.evidence_binding]
- [x] [parallel] Add positive attribution and minimization fixtures and negative budget, ranking, identity, and non-reproducing fixtures. r[chaoscontrol.causality.validation]
- [x] [depends:causality-validation] Run focused core, shell, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.causality.validation]
