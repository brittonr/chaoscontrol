## Tasks

- [x] [serial] Confirm existing round-log, workload, and verdict surfaces that the checker extends. r[chaoscontrol.phenomena.history]
- [ ] [depends:phenomena-baseline] Add the pure typed history and dependency model. r[chaoscontrol.phenomena.history]
- [ ] [depends:phenomena-history] Add the enumerated phenomena checks and cycle-detection classification. r[chaoscontrol.phenomena.checker]
- [ ] [depends:phenomena-checker] Add the bounded insufficient-data result for observation gaps. r[chaoscontrol.phenomena.incomplete]
- [ ] [depends:phenomena-incomplete] Add the shell that ingests round artifacts and validates history identities. r[chaoscontrol.phenomena.boundary]
- [ ] [depends:phenomena-shell] Bind phenomena evidence to history and operation identities with fail-closed validation. r[chaoscontrol.phenomena.evidence_binding]
- [ ] [parallel] Add positive fixtures for each named phenomenon and negative fixtures for clean and incomplete histories. r[chaoscontrol.phenomena.validation]
- [ ] [depends:phenomena-validation] Run focused core, shell, receipt, Cairn, and relevant Nix validation. r[chaoscontrol.phenomena.validation]
