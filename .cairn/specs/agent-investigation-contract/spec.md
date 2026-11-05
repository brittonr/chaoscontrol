# Agent Investigation Contract Specification

## Purpose

Defines the `agent-investigation-contract` capability.

## Requirements

### Requirement: Mid-run bug events expose a stable failure reference

r[chaoscontrol.agent_investigation.bug_event_reference]

Every `BugFound` dashboard event MUST carry the emitted failure's dedup key and the
replay parent depth of the branch that produced it. The dedup key MUST equal the
key stored in the corresponding checkpoint bug report, and the depth MUST equal
that report's `replay_parent_depth`. Both fields MUST be additive: previously
serialized events without them MUST deserialize with defined defaults, and
serialization MUST always include them. Emitting an event MUST NOT serialize,
hash, or persist a snapshot inside the run loop.

#### Scenario: Event matches the checkpoint record

- GIVEN a failing assertion admitted by exploration
- WHEN the bug is extracted and a `BugFound` event is emitted
- THEN the event carries the same dedup key and replay parent depth as the bug report

#### Scenario: Legacy event input

- GIVEN a serialized `BugFound` event without the new fields
- WHEN it is deserialized
- THEN the fields take their defined defaults and no other event field changes

### Requirement: Checkpoint export selects by failure identity

r[chaoscontrol.agent_investigation.export_by_dedup_key]

`export-bugs` MUST accept a dedup-key filter and write only checkpoint bugs whose
stored key equals it. The filter MUST compose with the assertion-id and
minimum-parent-depth filters, `max_bugs` MUST still cap writes after matching, and
a checkpoint bug without a stored key MUST NOT match a requested key. Unmatched
filters MUST write no artifacts and MUST NOT error.

#### Scenario: Keyed selection

- GIVEN a checkpoint holding bugs with distinct dedup keys
- WHEN the operator exports with one of those keys
- THEN only that bug's artifact is written and the summary counts the scan and match

#### Scenario: Legacy checkpoint

- GIVEN a checkpoint whose bugs predate stored dedup keys
- WHEN the operator requests a specific key
- THEN nothing matches, no artifact is written, and the command exits successfully

### Requirement: Checkpoint failures can be located without artifacts

r[chaoscontrol.agent_investigation.locate_bugs]

A noninteractive listing operation MUST surface admitted checkpoint-held failures
as stable records without writing artifacts or loading snapshot bytes. Each record
MUST carry its checkpoint index and artifact name, assertion id, optional dedup
key, tick, replay parent depth, optional snapshot digest, identity presence, and
process instances. Filters by assertion id, dedup key, and inclusive tick range
MUST compose, an inverted range MUST be rejected, and a bug without a stored dedup
key MUST NOT match a requested key.

#### Scenario: Locate by failure identity
- GIVEN a checkpoint holding admitted failures
- WHEN the operator lists with a dedup key or tick range
- THEN only matching records are returned in checkpoint order and no bug file is written

#### Scenario: Invalid range
- GIVEN a minimum tick greater than the maximum
- WHEN listing runs
- THEN it fails with an explicit range error

### Requirement: A single failure can be inspected as versioned read-only JSON

r[chaoscontrol.agent_investigation.inspect_bug]

An inspect operation MUST accept exactly one selector (checkpoint index or stored
dedup key), validate the checkpoint bug set, and return a versioned record
containing the checkpoint index, artifact name, the exact stored bug report, and
explicit non-claims. Zero matches, ambiguous matches, and empty or dual selectors
MUST fail with distinct typed outcomes. Inspection MUST NOT write artifacts, load
snapshot bytes, execute a VM, or claim replay evidence.

#### Scenario: Inspect by key
- GIVEN a checkpoint with one bug for a dedup key
- WHEN the operator inspects that key
- THEN the exact stored report is returned with its artifact name and non-claims

#### Scenario: Selector errors
- GIVEN no selector, both selectors, or a selector matching zero/two bugs
- WHEN inspection runs
- THEN it fails with the corresponding typed outcome and writes nothing

### Requirement: Retained checkpoint admission is versioned and fails closed

r[chaoscontrol.agent_investigation.verify_checkpoint]

An admission operation MUST validate checkpoint structure and the referenced
snapshot artifacts, returning a versioned record with a BLAKE3 checkpoint digest,
bug counts, validated counts, per-bug issues, a verdict, and explicit non-claims.
Each unique snapshot reference MUST be validated once; missing artifacts, digest
mismatches, or depth mismatches MUST make the verdict invalid rather than being
repaired or ignored. Admission MUST NOT execute a VM or claim replay evidence.

#### Scenario: Admission with a missing artifact
- GIVEN a checkpoint whose bug references an absent snapshot file
- WHEN admission runs
- THEN the verdict is invalid and the issue names the bug and reason

#### Scenario: Shared snapshot reference
- GIVEN several bugs referencing the same snapshot artifact
- WHEN admission runs
- THEN the artifact bytes are validated once and each matching bug counts as validated

### Requirement: Admission can bind the expected session digest

r[chaoscontrol.agent_investigation.session_digest_binding]

Admission MUST accept an expected checkpoint digest and reject a mismatch before
validating any snapshot artifact. The digest MUST be the BLAKE3 digest of the exact
checkpoint bytes and MUST be reported in the admission record. A mismatch MUST be a
distinct typed failure, not a warning, and MUST NOT fall back to the actual digest.

#### Scenario: Bound digest matches
- GIVEN a checkpoint and its reported BLAKE3 digest
- WHEN admission runs with that digest
- THEN it proceeds with artifact validation

#### Scenario: Bound digest differs
- GIVEN an expected digest that differs from the checkpoint bytes
- WHEN admission runs
- THEN it fails with a digest-mismatch error before reading snapshots

### Requirement: A failure moment has a stable read-only reference

r[chaoscontrol.agent_investigation.moment_reference]

A moment-reference operation MUST bind the checkpoint BLAKE3 digest, checkpoint
index and artifact name, bug/assertion identity, dedup key, recorded failure tick,
replay parent depth, and the snapshot digest when a reference is present. A supplied
tick MUST equal the recorded failure tick or fail with a distinct error. A present
snapshot reference MUST be validated (digest and depth) before the digest is
reported. The record MUST state that it is not replay evidence and that the tick is
the recorded failure tick, not an arbitrary seek position.

#### Scenario: Stable reference
- GIVEN a valid checkpoint bug
- WHEN a moment reference is requested
- THEN the session digest, bug identity, tick, and optional snapshot digest are returned together

#### Scenario: Tick mismatch
- GIVEN a requested tick different from the stored failure tick
- WHEN the reference is requested
- THEN it fails with a typed tick-mismatch error

### Requirement: Recorded sessions expose bounded JSON seek/inspect

r[chaoscontrol.agent_investigation.debug_state_json]

A debug command MUST support a versioned JSON output that binds the recording
BLAKE3 digest, session id, seed, configuration summary, total ticks, target tick,
inclusive event window, nearest checkpoint (with distance) or null, bounded event
projections, checkpoint serial output, and explicit non-claims. Event projections
MUST be capped, and the record MUST NOT claim a live VM, replay verdict, or that the
nearest checkpoint is replayable.

#### Scenario: Seek within a recording
- GIVEN a recorded session
- WHEN debug runs with JSON output at a tick
- THEN the digest, window bounds, nearest checkpoint, and bounded events are returned together

#### Scenario: Missing checkpoint
- GIVEN a tick before any checkpoint
- WHEN debug runs with JSON output
- THEN the nearest checkpoint is null and the rest of the bounded record is still returned

### Requirement: Admission distinguishes structural from snapshot validation

r[chaoscontrol.agent_investigation.admission_modes]

Admission MUST expose a structure-only mode that validates the checkpoint record and
bug set without reading snapshot artifacts, and MUST report the mode, whether
snapshots were checked, the referenced/validated counts, and mode-specific
non-claims. Full mode MUST retain snapshot digest/depth validation and fail closed.
Structure-only success MUST NOT be presented as snapshot admission or replay
evidence.

#### Scenario: Structure-only with absent artifacts
- GIVEN a checkpoint whose snapshot artifact is absent
- WHEN structure-only admission runs
- THEN it succeeds with `snapshots_checked: false` and no snapshot issues

#### Scenario: Full admission still fails
- GIVEN the same checkpoint
- WHEN full admission runs
- THEN it reports the missing artifact and an invalid verdict

### Requirement: A referenced parent snapshot can be decoded and summarized

r[chaoscontrol.agent_investigation.inspect_snapshot]

An inspection operation MUST decode the replay parent snapshot referenced by one
selected checkpoint failure, validate its depth against the bug, and return a
versioned summary bound to the checkpoint digest, snapshot digest, tick, VM count,
per-device state entry counts, pending observation count, and fault operation
sequence. It MUST fail when the bug has no reference or the artifact is invalid,
and MUST NOT execute a VM or claim replay/admissibility.

#### Scenario: Parent summary
- GIVEN a bug with a valid snapshot reference
- WHEN the snapshot is inspected
- THEN the summary carries the snapshot digest and decoded shape fields

#### Scenario: Missing reference
- GIVEN a bug without a snapshot reference
- WHEN inspection runs
- THEN it fails with a distinct not-found outcome

### Requirement: Unique snapshot references can be listed without decoding

r[chaoscontrol.agent_investigation.list_snapshots]

A listing operation MUST group checkpoint snapshot references by path and return the
digest, store, codec, depth, referencing bug indices, and artifact names without
reading artifact bytes. References sharing a path with differing digests or depths
MUST be rejected as a conflict. The record MUST carry the checkpoint digest and
state that presence is not admission or replay evidence.

#### Scenario: Shared reference grouped
- GIVEN several bugs referencing one snapshot path
- WHEN references are listed
- THEN one group carries all referencing bug indices and artifact names

#### Scenario: Conflicting metadata
- GIVEN one path with two different digests
- WHEN references are listed
- THEN the listing fails with a conflict error

### Requirement: A failure's restore inputs can be planned and checked

r[chaoscontrol.agent_investigation.restore_plan]

A planning operation MUST emit a versioned read-only restore plan for one selected
failure, binding the checkpoint digest and bug identity to the snapshot reference/
path presence, declared kernel/initrd/disk paths and their presence, and a readiness
verdict with an explicit missing list. It MUST NOT execute a VM, verify snapshot
digests, or claim admission, replay, or workload correctness.

#### Scenario: All declared inputs present
- GIVEN a failure whose snapshot file and declared kernel/initrd exist
- WHEN the plan is requested
- THEN it reports ready with an empty missing list

#### Scenario: Missing input
- GIVEN a declared kernel path that does not exist
- WHEN the plan is requested
- THEN readiness is false and the missing list names the kernel path

### Requirement: Restore planning can verify the snapshot on request

r[chaoscontrol.agent_investigation.restore_plan_verification]

Restore planning MUST support an explicit verification mode that validates the
referenced snapshot artifact's digest and depth before reporting readiness, and MUST
report whether verification ran. In the default mode the plan MUST NOT decode the
artifact, and an invalid or missing artifact MUST NOT be reported as verified.

#### Scenario: Verification requested
- GIVEN a valid stored snapshot artifact
- WHEN planning runs in verification mode
- THEN the plan reports snapshot_verified=true and ready=true

#### Scenario: Verification failure
- GIVEN a missing or invalid artifact
- WHEN planning runs in verification mode
- THEN planning fails rather than reporting a ready, unverified plan

### Requirement: A restore plan identifies a disposable branch

r[chaoscontrol.agent_investigation.branch_identity]

A restore plan MUST carry a deterministic branch identity derived with
length-delimited BLAKE3 input from the checkpoint digest, snapshot digest (or an
explicit absence marker), checkpoint index, recorded tick, dedup key (or absence),
and replay parent depth. Repeating the same inputs MUST yield the same identity, and
changing any bound field MUST change it. The identity MUST NOT be presented as an
executed branch, isolation proof, or replay evidence.

#### Scenario: Stable identity
- GIVEN one failure moment
- WHEN planning runs twice
- THEN both plans carry the same branch identity

#### Scenario: Field-bound identity
- GIVEN the same session but a different tick or bug index
- WHEN planning runs
- THEN the branch identity differs

### Requirement: A failure's parent can be restored in a fresh process

r[chaoscontrol.agent_investigation.restore_probe]

A restore probe MUST build fresh VMs from the checkpoint's declared configuration,
restore the validated parent snapshot, and report the restored tick, VM statuses,
vCPU MP states, general-purpose registers, and a bounded guest-memory probe without
executing guest instructions afterward. It MUST fail on a missing reference, invalid
artifact, depth mismatch, or controller/restore error rather than reporting state.
The record MUST state that restore is not execution, replay, or a verdict.

#### Scenario: Restore and inspect
- GIVEN a valid checkpoint, kernel/initrd paths, and snapshot artifact
- WHEN the probe runs
- THEN fresh VMs restore the snapshot and report statuses, registers, and a bounded memory probe

#### Scenario: Restore failure
- GIVEN an invalid or missing parent artifact
- WHEN the probe runs
- THEN it fails before reporting any restored state

### Requirement: A restored parent can be sought forward by bounded rounds

r[chaoscontrol.agent_investigation.restore_forward_seek]

After restore, the probe MUST accept a bounded round count (capped at 1000), execute
that many scheduling rounds through the controller, stop early when no VM is
running, and report each round's tick, running VM count, and fired-fault count plus
the final tick and state. It MUST surface execution errors and MUST NOT claim a
replay verdict, workload correctness, or an accepted outcome.

#### Scenario: Bounded forward seek
- GIVEN a restored parent
- WHEN the probe runs N rounds
- THEN the record lists each executed round and the final tick/state

#### Scenario: Early stop
- GIVEN all VMs stop during the seek
- WHEN a round reports no running VMs
- THEN the probe stops and reports the partial round trace

### Requirement: A restored parent can be sought to a target tick

r[chaoscontrol.agent_investigation.restore_seek_tick]

The probe MUST accept a target simulation tick and execute bounded rounds until the
current tick reaches or passes it (capped at 1000 rounds), stopping early when no VM
runs. It MUST report the target, whether it was reached, the executed round trace,
and the final tick/state. A request that cannot reach the target within the cap MUST
report reached=false rather than looping or claiming success.

#### Scenario: Target reached
- GIVEN a restored parent below the target tick
- WHEN the probe seeks to the target
- THEN it stops at or after the target and reports reached=true with the trace

#### Scenario: Cap or early stop
- GIVEN a target beyond the round cap or a stopped simulation
- WHEN the probe seeks
- THEN it stops and reports reached=false or the partial trace

### Requirement: Restore preflights the declared topology

r[chaoscontrol.agent_investigation.restore_shape_preflight]

Before creating VMs, a restore MUST compare the decoded snapshot's VM count with the
checkpoint's declared `num_vms` and fail with a distinct message on mismatch rather
than attempting a partial restore. The check MUST run before controller creation and
MUST NOT be bypassed by snapshot verification mode.

#### Scenario: Matching topology
- GIVEN a snapshot whose VM count equals the declared topology
- WHEN restore preflight runs
- THEN it proceeds

#### Scenario: Mismatched topology
- GIVEN a snapshot whose VM count differs from the declared topology
- WHEN restore preflight runs
- THEN restore fails before any VM is created

### Requirement: Restore-probe memory inspection is bounded and explicit

r[chaoscontrol.agent_investigation.restore_probe_memory_window]

The probe MUST accept an explicit guest-physical address and length, reject lengths
outside 1..=4096 and address-plus-length overflow before restore, and report the
address, length, and bytes (or a distinct read error) for each VM. The default
window MUST remain 16 bytes at 0x1000.

#### Scenario: Bounded explicit window
- GIVEN a restored parent and a valid address/length
- WHEN the probe inspects memory
- THEN the record carries the requested address and length with the bytes

#### Scenario: Invalid window
- GIVEN a zero/oversized length or an overflowing range
- WHEN the probe starts
- THEN it fails before creating VMs

### Requirement: A disposable branch accepts an explicit typed register nudge

r[chaoscontrol.agent_investigation.restore_intervention]

After restore, the probe MUST accept an explicit `NAME=VALUE` register intervention
limited to general-purpose registers, RSP, RIP, and RFLAGS; reject unknown names and
invalid values before loading a snapshot; and apply it to one selected VM's vCPU 0.
It MUST record the intervention, mark the record counterfactual, and expose the
resulting register values. Applying an intervention MUST NOT alter the checkpoint or
snapshot artifacts.

#### Scenario: Register nudge recorded
- GIVEN a restored parent and a valid register assignment
- WHEN the probe applies it
- THEN the record marks counterfactual=true, lists the intervention, and reports the new value

#### Scenario: Unsupported register
- GIVEN an assignment naming an unsupported register
- WHEN the probe starts
- THEN it fails before loading a snapshot

### Requirement: Seek traces align recorded fault events to ticks

r[chaoscontrol.agent_investigation.restore_seek_event_alignment]

Each executed seek round MUST report its tick, running VM count, fired-fault count,
and up to 20 fired-fault descriptions. The record MUST therefore expose which
recorded fault events occurred at which tick during the seek; it MUST NOT claim that
these events are the complete run trace or a replay verdict.

#### Scenario: Fault at a seek tick
- GIVEN a restored parent with pending scheduled faults
- WHEN the probe seeks across a fault tick
- THEN that round's entry names the fired fault and tick

### Requirement: Seek results can be checked against expected register values

r[chaoscontrol.agent_investigation.restore_register_expectation]

The probe MUST accept an expected register assignment and compare it with the
selected VM's post-seek value, recording the register, expected value, actual value,
and match, and MUST exit non-zero on mismatch after writing the record. Invalid
assignments MUST be rejected before loading a snapshot. A match is a drift check for
one value, not proof of deterministic replay.

#### Scenario: Matching expectation
- GIVEN a restored branch with a known register value
- WHEN the probe checks the expectation
- THEN the record reports matched=true and exits zero

#### Scenario: Mismatch
- GIVEN an expected value different from the actual value
- WHEN the probe checks the expectation
- THEN the record reports matched=false and the command exits non-zero

### Requirement: Repeated probes report trace drift

r[chaoscontrol.agent_investigation.restore_repeat_drift]

The probe MUST accept a bounded repeat count (1-3) and rerun the same restore,
intervention, and seek from the checkpoint, comparing executed round traces, final
VM observations, and current tick. It MUST record whether rounds, VMs, and ticks
match, mark drift, and exit non-zero on drift. A matching pair is bounded
self-consistency evidence for this checkpoint and host, not proof of universal
determinism.

#### Scenario: Matching repeats
- GIVEN the same checkpoint and probe arguments
- WHEN the probe runs twice
- THEN round traces, VM observations, and tick match with drift=false

#### Scenario: Drift detected
- GIVEN two repeat runs whose traces differ
- WHEN the probe compares them
- THEN the record marks drift and the command exits non-zero

### Requirement: A probe can emit a candidate-validation receipt

r[chaoscontrol.agent_investigation.restore_candidate_receipt]

The probe MUST optionally emit a versioned receipt binding the candidate source
revision, checkpoint content digest, seek verdict, executed rounds, repeat count and
drift result, and expectation result under one BLAKE3 receipt digest, verified before
writing. Missing revisions, malformed digest facts, invalid repeat counts, or digest
mismatches MUST fail. The receipt records bounded observations; it does not prove
correctness or release eligibility.

#### Scenario: Receipt binds a passing probe
- GIVEN a candidate revision and completed probe
- WHEN a receipt is requested
- THEN the receipt verifies and records the bound facts

#### Scenario: Receipt fact rejected
- GIVEN an empty revision or malformed checkpoint digest
- WHEN the receipt is built
- THEN the probe fails without writing a receipt

### Requirement: Drift reports the first differing observation

r[chaoscontrol.agent_investigation.restore_repeat_drift_detail]

When repeated probe records differ, the drift report MUST name the first differing
observation: the current tick, or the index and tick of the first differing round
entry, or the index of the first differing final VM entry. Matched records MUST report
no difference. The detail is diagnostic and MUST NOT claim a cause or a determinism
verdict beyond the bounded comparison.

#### Scenario: Round drift names the tick
- GIVEN two records whose second round entry differs
- WHEN drift is reported
- THEN the detail names kind `round`, the index, and the round tick

#### Scenario: Matched records
- GIVEN two identical records
- WHEN drift is reported
- THEN the difference is absent

### Requirement: Probes expose per-vCPU state and select the intervention target

r[chaoscontrol.agent_investigation.restore_smp_observations]

Every VM observation MUST list each vCPU's register state, and the register
intervention and expectation MUST accept an explicit vCPU index validated against the
selected VM. Invalid VM or vCPU indices MUST fail before recording a result. The
observation is bounded state reporting for SMP runs; it does not prove scheduling
determinism.

#### Scenario: SMP state reported
- GIVEN a restored VM with multiple vCPUs
- WHEN the probe records observations
- THEN each vCPU appears with its own register state

#### Scenario: Invalid vCPU rejected
- GIVEN a vCPU index at or beyond the VM's count
- WHEN an intervention or expectation is requested
- THEN the probe fails without recording a result

### Requirement: Moment references bind a branch identity and declare granularity

r[chaoscontrol.agent_investigation.moment_reference_branch_granularity]

A moment reference MUST bind the checkpoint digest and index, snapshot digest when
present, failure tick, selector and parent depth into one deterministic branch
identity, and MUST declare the supported seek granularity as the recorded tick.
Repeated binding of the same evidence MUST produce the same branch identity. The
identity does not create, isolate, or replay a branch.

#### Scenario: Repeated binding is stable
- GIVEN the same checkpoint and selector
- WHEN the moment is bound twice
- THEN both references carry the same branch identity and `tick` granularity

#### Scenario: Identity separates moments
- GIVEN a different tick, selector, or parent surface
- WHEN the moment is bound
- THEN the branch identity differs

### Requirement: A restore probe records the disposable branch identity

r[chaoscontrol.agent_investigation.restore_probe_branch_binding]

The restored parent snapshot MUST expose the same deterministic branch identity used
by moment references and restore plans, and each probe record MUST include it. The
probe and its receipt MUST therefore name the branch before any intervention. The
identity labels the disposable branch; it does not create, isolate, persist, or
replay one.

#### Scenario: Probe names the branch
- GIVEN a checkpoint failure with a replay parent snapshot
- WHEN the probe restores it
- THEN the record and receipt carry the branch identity from the parent binding

#### Scenario: Identity matches moment binding
- GIVEN the same checkpoint and selector
- WHEN a moment reference and a probe record are produced
- THEN both branch identities are equal

### Requirement: Failure listing filters by owner scope and identity admission

r[chaoscontrol.agent_investigation.list_bugs_owner_identity_filter]

Failure listing MUST accept an exact runtime owner process instance and an
identity-required flag in addition to assertion id, dedup key, and bounded tick range.
An owner filter MUST match only bugs whose owner set contains the exact instance, and
identity-required MUST match only bugs carrying an admitted assertion identity.
Filters combine conjunctively and do not write artifacts.

#### Scenario: Owner filter selects one bug
- GIVEN bugs with different runtime owner sets
- WHEN listing filters by one process instance
- THEN only bugs owned by that instance are returned

#### Scenario: Identity filter excludes legacy bugs
- GIVEN a mix of identity-admitted and legacy bugs
- WHEN identity-required is set
- THEN only bugs with an admitted identity are returned

### Requirement: Failure listing filters by exact assertion fingerprint

r[chaoscontrol.agent_investigation.list_bugs_assertion_fingerprint_filter]

Failure listing MUST accept an exact admitted assertion fingerprint as lower hex and
MUST reject malformed values before loading results. A fingerprint filter MUST match
only bugs whose admitted identity carries exactly that fingerprint and MUST exclude
legacy identity-less bugs. The filter identifies assertion evidence; it does not
select a replay verdict or authorize a branch.

#### Scenario: Fingerprint selects recorded failures
- GIVEN failures with admitted assertion identities
- WHEN listing filters by one fingerprint
- THEN only bugs carrying that exact fingerprint are returned

#### Scenario: Malformed fingerprint rejected
- GIVEN a value that is not a valid fingerprint hex string
- WHEN listing filters by it
- THEN the typed rejection is returned

### Requirement: Interventions record their exact boundary and prior value

r[chaoscontrol.agent_investigation.restore_intervention_boundary]

Each applied register intervention MUST record the selected VM and vCPU, register,
the value before mutation, the applied value, and the boundary label
`after_restore_before_seek`. The prior value MUST be read from the same vCPU before
mutation. The record labels a disposable-branch mutation; it does not prove replay
or isolate the branch.

#### Scenario: Nudge records before and after
- GIVEN a restored VM and an allowed register assignment
- WHEN the probe applies it
- THEN the record names the prior and applied values and the pre-seek boundary

#### Scenario: Boundary is not implied by later execution
- GIVEN a subsequent seek
- WHEN the record is inspected
- THEN the intervention still reports the pre-seek boundary label

### Requirement: Seek stops are classified distinctly

r[chaoscontrol.agent_investigation.restore_seek_outcome]

A probe record MUST classify why the seek stopped as one of `reached`, `no_target`,
`round_cap`, or `all_vms_stopped`. A reached target MUST take precedence; a missing
target MUST remain distinct from an exhausted round bound, and a stopped VM set MUST
remain distinct from both. The classification describes the bounded seek and is not
a replay or failure verdict.

#### Scenario: Reached target
- GIVEN a seek that reaches its requested tick
- WHEN the record is produced
- THEN the outcome is `reached`

#### Scenario: Round cap and stopped VMs differ
- GIVEN an unreached target with an exhausted cap or no running VM
- WHEN the record is produced
- THEN the outcome names the matching distinct class

### Requirement: One failure can be addressed by exact assertion fingerprint

r[chaoscontrol.agent_investigation.selector_assertion_fingerprint]

Inspect, moment-binding, and restore-probe commands MUST accept exactly one selector:
index, dedup key, or exact admitted assertion fingerprint. A fingerprint selection
MUST resolve to the single matching bug, reject malformed fingerprints, and return
distinct not-found or ambiguous errors. Resolution is read-only and does not write
artifacts, select a replay verdict, or authorize a branch.

#### Scenario: Unique fingerprint resolves
- GIVEN exactly one bug carries the fingerprint
- WHEN a command selects by it
- THEN the resolved checkpoint index is used

#### Scenario: Ambiguous or missing fingerprint
- GIVEN zero or several bugs carry the fingerprint
- WHEN a command selects by it
- THEN the matching distinct error is returned

### Requirement: Checkpoints record SMP topology and restore validates it

r[chaoscontrol.agent_investigation.restore_smp_topology_admission]

A checkpoint MUST record the vCPU count per VM and any extra guest cmdline used by
the run, defaulting to one vCPU and no extra cmdline for older checkpoints. Restore
MUST rebuild VMs with that topology, pass the recorded cmdline, and reject a snapshot
whose per-VM vCPU count differs from the declared topology. The admission checks
structural agreement only; it does not prove SMP scheduling determinism.

#### Scenario: SMP checkpoint restores
- GIVEN a checkpoint written with two vCPUs and an extra cmdline
- WHEN the probe restores it
- THEN the VM is built with two vCPUs and the recorded cmdline

#### Scenario: Mismatched snapshot rejected
- GIVEN a snapshot whose vCPU count differs from the checkpoint declaration
- WHEN restore validates topology
- THEN it fails with the observed and expected counts

### Requirement: Checkpoint vCPU declarations stay within supported bounds

r[chaoscontrol.agent_investigation.checkpoint_smp_topology_bounds]

Checkpoint admission MUST reject a declared vCPU count of zero or greater than the
scheduler's supported maximum, and MUST accept counts within that range. This is a
structural bound on the recorded topology, not proof that a run used or scheduled
those vCPUs deterministically.

#### Scenario: Out-of-range declarations rejected
- GIVEN a checkpoint declaring zero or excess vCPUs
- WHEN checkpoint admission runs
- THEN it fails with an invalid-bounds error

#### Scenario: Supported declaration admitted
- GIVEN a checkpoint declaring two vCPUs
- WHEN checkpoint admission runs
- THEN it succeeds

### Requirement: Restore plans and snapshot inspections expose SMP topology

r[chaoscontrol.agent_investigation.restore_plan_topology_surface]

A restore plan MUST report the declared vCPU count and any extra guest cmdline. A
snapshot inspection MUST report the declared vCPU count, the first stored VM's
observed vCPU count when one exists, and whether they match. Absent observations MUST
be distinguishable from mismatches. These are structural facts; they do not prove
SMP scheduling determinism or replay admissibility.

#### Scenario: SMP plan and inspection
- GIVEN a checkpoint declaring two vCPUs and a cmdline
- WHEN the plan and inspection are emitted
- THEN both report two vCPUs and the inspection reports a match

#### Scenario: Empty snapshot surface
- GIVEN an inspection with no stored VM
- WHEN it is emitted
- THEN observed count and match are absent rather than false

### Requirement: Failure listings expose the exact assertion fingerprint

r[chaoscontrol.agent_investigation.list_bugs_identity_surface]

Each listed failure MUST include its admitted assertion fingerprint as lower hex when
an identity is present, and MUST omit the field for legacy identity-less bugs. The
fingerprint MUST match the value accepted by exact-identity selection. Listing exposes
identity for addressing; it does not admit replay or a verdict.

#### Scenario: Identity-bearing failure
- GIVEN a bug with an admitted assertion identity
- WHEN it is listed
- THEN its fingerprint is present and selectable by the same value

#### Scenario: Legacy failure
- GIVEN a bug without an admitted identity
- WHEN it is listed
- THEN no fingerprint field is emitted

### Requirement: Candidate receipts bind seek outcome and intervention count

r[chaoscontrol.agent_investigation.receipt_seek_intervention_facts]

A candidate receipt MUST record the classified seek outcome and the number of applied
register interventions, and MUST reject unknown seek-outcome labels. These facts bind
the bounded branch behavior the receipt describes; they do not prove replay or
candidate correctness.

#### Scenario: Receipt binds branch facts
- GIVEN a probe with no seek target and one applied intervention
- WHEN a receipt is written
- THEN it reports `no_target` and an intervention count of one

#### Scenario: Unknown outcome rejected
- GIVEN an unknown seek-outcome label
- WHEN a receipt is built
- THEN the typed rejection is returned

### Requirement: Candidate receipts link bounded diagnostic results

r[chaoscontrol.agent_investigation.receipt_diagnostic_links]

A candidate receipt MUST accept zero or more diagnostic receipt files, decode and
admit each one, and bind their canonical receipt digests as sorted unique `b3:`
values. Malformed diagnostic receipts or non-BLAKE3 digests MUST be rejected before a
candidate receipt is written. The links identify bounded diagnostic results; they do
not prove the candidate fix or replay fidelity.

#### Scenario: Diagnostic result linked
- GIVEN a valid diagnostic receipt file
- WHEN the candidate receipt is written
- THEN its digest appears in the candidate receipt's link list

#### Scenario: Malformed link rejected
- GIVEN a file that is not a valid diagnostic receipt
- WHEN the candidate receipt is requested
- THEN the command fails without writing a receipt

### Requirement: Candidate receipts bind the original failure identity

r[chaoscontrol.agent_investigation.receipt_failure_identity]

A candidate receipt MUST optionally bind the recorded bug artifact, bug id, stored
dedup key, and failure tick, recomputing its digest after binding. An empty artifact
MUST be rejected. These facts identify the original failure the candidate was tested
against; they do not prove the failure is fixed.

#### Scenario: Failure identity bound
- GIVEN a probe of a recorded failure
- WHEN the receipt is written
- THEN it names the artifact, bug id, dedup key, and failure tick

#### Scenario: Empty artifact rejected
- GIVEN an empty artifact name
- WHEN failure identity is bound
- THEN the typed rejection is returned

### Requirement: Candidate guest artifacts receive fresh content identities

r[chaoscontrol.agent_investigation.candidate_artifact_identity]

An explicit operation MUST emit `b3:` BLAKE3 identities, byte sizes, and kinds for the
supplied candidate kernel, initrd, and disk image, failing on unreadable paths and
omitting absent optional artifacts. The output MUST state that identity alone does not
prove a build, boot, or fix and that a fresh run is required.

#### Scenario: Candidate identities emitted
- GIVEN readable candidate kernel and initrd files
- WHEN identities are requested
- THEN both appear with `b3:` digests and sizes under one schema

#### Scenario: Missing artifact rejected
- GIVEN an unreadable or absent required path
- WHEN identities are requested
- THEN the command fails without emitting a partial record

### Requirement: A fresh bounded campaign validates candidate artifacts

r[chaoscontrol.agent_investigation.candidate_validation_campaign]

An explicit operation MUST bind an original failure, fresh candidate artifact
identities, and a bounded profile (seed, rounds, ticks, vCPUs) to a fresh exploration
run under a separate output directory, then emit a digest-verified receipt recording
the process exit, completion, and observed bug count. Exit codes 0 and 1 (completed,
with or without findings) MUST count as completed; other codes MUST not. The receipt
MUST NOT claim the original failure is fixed or that the old snapshot replayed.

#### Scenario: Candidate run completes with findings
- GIVEN readable candidate artifacts and a bounded profile
- WHEN validation runs
- THEN a fresh checkpoint is produced and the receipt records completion and bug count

#### Scenario: Run failure
- GIVEN a fresh run that exits outside 0/1
- WHEN the receipt is emitted
- THEN completed is false and the command exits non-zero

### Requirement: Candidate validation requires a fresh checkpoint

r[chaoscontrol.agent_investigation.candidate_validation_checkpoint_gate]

A candidate validation run MUST count as completed only when the process exits with
code 0 or 1 and a fresh checkpoint exists in its output directory. An exit code alone
MUST NOT mark completion, and a missing checkpoint MUST leave the observed bug count
absent. The command MUST exit non-zero for an incomplete run while still writing the
receipt.

#### Scenario: Fresh checkpoint present
- GIVEN a run that exits 0 or 1 and writes a checkpoint
- WHEN the receipt is built
- THEN completion is true and the bug count is recorded

#### Scenario: No checkpoint written
- GIVEN a run that exits 0 or 1 without producing a checkpoint
- WHEN the receipt is built
- THEN completion is false and the command exits non-zero
