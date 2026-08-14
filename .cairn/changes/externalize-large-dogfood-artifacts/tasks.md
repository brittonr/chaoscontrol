## Tasks

- [x] [serial] Define storage-neutral BLAKE3 object references, retention states, two-phase migration, and bounded non-claims. r[chaoscontrol.dogfood_artifacts.object_ref] r[chaoscontrol.dogfood_artifacts.boundary]
- [ ] [depends:artifact-foundation] Inventory tracked payloads, live manifest references, duplicate cohorts, and diagnostic-only artifacts. r[chaoscontrol.dogfood_artifacts.inventory]
- [ ] [depends:artifact-foundation] Add the typed Nickel retention and storage-adapter policy. r[chaoscontrol.dogfood_artifacts.retention]
- [ ] [depends:artifact-policy] Implement pure object-reference, linkage, size, digest, role, duplicate, and deletion admission. r[chaoscontrol.dogfood_artifacts.functional_core]
- [ ] [depends:artifact-core] Implement bounded staging and materialization shells for the selected storage adapter. r[chaoscontrol.dogfood_artifacts.materialization]
- [ ] [depends:artifact-materializer] Add references for live accepted and selected diagnostic cohorts while tracked blobs remain. r[chaoscontrol.dogfood_artifacts.migration]
- [ ] [parallel] Add positive exact materialization and negative missing, corrupt, truncated, wrong-size, wrong-role, unsafe-path, duplicate, unavailable, and deletion-blocked cases. r[chaoscontrol.dogfood_artifacts.validation]
- [ ] [depends:artifact-dual-validation] Remove tracked large payloads only after every live reference and readiness gate passes. r[chaoscontrol.dogfood_artifacts.migration]
- [ ] [depends:artifact-migration] Add tracked-size, duplicate-retention, raw-log, and manifest freshness gates. r[chaoscontrol.dogfood_artifacts.retention] r[chaoscontrol.dogfood_artifacts.validation]
- [ ] [depends:artifact-validation] Run focused core, materialization, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.dogfood_artifacts.validation]
