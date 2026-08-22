## Tasks

- [x] [serial] Confirm existing campaign, workload, and assertion-catalog surfaces that the corpus extends. r[chaoscontrol.benchmark.manifest]
- [ ] [depends:benchmark-baseline] Add the Nickel corpus manifest contract and exported BLAKE3-bound projection. r[chaoscontrol.benchmark.manifest]
- [ ] [depends:benchmark-manifest] Add the interleaving-race entry with positive and negative variants. r[chaoscontrol.benchmark.interleaving]
- [ ] [depends:benchmark-interleaving] Add the liveness entry with positive and negative variants. r[chaoscontrol.benchmark.liveness]
- [ ] [depends:benchmark-liveness] Add the rarity entry with a measured seeded base distribution. r[chaoscontrol.benchmark.rarity]
- [ ] [depends:protocol-observation-cohorts] Add the protocol-state entry with a coordinated cohort, independent oracle, stable novelty guidance, and positive and negative variants. r[chaoscontrol.benchmark.protocol_state]
- [ ] [depends:benchmark-rarity] [depends:benchmark-protocol-state] Add the bounded runner that asserts verdicts and emits binding receipts. r[chaoscontrol.benchmark.runner]
- [ ] [parallel] Add adversarial validation that every entry reproduces its expected verdict and that mismatches are typed. r[chaoscontrol.benchmark.validation]
- [ ] [depends:benchmark-validation] Run focused runner, receipt, Cairn, and relevant Nix validation. r[chaoscontrol.benchmark.validation]
