## Phase 1: Identity and registry core

- [x] [serial] Define versioned assertion namespace/logical-key, canonical descriptor, BLAKE3 fingerprint, legacy-key variant, and deterministic canonical encoding. r[chaoscontrol.assertion_identity.model]
- [x] [serial] Implement pure catalog insertion and completion validation with exact-duplicate idempotence and typed logical-key, fingerprint, metadata, malformed-field, and legacy-alias conflicts. r[chaoscontrol.assertion_identity.catalog_validation] r[chaoscontrol.assertion_identity.boundary]
- [x] [serial] Add positive canonicalization/idempotence assertions and negative forced-fingerprint-collision, changed-kind/message/source/guest/category, invalid-field, and ordering assertions. r[chaoscontrol.assertion_identity.validation.conflicts]

## Phase 2: SDK and protocol binding

- [x] [serial] Add versioned catalog begin/descriptor/complete transport records and preserve complete source, guest, category, and logical-key metadata through the host decoder. r[chaoscontrol.assertion_identity.catalog_protocol]
- [x] [parallel] Add explicit stable namespace/key assertion APIs; remove public `u32` assertion APIs, compatibility commands, and unbound guidance; migrate in-repo callers. r[chaoscontrol.assertion_identity.compatibility]
- [x] [serial] Bind runtime events to accepted catalog fingerprints or tokens and reject pre-catalog, unknown, mismatched, or post-conflict events in strict mode. r[chaoscontrol.assertion_identity.event_binding]
- [x] [serial] Remove strict-mode oracle auto-creation and `u32`-only authoritative maps while preserving an explicitly quarantined diagnostic path. r[chaoscontrol.assertion_identity.event_binding]

## Phase 3: Oracle, reports, and snapshots

- [x] [serial] Store complete descriptors and structured identities in oracle records, events, snapshots, and report schemas. r[chaoscontrol.assertion_identity.report_identity]
- [x] [serial] Merge per-VM records only after exact descriptor validation, aggregate matching guest-binary properties across VM instances, and keep distinct namespaces separate. r[chaoscontrol.assertion_identity.merge]
- [x] [serial] Reuse the pure identity/merge core in local JSONL reporting and reject first-wins metadata conflicts. r[chaoscontrol.assertion_identity.local_reports]
- [x] [serial] Update compact Nickel review-boundary contracts and acceptance logic for identity version, descriptor, fingerprint, catalog status, and legacy classification while keeping runtime records Rust-owned. r[chaoscontrol.assertion_identity.compatibility]
- [x] [serial] Bind exported bugs, schema-v2 replay verdicts, checkpoints, campaign collections, and minimization inputs to exact admitted assertion identity; reject the whole carrier on the first invalid bug. r[chaoscontrol.assertion_identity.replay_artifacts]

## Phase 4: Regression evidence

- [x] [parallel] Add exact-duplicate registration and same-descriptor multi-VM aggregation tests. r[chaoscontrol.assertion_identity.validation.merge]
- [x] [parallel] Add conflicting explicit-ID, automatic-hash alias, source-metadata, cross-namespace, and forced-digest-collision tests proving no records merge. r[chaoscontrol.assertion_identity.validation.conflicts]
- [x] [parallel] Add pre-catalog, unknown-fingerprint, mismatched-token, event-kind-spoofing, and catalog-conflict runtime tests. r[chaoscontrol.assertion_identity.validation.events]
- [x] [parallel] Add local JSONL conflict and namespace-separation tests. r[chaoscontrol.assertion_identity.validation.local]
- [x] [serial] Add strict rejection and diagnostic quarantine tests for legacy `u32`-only protocol, snapshot, and report inputs. r[chaoscontrol.assertion_identity.validation.legacy]
- [x] [parallel] Add forged descriptor, canonical-byte, catalog-token, alias-substitution, alias-collision, and mixed valid/invalid bug collection tests. r[chaoscontrol.assertion_identity.replay_artifacts]
- [x] [serial] Document automatic build scope, explicit-key continuity, unsupported old wire/API forms, and the digest-uniqueness non-claim. r[chaoscontrol.assertion_identity.compatibility]
- [x] [serial] Run focused protocol/SDK/oracle/report/contract tests, workspace tests, Cairn validation, and proposal/design/tasks gates before sync or archive. r[chaoscontrol.assertion_identity.validation]
