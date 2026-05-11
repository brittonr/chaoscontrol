# replay-readiness-operator Specification

## Purpose

Defines the operator-facing replay-readiness receipt surfaces used by CI and local dashboard review while preserving bounded evidence claims.
## Requirements
### Requirement: Replay-readiness dashboard artifact [r[replay-readiness-dashboard-artifact]]

The system MUST render a replay-readiness receipt into a self-contained static HTML dashboard artifact for CI and local operator review.

#### Scenario: Dashboard renders a passing checks-only receipt [r[replay-readiness-dashboard-artifact.checks-only]]

- GIVEN a valid replay-readiness receipt with all static gates passing and no selected dogfood workload
- WHEN the dashboard renderer is invoked with an output path
- THEN it writes a static HTML file containing final status, static gate counts, dogfood skipped status, and bounded scope text

#### Scenario: Dashboard renders selected dogfood evidence [r[replay-readiness-dashboard-artifact.dogfood]]

- GIVEN a valid replay-readiness receipt with a selected dogfood workload and summary verdict
- WHEN the dashboard renderer is invoked
- THEN the HTML includes the workload, expectation status, acceptance status, replay class, replay-parent depth, seed, and fail-after value

#### Scenario: Dashboard fails closed on malformed receipts [r[replay-readiness-dashboard-artifact.fail-closed]]

- GIVEN a malformed or non-replay-readiness receipt
- WHEN the dashboard renderer is invoked
- THEN it exits nonzero and does not emit a misleading dashboard artifact

### Requirement: Dashboard CI packaging [r[replay-readiness-dashboard-ci]]

The replay-readiness CI/check surface MUST package the generated dashboard next to the receipt JSON and one-line summary so operators can inspect the same evidence without raw log scraping.

#### Scenario: Nix check emits dashboard artifact [r[replay-readiness-dashboard-ci.nix-check]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains non-empty `replay-readiness-receipt.json`, `replay-readiness-summary.txt`, and `replay-readiness-dashboard.html` artifacts

### Requirement: README replay-readiness status snippet [r[replay-readiness-readme-status]]

The system MUST provide a deterministic README status snippet derived from a validated replay-readiness receipt summary so the repository front page exposes the current bounded readiness claim without requiring CI artifact inspection.

#### Scenario: README status refreshes from a valid receipt [r[replay-readiness-readme-status.refresh]]

- GIVEN a README containing the replay-readiness status marker block and a valid replay-readiness receipt
- WHEN the README status updater is invoked
- THEN only the marker-bounded status block is replaced with a Markdown snippet containing the stable summary line and bounded scope language

#### Scenario: README status updater fails closed [r[replay-readiness-readme-status.fail-closed]]

- GIVEN a malformed receipt or a README missing the marker block
- WHEN the README status updater is invoked
- THEN it exits nonzero and does not emit a misleading status update

### Requirement: Fresh workload promotion gate [r[fresh-workload-promotion-gate]]
The replay-readiness static gate MUST fail closed when a workload is presented as supported without an accepted manifest proof, bounded anti-claims, and the generated experimental/unproven surface classifications that prevent overclaiming fresh workload authoring.

#### Scenario: Accepted manifest and report agree [r[fresh-workload-promotion-gate.manifest-report-agree]]
- GIVEN the accepted workload proof manifest and generated readiness status report
- WHEN the promotion gate runs
- THEN every supported workload row maps to exactly one accepted manifest proof with a unique assertion ID
- AND fresh workload authoring remains classified as experimental until a new accepted proof is committed

#### Scenario: Report-only promotion fails closed [r[fresh-workload-promotion-gate.report-only-fails]]
- GIVEN a readiness status report that lists a supported workload missing from the accepted manifest
- WHEN the promotion gate runs
- THEN it exits nonzero and reports the unsupported workload promotion

#### Scenario: Anti-claim removal fails closed [r[fresh-workload-promotion-gate.anti-claim-fails]]
- GIVEN an accepted manifest with missing or weakened anti-claim text
- WHEN the promotion gate runs
- THEN it exits nonzero before the readiness surface can be promoted

### Requirement: Generated readiness surface drift gate [r[generated-readiness-surface-drift-gate]]
The replay-readiness static gate MUST fail closed when the receipt, one-line summary, dashboard, README status snippet, or static gate metadata diverges from the same replay-readiness source of truth.

#### Scenario: Static gate metadata matches executed gates [r[generated-readiness-surface-drift-gate.static-gates]]
- GIVEN the replay-readiness shell executes a set of static `run_gate` checks
- WHEN the generated-surface drift gate runs
- THEN every executed static gate appears in the receipt `static_gates` metadata exactly once
- AND no receipt static gate is listed without a corresponding executed gate

#### Scenario: Summary surfaces share one summary line [r[generated-readiness-surface-drift-gate.summary-line]]
- GIVEN a valid replay-readiness receipt fixture
- WHEN the summary, dashboard, and README status renderers run
- THEN the dashboard and README snippet contain the exact summary line produced by the summary renderer
- AND both surfaces preserve bounded-scope anti-overclaim language

#### Scenario: Missing renderer marker fails closed [r[generated-readiness-surface-drift-gate.marker-fails]]
- GIVEN a README input without the replay-readiness status marker block
- WHEN the generated-surface drift gate exercises the README updater
- THEN it exits nonzero instead of silently accepting a stale or missing status surface

### Requirement: Static fleet triage index [r[static-fleet-triage-index]]

The system MUST render one or more replay-readiness receipts into a static multi-receipt fleet triage index without claiming hosted service, scheduler, shared decision-store, or full product parity support.

#### Scenario: Index renders multiple receipt summaries [r[static-fleet-triage-index.render]]

- GIVEN one or more valid replay-readiness receipt paths
- WHEN the fleet triage index renderer is invoked with an output path
- THEN it writes a static HTML artifact listing each receipt path, status, selected workload, replay class, replay-parent depth, and stable summary line

#### Scenario: Empty receipt set fails closed [r[static-fleet-triage-index.empty-fails]]

- GIVEN no replay-readiness receipt paths
- WHEN the fleet triage index renderer is invoked
- THEN it exits nonzero instead of emitting an empty or misleading fleet artifact

#### Scenario: Hosted fleet parity remains unpromoted [r[static-fleet-triage-index.anti-overclaim]]

- GIVEN a generated static fleet triage index
- WHEN an operator reviews the artifact
- THEN the artifact states that it is bounded static multi-receipt review, not universal replay evidence, hosted service, scheduler integration, shared decision store, or full Antithesis-style product replacement

### Requirement: Local replay-readiness decision receipt [r[local-decision-receipt]]

The system MUST provide a bounded local replay-readiness decision receipt format that records operator decisions for fleet-style triage without claiming hosted UI, scheduler integration, shared decision-store support, raw-log scraping, or full product parity.

#### Scenario: Decision receipt validates local decisions [r[local-decision-receipt.validate]]

- GIVEN a decision receipt with schema version, source fleet index, source replay-readiness receipt paths, non-empty decisions, linked artifacts, local scope, and anti-claims
- WHEN the decision receipt validator runs
- THEN it returns a stable summary containing the recorded status, decision count, actions, receipt count, and bounded-local-not-shared scope token

#### Scenario: Unsafe decision receipt fails closed [r[local-decision-receipt.fail-closed]]

- GIVEN a decision receipt that enables raw-log scraping, omits source receipts, duplicates decision IDs, uses unsupported actions, or weakens the bounded local anti-claim language
- WHEN the decision receipt validator runs
- THEN it exits nonzero and does not accept the receipt as fleet triage evidence

#### Scenario: Decision receipt is packaged with readiness artifacts [r[local-decision-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty decision receipt and decision receipt summary alongside the readiness receipt, summary, dashboard, runbook, and fleet index artifacts

### Requirement: Local replay scheduler receipt [r[local-scheduler-receipt]]

The system MUST provide a bounded local replay scheduler receipt format that records a manual multi-run replay-readiness plan without claiming hosted service, fleet-scale scheduler, shared queue, cross-machine orchestration, raw-log scraping, or full product parity.

#### Scenario: Scheduler receipt validates local run plans [r[local-scheduler-receipt.validate]]

- GIVEN a scheduler receipt with schema version, local scope, bounded schedule, non-empty run plan, unique run IDs, workload names, commands, receipt paths, decision policies, and anti-claims
- WHEN the scheduler receipt validator runs
- THEN it returns a stable summary containing recorded status, run count, workloads, schedule mode, and bounded-local-not-hosted scope token

#### Scenario: Unsafe scheduler receipt fails closed [r[local-scheduler-receipt.fail-closed]]

- GIVEN a scheduler receipt that enables raw-log scraping, duplicates run IDs, exceeds max runs, uses unsupported schedule modes, uses unsupported decision policies, or weakens the not-hosted/not-fleet/not-shared-queue anti-claim language
- WHEN the scheduler receipt validator runs
- THEN it exits nonzero and does not accept the receipt as scheduler orchestration evidence

#### Scenario: Scheduler receipt is packaged with readiness artifacts [r[local-scheduler-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty scheduler receipt and scheduler receipt summary alongside the readiness receipt, dashboard, runbook, fleet index, and decision receipt artifacts

### Requirement: Local replay scheduler execution receipt [r[local-scheduler-execution-receipt]]

The system MUST provide a bounded local sequential scheduler execution receipt that runs or records multiple scheduler-plan commands, links each successful run to a replay-readiness receipt summary, and does not claim hosted scheduling, fleet-scale workers, shared queues, cross-machine orchestration, raw-log scraping, or product parity.

#### Scenario: Scheduler execution records receipt-backed runs [r[local-scheduler-execution-receipt.records-runs]]

- GIVEN a valid local scheduler receipt with `concurrency=1` and multiple run-plan entries
- WHEN the local scheduler execution command runs the plan
- THEN it writes an execution receipt with one run result per entry, command, exit code, receipt path, stable receipt summary for passed runs, and bounded-local-sequential scope

#### Scenario: Scheduler execution fails closed on overclaims [r[local-scheduler-execution-receipt.fail-closed]]

- GIVEN a scheduler execution receipt that enables raw-log scraping, declares hosted or fleet/shared-queue semantics, duplicates run IDs, omits successful receipt summaries, or records concurrency greater than one
- WHEN the scheduler execution validator runs
- THEN it rejects the receipt and does not accept it as scheduler orchestration evidence

#### Scenario: Scheduler execution is packaged with readiness artifacts [r[local-scheduler-execution-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty scheduler execution plan, scheduler execution receipt, scheduler execution summary, and linked per-run replay-readiness receipts

### Requirement: Bounded fleet scheduler runtime [r[bounded-fleet-scheduler-runtime]]

The system MUST provide a bounded hosted/fleet scheduler runtime that consumes a durable queue plan, persists queue state for restart recovery, leases entries to workers, executes replay-readiness commands, emits run-linked receipt summaries, and records linked operator decision receipts without relying on raw-log scraping or claiming a live always-on multi-machine product service.

#### Scenario: Fleet scheduler runtime links durable queue and worker runs [r[bounded-fleet-scheduler-runtime.links-queue-workers-runs]]

- GIVEN a fleet scheduler plan with durable queue entries, worker IDs, bounded max concurrency, replay-readiness commands, receipt paths, and decision receipt links
- WHEN the fleet scheduler runtime command executes the plan
- THEN it writes a fleet scheduler receipt where each run links to an existing queue entry, existing worker, command, exit code, receipt path, replay-readiness summary for passed runs, and a restart-recovery section proving queue state was persisted after each run

#### Scenario: Fleet scheduler runtime rejects unsafe plans and receipts [r[bounded-fleet-scheduler-runtime.rejects-unsafe-plans-receipts]]

- GIVEN a fleet scheduler plan or receipt with raw-log scraping, duplicate identifiers, missing worker links, missing queue links, missing persisted queue-state evidence, missing receipt summaries for passed runs, no linked decision receipts, or max concurrency greater than worker count
- WHEN the fleet scheduler runtime or validator runs
- THEN it rejects the artifact as insufficient hosted/fleet scheduler evidence

#### Scenario: Fleet scheduler runtime is packaged with readiness output [r[bounded-fleet-scheduler-runtime.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty fleet scheduler plan, fleet scheduler receipt, fleet scheduler summary, fleet scheduler queue-state file, and linked per-run replay-readiness receipts alongside local scheduler execution artifacts

### Requirement: Local multi-hypervisor campaign runner [r[local-multi-hypervisor-campaign-runner]]

The system MUST provide a bounded local campaign runner that can launch multiple ChaosControl hypervisor workers from one durable campaign queue, lease work to those workers, persist queue state after each lease/run transition, and emit receipt-backed evidence without claiming hosted service, cross-machine scheduler, or full product parity.

#### Scenario: Campaign runner links hypervisors to queue leases and run receipts [r[local-multi-hypervisor-campaign-runner.links-hypervisors-leases-runs]]

- GIVEN a local campaign plan with a campaign ID, bounded hypervisor worker count, durable queue state path, workload/run entries, replay-readiness receipt paths, and linked local decision receipt policy
- WHEN the local multi-hypervisor campaign runner executes the plan
- THEN it writes a campaign/fleet receipt where every run links to an existing queue entry, a unique lease, a concrete `hypervisor_worker_id`, command/run metadata, exit status, receipt path, replay-readiness summary for passed runs, and queue-state persistence evidence

#### Scenario: Campaign runner preserves bounded local scope [r[local-multi-hypervisor-campaign-runner.scope]]

- GIVEN a generated campaign receipt or readiness status row for local multi-hypervisor execution
- WHEN an operator reviews the artifact
- THEN it states that the evidence covers bounded local multi-hypervisor campaign execution only, not hosted service, shared remote queue, cross-machine scheduling, universal fleet-scale throughput, or full Antithesis-style product replacement

#### Scenario: Campaign runner fails closed on ambiguous coordination [r[local-multi-hypervisor-campaign-runner.fail-closed]]

- GIVEN a campaign plan or receipt with duplicate leases, duplicate run IDs, missing hypervisor worker links, missing queue links, missing persisted queue-state evidence, successful runs without replay-readiness summaries, raw-log scraping, unbounded worker count, or anti-claims that imply hosted/shared multi-machine scheduling
- WHEN the campaign runner or validator runs
- THEN it rejects the artifact as insufficient multi-hypervisor campaign evidence

#### Scenario: Campaign runner packages replay-readiness evidence [r[local-multi-hypervisor-campaign-runner.packaged]]

- GIVEN the replay-readiness Nix check or equivalent local readiness package succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty local multi-hypervisor campaign plan, campaign/fleet receipt, campaign summary, persisted queue-state proof, and linked per-hypervisor replay-readiness receipts alongside the existing fleet scheduler artifacts

### Requirement: KVM-backed local multi-hypervisor smoke rail

The system MUST provide a bounded, explicitly KVM-backed local smoke rail for the local multi-hypervisor campaign runner that executes real replay-readiness dogfood commands through at least two local hypervisor worker identities and emits receipt-backed evidence without raw-log scraping or hosted-service claims.

#### Scenario: Smoke rail executes dogfood receipts through local hypervisor workers

- GIVEN a machine with `/dev/kvm` available and a selected bounded workload set
- WHEN the KVM local multi-hypervisor smoke rail runs
- THEN it writes a campaign plan, campaign receipt, queue-state file, per-run replay-readiness receipts, and summary file where every passed run includes a replay-readiness summary from a real dogfood command and every run is linked to a local hypervisor worker identity and durable queue lease

#### Scenario: Smoke rail remains bounded and local

- GIVEN a generated KVM multi-hypervisor smoke receipt or summary
- WHEN an operator reviews the artifact
- THEN it states the evidence covers bounded local KVM multi-hypervisor campaign execution only, not a hosted service, shared remote queue, cross-machine scheduler, fleet-scale throughput, or full Antithesis-style replacement

### Requirement: Hosted scheduler shared-state receipt [r[hosted-scheduler-shared-state-receipt]]

The system MUST provide a bounded hosted/fleet scheduler shared-state receipt model that coordinates replay-readiness queue entries across multiple machine identities and can be backed by a networked or multi-process harness without relying on raw-log scraping or claiming SaaS/product parity.

#### Scenario: Shared queue leases bind independently started worker evidence [r[hosted-scheduler-shared-state-receipt.networked-workers]]

- GIVEN a hosted scheduler plan with at least two independently started machine/worker identities attached to a shared queue and decision-store adapter
- WHEN the networked hosted scheduler harness executes the plan
- THEN the receipt records each queue entry, lease ID, lease epoch, owning machine ID, hypervisor worker ID, worker session ID, command, exit status, receipt path, stable replay-readiness summary, and state snapshot digest for every completed run

#### Scenario: Networked scheduler rejects ambiguous coordination [r[hosted-scheduler-shared-state-receipt.networked-fail-closed]]

- GIVEN a networked scheduler receipt with duplicate active leases, missing worker-session heartbeats, stale queue-state revisions, missing decision-store revisions, missing receipt summaries for passed runs, raw-log scraping, or anti-claims that imply full hosted product parity
- WHEN the shared scheduler validator runs
- THEN it rejects the artifact as insufficient hosted/fleet scheduler evidence

### Requirement: Shared replay-readiness decision store [r[shared-replay-readiness-decision-store]]

The system MUST provide a bounded shared decision-store receipt model that records operator decisions against replay-readiness receipts with writer identity, revision/conflict evidence, source receipt links, and explicit non-claims for a production hosted UI unless separately evidenced.

#### Scenario: Networked decision writes preserve revision ordering [r[shared-replay-readiness-decision-store.networked-revisions]]

- GIVEN multiple independently started workers writing decision records through the shared decision-store adapter
- WHEN the harness records decisions for run receipts
- THEN each decision record includes writer identity, machine identity, worker session ID, monotonic revision, previous revision or conflict marker, target run/queue entry, source receipt paths, and a stable summary suitable for fleet triage review

### Requirement: Hosted scheduler readiness promotion gate [r[hosted-scheduler-readiness-promotion-gate]]

The replay-readiness promotion gate MUST fail closed unless hosted/fleet triage and replay scheduler readiness rows are backed by shared queue and decision-store receipts that prove bounded cross-machine or multi-process coordination semantics.

#### Scenario: Promotion requires networked shared-state artifacts [r[hosted-scheduler-readiness-promotion-gate.requires-networked-artifacts]]

- GIVEN a generated readiness report that promotes hosted/fleet triage or scheduler support beyond bounded loopback shared-state evidence
- WHEN the promotion gate runs without a valid networked scheduler receipt, worker-session records, queue-state snapshots, decision-store snapshots, and linked run receipts
- THEN it exits nonzero and reports the missing networked hosted/shared-state evidence

### Requirement: Local-first replay-readiness product scope [r[replay-readiness-operator.local-rust-scope]]

The replay-readiness status surfaces MUST describe ChaosControl's current product target as Rust-only workload support on one machine with multiple local hypervisors, and MUST NOT present SaaS, cross-machine fleet scheduling, or multi-language SDK coverage as active missing features for current readiness.

#### Scenario: Status names current local scope [r[replay-readiness-operator.local-rust-scope.status]]

- GIVEN the generated replay-readiness status report is rendered
- WHEN an operator reviews experimental or unproven surfaces
- THEN the report identifies current missing product work in terms of local multi-hypervisor execution, local triage, Rust workload authoring, bounded determinism, and local artifact hygiene
- AND it labels hosted service, cross-machine fleet scheduling, and non-Rust SDKs as out-of-scope for current product readiness

#### Scenario: Hosted and fleet overclaims still fail [r[replay-readiness-operator.local-rust-scope.overclaim]]

- GIVEN a readiness report claims SaaS, real cross-machine fleet scheduling, or full Antithesis replacement from local evidence
- WHEN the promotion gate runs
- THEN it rejects the report even though those surfaces are current non-goals

### Requirement: Local multi-hypervisor control plane [r[local-multi-hypervisor-control-plane]]

The system MUST provide a bounded single-machine control plane for multiple local ChaosControl hypervisor workers that manages durable queue state, worker resource placement, per-run artifact roots, bug follow-up jobs, and receipt-backed execution without claiming hosted service, cross-machine fleet scheduling, or full product parity.

#### Scenario: Control plane leases work to local hypervisors [r[local-multi-hypervisor-control-plane.leases-local-workers]]

- GIVEN a local control-plane plan with bounded worker count, queue state path, worker IDs, optional CPU/memory budgets, per-worker artifact roots, and replay-readiness commands
- WHEN the control plane runs the plan
- THEN it starts or records local hypervisor workers, leases each queue entry to exactly one worker, writes state after each transition, and emits a receipt linking worker, lease, command, exit status, artifact root, and replay-readiness summary

#### Scenario: Control plane schedules reproduce and minimize follow-ups [r[local-multi-hypervisor-control-plane.bug-handoff]]

- GIVEN a local hypervisor run emits a bug artifact with snapshot-backed replay context or schedule-only gap evidence
- WHEN bug follow-up policy is enabled
- THEN the control plane enqueues local reproduce and/or minimize jobs that link back to the original worker/run, bug artifact, snapshot ref, and resulting verdict or minimization receipt

#### Scenario: Control plane rejects unsafe local coordination [r[local-multi-hypervisor-control-plane.fail-closed]]

- GIVEN a control-plane plan or receipt with duplicate active leases, missing state persistence, missing worker/run links, unbounded worker count, successful runs without receipt summaries, raw-log scraping, remote shared queue semantics, cross-machine scheduling claims, or hosted-service language
- WHEN the validator runs
- THEN it rejects the artifact as insufficient local multi-hypervisor evidence

### Requirement: Local multi-hypervisor artifact hygiene [r[local-multi-hypervisor-artifact-hygiene]]

The local multi-hypervisor control plane MUST keep campaign artifacts content-addressed or hash-bound, bounded by retention policy, and attributable to a worker/run without relying on raw logs.

#### Scenario: Artifact index binds run outputs [r[local-multi-hypervisor-artifact-hygiene.index]]

- GIVEN a completed local multi-hypervisor campaign
- WHEN the artifact index is generated
- THEN it records each worker/run artifact root, replay-readiness receipt path, bug artifact path, snapshot/chunk manifest digest when present, reproduce/minimize receipt paths, and retention/GC status
