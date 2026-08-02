## Phase 1: Authority and path inventory

- [ ] [serial] Inventory every repository-owned crate, tool, template, fixture, generated source path, lifecycle path, and documentation surface. r[chaoscontrol.unified_agpl.scope]
- [ ] [serial] Record third-party, upstream-derived, unknown-authority, and earlier-release exclusions before metadata changes. r[chaoscontrol.unified_agpl.authority]
- [ ] [serial] Record the last published Apache-2.0 revisions and the first revision governed by the unified policy. r[chaoscontrol.unified_agpl.prior_grants]

## Phase 2: License migration

- [ ] [serial] Change workspace and crate package metadata for authorized repository-owned source to `AGPL-3.0-or-later`. r[chaoscontrol.unified_agpl.metadata]
- [ ] [parallel] Replace crate-local Apache license artifacts with complete AGPL artifacts while retaining required third-party notices. r[chaoscontrol.unified_agpl.metadata]
- [ ] [parallel] Update repository-owned templates and scaffold output with visible AGPL notices. r[chaoscontrol.unified_agpl.templates]
- [ ] [parallel] Update README, detailed license mapping, package guidance, and prior-grant language. r[chaoscontrol.unified_agpl.prior_grants] r[chaoscontrol.unified_agpl.outputs]
- [ ] [serial] Update dependency-license policy without weakening third-party review. r[chaoscontrol.unified_agpl.dependency_policy]

## Phase 3: Package and distribution checks

- [ ] [parallel] Add positive checks for AGPL metadata and complete license artifacts in every authorized package. r[chaoscontrol.unified_agpl.validation]
- [ ] [parallel] Add negative fixtures for stale Apache metadata, missing license text, mismatched template notices, and accidental third-party relabeling. r[chaoscontrol.unified_agpl.validation]
- [ ] [serial] Inspect packaged crates and generated templates rather than relying only on workspace source paths. r[chaoscontrol.unified_agpl.metadata] r[chaoscontrol.unified_agpl.templates]
- [ ] [serial] Run focused license policy checks, package checks, workspace checks, and Cairn gates before sync or archive. r[chaoscontrol.unified_agpl.validation]
