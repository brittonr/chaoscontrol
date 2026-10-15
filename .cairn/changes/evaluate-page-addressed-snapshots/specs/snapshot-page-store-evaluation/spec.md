# Snapshot Page Store Evaluation Specification

## Purpose

Give explorer branching and retained replay-parent snapshot consumers a finite correctness-first comparison of existing Arc overlays and whole CBOR+zstd artifacts against an isolated persistent 4 KiB BLAKE3 page-addressed keyframe DAG, ending in a reproducible decision rather than an automatic production migration.

## ADDED Requirements

### Requirement: Evaluation scope and ownership remain explicit

r[chaoscontrol.snapshot_page_evaluation.scope] The evaluation MUST compare the current in-memory Arc-overlay path and current whole-envelope CBOR+zstd retained-artifact path with an isolated experimental persistent 4096-byte BLAKE3 page store and keyframe DAG. It MUST preserve existing production codecs, references, consumers and retained proof artifacts. A named explorer/replay-evidence maintainer MUST own the protocol, harness and decision, and VMM snapshot maintainers MUST review fidelity admission. Immediate deliverables MUST be the finite protocol, experiment evidence and adopt/defer/reject receipt; the durable capability MUST be a reusable bounded comparison method. The immutable RewindVM reference at `https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082`, especially `crates/rewind-store/src/lib.rs` and `crates/rewind-core/src/keyframes.rs`, MUST be identified as inspiration, not a dependency or parity proof. Engine MIT, guest-kernel-patch GPL-2.0 and proprietary desktop-app boundaries MUST be recorded; app code MUST NOT be imported. Scope MUST remain Rust-only and local-machine, without containers, hosted services or weaker determinism fallback.

#### Scenario: A reviewer opens the experiment plan

- GIVEN the current explorer branching and retained replay-parent consumers
- WHEN the evaluation protocol is admitted
- THEN it MUST name responsible maintainers, all three cost surfaces and isolated output roots, and MUST leave production references and behavior unchanged.

#### Scenario: A favorable experiment is offered as a production migration

- GIVEN the experimental representation passes its bounded checks
- WHEN a consumer attempts to replace production codecs or references under this evaluation
- THEN the cutover MUST remain unauthorized until a separate reviewed migration change is approved.

### Requirement: Protocols declare finite workloads and budgets before execution

r[chaoscontrol.snapshot_page_evaluation.protocol] A Nickel-backed protocol MUST bind finite positive `max_cases`, `max_vms`, `max_guest_bytes_per_vm`, `max_snapshots`, `max_branches`, `max_lineage_depth`, `max_dag_nodes`, `max_page_objects`, `max_manifest_bytes`, `max_non_memory_bytes`, `max_compressed_page_bytes`, `max_store_bytes`, `max_staging_bytes`, `max_gc_scan_objects`, `max_continuation_ticks`, `max_continuation_instructions`, `max_seek_targets`, `repetitions` and `host_operation_timeout_ms`. It MUST bind explicit cases, seed(s), page sets, cache/compression settings, logical `keyframe_interval_ticks`, seek targets and canonical ordering of cases, VMs, pages, graph traversal and failure reporting. Bounds MUST be validated with checked arithmetic before resource use; non-page-aligned experimental geometry MUST be rejected rather than truncated. The finite case table MUST cover sparse/dense unique changes, identical/zero pages, siblings, deep lineages through the configured bound, inherited and host-written pages, multi-VM distinct identity with shared content, and non-memory-only transitions. It MUST include the admitted Raft and redb retained cohorts when valid, or record explicit admission blockers. Wall-clock measurements and operational deadlines MUST NOT determine guest scheduling, virtual time, checkpoint placement or deterministic case content.

#### Scenario: The finite matrix is admitted

- GIVEN explicit sparse, dense, identical-page, sibling, deep, multi-VM and non-memory cases within a validated profile
- WHEN protocol admission succeeds
- THEN each case MUST have a stable ID, finite bounds and deterministic ordering, with Raft/redb admission status recorded separately.

#### Scenario: Resource or geometry admission fails

- GIVEN an absent/unbounded budget, arithmetic overflow, one-over-limit case or nonaligned memory size
- WHEN admission runs
- THEN the case MUST fail before allocation, artifact publication or guest mutation, without silently dropping pages or enlarging bounds.

#### Scenario: A host timeout interrupts a sample

- GIVEN a timed operation exceeds its host deadline
- WHEN the shell cancels it
- THEN the row MUST be incomplete/diagnostic and MUST NOT substitute timer-driven guest progress or a different keyframe interval.

### Requirement: Full state and continuation fidelity gate every comparison

r[chaoscontrol.snapshot_page_evaluation.fidelity] Every measured state MUST have an independent full-snapshot oracle. Before interpreting paired metrics, the evaluation MUST prove exact per-VM full-memory materialization and canonical non-memory equivalence for both baseline overlays/artifacts and experimental keyframes against that oracle. It MUST preserve completeness schema/profile, VM topology/inventory, CPU/KVM state, devices and block backends, entropy, clocks, fault engines, scheduler/exact-progress/HLT state, serial, coverage, network fabric, VM status and controller pending-effect/counter state. Fresh-controller and long-lived-controller restore paths MUST undergo bounded identical continuation with matching canonical schedule/round traces, final full-state identities, assertion/outcome and error observations at declared boundaries. Sibling switching, inherited/host-written pages and deep chains MUST be covered. Incomplete baselines, known unresolved lineage defects, invalid/missing artifacts or divergent continuation MUST block that cohort's comparison and any adopt recommendation; diagnostic timings MUST NOT enter valid metric aggregates. Overlay repair MUST remain owned by `repair-incremental-snapshot-lineage`, without silent repair or normalization in this experiment.

#### Scenario: A sibling and deep lineage are faithful

- GIVEN independently captured full snapshots and admitted complete incremental parents
- WHEN each representation is materialized, restored and continued under identical bounded exact inputs in fresh and long-lived controllers
- THEN full memory, complete non-memory state and continuation observations MUST agree before the associated costs become comparable.

#### Scenario: Two incremental paths agree but disagree with the full oracle

- GIVEN a missing inherited or host-written page in a baseline parent
- WHEN the experimental representation reproduces the same incomplete state
- THEN full-oracle validation MUST reject the cohort and MUST block favorable comparison or adoption despite agreement between the two broken paths.

#### Scenario: A documented retained cohort is unavailable

- GIVEN a Raft or redb documentation row but missing exact snapshot, schema/profile or replay inputs
- WHEN cohort admission runs
- THEN the receipt MUST name the exact blocker and MUST NOT replace the cohort with synthetic success or promote historical diagnostic evidence.

### Requirement: Page and graph references fail closed within bounds

r[chaoscontrol.snapshot_page_evaluation.references] Experimental references MUST bind schema/domain, BLAKE3 algorithm/digest, exact decoded page length of 4096, encoded length/codec and a root-confined local locator. Keyframe manifests MUST bind exact byte length/digest, completeness schema/profile, VM geometry/topology, full-state identity, canonical page ordering and parent linkage. The validator MUST bound encoded/decoded bytes, metadata and page/graph counts before decoding/allocation, check page-offset arithmetic, reject duplicate/ambiguous entries and incompatible ancestors, and validate the complete reachable closure before restore mutation. Missing/corrupt/truncated/oversized objects, invalid hashes/lengths, unsupported codecs/algorithms/schemas, decompression excess, out-of-range pages, unsafe/symlink/nonregular locators, cycles and depth/node exhaustion MUST yield typed blockers. Zero-page elision MUST use a versioned explicit variant with exact zero materialization or a real BLAKE3 object, not a sentinel misrepresented as a content hash. Failure MUST NOT zero-fill a missing object, skip an ancestor or yield a successful snapshot; post-mutation failures MUST preserve existing controller poison semantics.

#### Scenario: A bounded graph resolves exactly

- GIVEN a canonical manifest and all referenced pages/parents within the profile bounds
- WHEN the closure is validated and materialized
- THEN exact content identities and full-state linkage MUST validate before guest mutation, including distinct VM/page positions for shared content.

#### Scenario: A reference is unsafe or corrupt

- GIVEN a wrong hash/length/schema, unsafe path, symlink escape, nonregular object, duplicate page or out-of-range offset
- WHEN reference preflight runs
- THEN it MUST return the corresponding typed blocker before restore and MUST NOT consume the object as evidence.

#### Scenario: A parent graph is cyclic or too deep

- GIVEN a parent cycle, absent ancestor or chain exceeding `max_lineage_depth` or `max_dag_nodes`
- WHEN closure traversal runs
- THEN it MUST terminate with a deterministic blocker within the admitted bound and MUST NOT publish a complete comparison row.

### Requirement: Publication and reachability protect pinned artifacts

r[chaoscontrol.snapshot_page_evaluation.retention] The experiment MUST pin source cohorts, replay parents, frontier/branch snapshots, active readers/materializations, manifests and decision/raw-observation artifacts before use. Published proof artifacts MUST remain read-only and MUST NEVER be deletion candidates. Writes MUST use bounded isolated staging and publish the manifest only after verified complete page, ancestor and non-memory closure, with an atomic visibility boundary. GC MUST operate only on proven-unreachable experimental objects under a stable root generation with exclusive publication/collection coordination; traversal and deletion plans MUST be deterministic and bounded. Unknown ownership, missing pins, changed root generations, failed reachability, cycles or interrupted replacement publication MUST abort deletion conservatively. Fault cases MUST cover staging/publish failure, resource exhaustion, interrupted compaction and reader/publisher pin races, retaining either the old complete closure or the new complete closure without dangling live refs. Local dedup/GC success MUST NOT imply external adapter selection, durability, availability, trust, tracked-payload migration or proof promotion; those materialization/retention concerns remain with `externalize-large-dogfood-artifacts`.

#### Scenario: Shared descendants retain a page

- GIVEN pinned sibling/keyframe roots and a page reachable from any live root or active reader
- WHEN the isolated scratch GC plan is derived and executed
- THEN that page and its required ancestors MUST remain retained even if another branch is released, and published proof artifacts MUST remain untouched.

#### Scenario: Publication stops before completeness

- GIVEN a staged page write, full store, failed hash check or interrupted manifest publication
- WHEN the operation fails
- THEN no partial keyframe MUST become consumable and prior pinned snapshots MUST still materialize exactly.

#### Scenario: Roots change during collection

- GIVEN a new reader/publisher pin or incomplete root inventory after planning
- WHEN collection admission rechecks the generation
- THEN deletion MUST abort without guessing reachability or calling a flat keep-list collector on the shared DAG.

### Requirement: Measurements retain identities, costs and raw observations

r[chaoscontrol.snapshot_page_evaluation.measurements] Every observation MUST bind exact source/experiment revisions, commands, toolchain/build and kernel/initrd/workload identities, host/KVM/exact-progress profile, topology, seed, Nickel protocol/config digest, cohort/root/reference/object IDs, schema/codec/compression settings, chain depth, keyframe interval, fidelity result and status. Raw rows MUST retain per-repetition capture, publication, materialization, restore, seek and bounded continuation wall durations; hashing/compression/serialization CPU costs; I/O bytes; peak memory/staging; logical represented bytes; unique decoded page bytes/counts; compressed payload; manifests/index/non-memory bytes; and total retained bytes. Arc shared-base/overlay accounting MUST be separate from persisted accounting; dedup numerator/denominator, zero-page policy, sharing scope and cache protocol/confounders MUST be explicit. Summaries MUST be reproducible from exact-hash/length raw artifacts and include failures/exclusions without double-counting shared objects or presenting unverified cold-cache conditions as controlled. Speedup/storage-savings claims MUST NOT precede valid measurement or generalize beyond the admitted bounded cohorts.

#### Scenario: Valid samples produce a comparison

- GIVEN equivalent states/continuations and complete raw rows for declared repetitions and cache conditions
- WHEN summary accounting runs
- THEN retained bytes, dedup and duration distributions MUST be reproducible with explicit denominators, full metadata costs and bounded scope.

#### Scenario: A report hides invalid rows or metadata costs

- GIVEN raw failures, excluded deep cohorts or nontrivial index/non-memory/staging bytes
- WHEN a summary omits them or claims savings without valid paired measurements
- THEN report validation MUST reject the claim rather than treating missing evidence as zero cost or successful execution.

### Requirement: Pure decisions are separate from shell effects

r[chaoscontrol.snapshot_page_evaluation.functional_core] Profile/reference checks, canonical page/graph planning, supplied-state fidelity comparisons, reachability/GC planning, accounting and decision derivation MUST be pure deterministic Rust logic over supplied facts. KVM operations, filesystem reads/staging/publication/locks/deletion, hashing observations and host clock/resource sampling MUST remain in the Rust effect shell. Nickel MUST own human-authored protocol and reviewable receipt contracts; high-volume runtime snapshots/measurements MUST remain Rust-derived. No filesystem iteration order or host timestamp MAY change a pure decision or grant guest scheduling authority.

#### Scenario: Input facts are reordered by the filesystem

- GIVEN identical admitted protocol, graph, root and observation facts delivered in a different shell iteration order
- WHEN the pure core canonicalizes and evaluates them
- THEN plans, typed blockers, accounting and decision MUST be identical.

#### Scenario: A shell operation fails after planning

- GIVEN a valid pure plan but failed page I/O, lock acquisition or KVM restore
- WHEN the shell records the failure
- THEN it MUST bind the typed effect observation to an incomplete/failed row and MUST NOT fabricate the planned successful result.

### Requirement: Decision receipts apply predeclared adoption criteria

r[chaoscontrol.snapshot_page_evaluation.decision] Before execution, the protocol MUST freeze mandatory cohorts, correctness/safety obligations, resource ceilings and owner-rationalized measured trade-off criteria with units/direction; missing criteria MUST block execution/adopt and criteria changes MUST create a new profile/receipt. The final Nickel-validated receipt MUST bind the protocol and raw evidence identities, commands, admission/exclusion table, fidelity/safety verdicts, complete bounded costs, criteria-by-criteria results, named maintenance owner/reviewers, limitations and exactly one `adopt`, `defer` or `reject` recommendation. `adopt` MUST require all mandatory cohorts admitted, exact fidelity/continuation and negative safety paths passing, complete reproducible measurements within declared bounds and predeclared trade-offs satisfied; it MUST authorize no production cutover. `defer` MUST identify exact missing/invalid inputs or unresolved trade-offs; `reject` MUST identify evaluated mandatory criterion failures. Either no-go outcome MUST be a complete valid deliverable without favorable performance evidence. Production codec/reference migration MUST require its own reviewed change, with every consumer, compatibility and retention gate addressed. This active spec-only package MUST check only observed source-review/foundation tasks; experiment implementation, fixtures, measurements, KVM smoke, final decision production and release closeout MUST remain unchecked until their own evidence exists. Structural lifecycle gates MUST NOT be represented as implementation, replay, release, sync or archive approval.

#### Scenario: Valid evidence recommends adoption

- GIVEN every mandatory cohort and predeclared correctness, safety, budget and trade-off criterion passes
- WHEN the final receipt is reviewed
- THEN it MAY recommend `adopt` for a separate reviewed migration and MUST state that existing production behavior remains unchanged.

#### Scenario: An invalid deep baseline prevents a favorable decision

- GIVEN an unresolved lineage gap, unavailable KVM/cohort or incomplete measurements
- WHEN the finite evaluation closes
- THEN a reproducible `defer` receipt with exact blockers MUST be accepted as complete and MUST NOT claim speedup, parity or snapshot proof promotion.

#### Scenario: Measured trade-offs fail the frozen criteria

- GIVEN valid equivalent snapshots and complete measurements that fail a mandatory predeclared criterion
- WHEN decision derivation runs
- THEN the receipt MUST recommend `reject` or a justified unresolved-trade-off `defer`, never alter thresholds after seeing results to manufacture `adopt`.
