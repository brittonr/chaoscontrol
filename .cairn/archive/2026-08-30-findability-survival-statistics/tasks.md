## Tasks

- [x] [serial] Confirm the existing round, verdict, and exploration-tree surfaces that this change extends. r[chaoscontrol.findability.observations]
- [x] [depends:findability-baseline] Add the pure typed observation model and first-bug-per-subtree assembly. r[chaoscontrol.findability.observations]
- [x] [depends:findability-observations] Add the pure exponential fit with mean time-to-bug and explicit no-bug result. r[chaoscontrol.findability.model]
- [x] [depends:findability-model] Add the conservative Lomax posterior, confidence projection, and stated assumptions. r[chaoscontrol.findability.confidence]
- [x] [depends:findability-confidence] Add the independence check and the baked-in-bug flag. r[chaoscontrol.findability.independence]
- [x] [depends:findability-independence] Add the shell that assembles observations from round artifacts and validates identities. r[chaoscontrol.findability.boundary]
- [x] [depends:findability-shell] Bind findability reports to observation and model identities with fail-closed validation. r[chaoscontrol.findability.evidence_binding]
- [x] [parallel] Add positive known-probability fixtures and negative empty, single-observation, no-bug, and baked-in fixtures. r[chaoscontrol.findability.validation]
- [x] [depends:findability-validation] Run focused core, shell, receipt, Cairn, and relevant Nix validation. r[chaoscontrol.findability.validation]
