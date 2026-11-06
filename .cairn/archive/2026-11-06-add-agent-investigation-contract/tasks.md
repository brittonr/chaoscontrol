## Tasks

- [x] [serial] Admit the agent investigation contract natively and record issue #1's phases, ownership, and non-goals. r[chaoscontrol.agent_investigation.bug_event_reference]
- [x] [serial] Add the stable failure reference to `BugFound` and select checkpoint exports by dedup key, with regressions. r[chaoscontrol.agent_investigation.bug_event_reference]
- [x] [serial] Verify focused tests, formatting, and denied-warning Clippy for the touched crates. r[chaoscontrol.agent_investigation.export_by_dedup_key]
- [x] [serial] Add a noninteractive checkpoint bug listing with stable records and assertion/dedup/tick filters. r[chaoscontrol.agent_investigation.locate_bugs]
- [x] [serial] Add single-failure read-only JSON inspection selected by checkpoint index or dedup key. r[chaoscontrol.agent_investigation.inspect_bug]
- [x] [serial] Bind a failure to a stable read-only moment reference (checkpoint digest, bug identity, recorded tick, snapshot digest). r[chaoscontrol.agent_investigation.moment_reference]
- [x] [serial] Add versioned JSON seek/inspect for recorded sessions with bounded windows and non-claims. r[chaoscontrol.agent_investigation.debug_state_json]
- [x] [serial] Decode and summarize a failure's referenced replay-parent snapshot without executing a VM. r[chaoscontrol.agent_investigation.inspect_snapshot]
- [x] [serial] List unique snapshot references and referencing bugs without decoding artifacts, rejecting path/digest conflicts. r[chaoscontrol.agent_investigation.list_snapshots]
- [x] [serial] Report a read-only restore plan with declared-input and snapshot-path presence. r[chaoscontrol.agent_investigation.restore_plan]
- [x] [serial] Add an explicit snapshot verification mode to restore planning. r[chaoscontrol.agent_investigation.restore_plan_verification]
- [x] [serial] Add a deterministic disposable-branch identity to the restore plan. r[chaoscontrol.agent_investigation.branch_identity]
- [x] [serial] Restore a failure's parent in fresh VMs and report read-only live state. r[chaoscontrol.agent_investigation.restore_probe]
- [x] [serial] Seek forward from a restored parent by bounded scheduling rounds. r[chaoscontrol.agent_investigation.restore_forward_seek]
- [x] [serial] Seek to a target simulation tick with a bounded round cap and reached verdict. r[chaoscontrol.agent_investigation.restore_seek_tick]
- [x] [serial] Align fired fault descriptions to seek-round ticks. r[chaoscontrol.agent_investigation.restore_seek_event_alignment]
- [x] [serial] Check an expected post-seek register value with a non-zero mismatch exit. r[chaoscontrol.agent_investigation.restore_register_expectation]
- [x] [serial] Repeat a probe and report round/VM/tick trace drift. r[chaoscontrol.agent_investigation.restore_repeat_drift]
- [x] [serial] Emit a digest-bound candidate-validation receipt. r[chaoscontrol.agent_investigation.restore_candidate_receipt]
- [x] [serial] Bind the loaded parent and probe record to the disposable branch identity. r[chaoscontrol.agent_investigation.restore_probe_branch_binding]
- [x] [serial] Filter failure listings by exact owner process instance and identity admission. r[chaoscontrol.agent_investigation.list_bugs_owner_identity_filter]
- [x] [serial] Filter failure listings by exact admitted assertion fingerprint. r[chaoscontrol.agent_investigation.list_bugs_assertion_fingerprint_filter]
- [x] [serial] Record each intervention's prior value and exact pre-seek boundary. r[chaoscontrol.agent_investigation.restore_intervention_boundary]
- [x] [serial] Classify seek stop reasons distinctly in the probe record. r[chaoscontrol.agent_investigation.restore_seek_outcome]
- [x] [serial] Resolve a single failure by exact assertion fingerprint in inspect, moment, and probe commands. r[chaoscontrol.agent_investigation.selector_assertion_fingerprint]
- [x] [serial] Record SMP topology and extra cmdline in checkpoints and validate them on restore. r[chaoscontrol.agent_investigation.restore_smp_topology_admission]
- [x] [serial] Bound the recorded vCPU declaration against the scheduler maximum. r[chaoscontrol.agent_investigation.checkpoint_smp_topology_bounds]
- [x] [serial] Expose declared and observed SMP topology in restore plans and snapshot inspections. r[chaoscontrol.agent_investigation.restore_plan_topology_surface]
- [x] [serial] Expose the exact admitted assertion fingerprint in failure listings. r[chaoscontrol.agent_investigation.list_bugs_identity_surface]
- [x] [serial] Bind seek outcome and intervention count into candidate receipts. r[chaoscontrol.agent_investigation.receipt_seek_intervention_facts]
- [x] [serial] Link admitted diagnostic receipt digests into candidate receipts. r[chaoscontrol.agent_investigation.receipt_diagnostic_links]
- [x] [serial] Bind the original failure artifact, bug id, dedup key, and tick into candidate receipts. r[chaoscontrol.agent_investigation.receipt_failure_identity]
- [x] [serial] Emit fresh content identities for candidate guest artifacts. r[chaoscontrol.agent_investigation.candidate_artifact_identity]
- [x] [serial] Launch a fresh bounded validation campaign and emit a failure-bound receipt. r[chaoscontrol.agent_investigation.candidate_validation_campaign]
- [x] [serial] Gate candidate completion on a fresh checkpoint, not exit code alone. r[chaoscontrol.agent_investigation.candidate_validation_checkpoint_gate]
- [x] [serial] Expose a bounded per-VM serial tail in restore probes. r[chaoscontrol.agent_investigation.restore_serial_tail]
- [x] [serial] Bind the fresh-run checkpoint identity into candidate receipts. r[chaoscontrol.agent_investigation.candidate_validation_counterexamples]
- [x] [serial] Name the first differing tick, round, or VM observation in drift reports. r[chaoscontrol.agent_investigation.restore_repeat_drift_detail]
- [x] [serial] Expose per-vCPU state and select the vCPU for register intervention and expectation. r[chaoscontrol.agent_investigation.restore_smp_observations]
- [x] [serial] Bind moment references to a deterministic branch identity and declared tick granularity. r[chaoscontrol.agent_investigation.moment_reference_branch_granularity]
- [x] [serial] Preflight the decoded snapshot VM count against the declared topology before restore. r[chaoscontrol.agent_investigation.restore_shape_preflight]
- [x] [serial] Add explicit bounded memory-probe address/length arguments with overflow checks. r[chaoscontrol.agent_investigation.restore_probe_memory_window]
- [x] [serial] Apply an explicit typed register intervention to one restored VM and record it as counterfactual. r[chaoscontrol.agent_investigation.restore_intervention]
- [x] [serial] Add versioned checkpoint admission that validates structure and unique referenced snapshot artifacts with a fail-closed verdict. r[chaoscontrol.agent_investigation.verify_checkpoint]
- [x] [serial] Add a structure-only admission mode distinct from full snapshot validation. r[chaoscontrol.agent_investigation.admission_modes]
- [x] [serial] Bind admission to an expected BLAKE3 session digest with a typed mismatch failure. r[chaoscontrol.agent_investigation.session_digest_binding]
- [x] [serial] Read-only investigation MVP: admit a retained session in a fresh process and expose locate/seek/inspect through versioned JSON requests. r[chaoscontrol.agent_investigation.bug_event_reference]
  - Evidence: `chaoscontrol_explore::investigation` admits `chaoscontrol.investigation.request.v1` (bounded size, schema, path, tick range, and fingerprint checks) and executes exactly one `locate`, `seek`, or `inspect` operation against a retained checkpoint without executing a VM or writing artifacts. The `chaoscontrol-investigate` binary reads a request from a file or stdin and emits one bounded response or typed failure; `--request -` smoke runs produced exit 3 (missing checkpoint) and exit 2 (stale schema). Four module tests cover admission, locate/seek/inspect against a saved checkpoint fixture, response bounds, and filter mapping.
- [x] [serial] Branch creation and intervention recording with disposable branch identity and original-immutability tests. r[chaoscontrol.agent_investigation.bug_event_reference]
  - Evidence: [live branch proof](evidence/2026-11-05-live-branch-proof.md). `restore-probe` on a fresh `premature_commit` checkpoint restored parent depth 1 into a fresh controller, bound the deterministic branch identity `b3:5e4fa949…`, recorded the `rax` intervention with previous value 117, boundary `after_restore_before_seek`, and `counterfactual: true`, and matched the post-restore expectation. The checkpoint SHA-256 was unchanged before and after. `read_only_investigation_never_rewrites_the_checkpoint` covers list/inspect/moment/plan immutability.
- [x] [serial] Opt-in guest diagnostic operation bound to the existing Rust guest supervisor, with admission, bounds, and artifact closure. r[chaoscontrol.agent_investigation.bug_event_reference]
  - Evidence: [diagnostic and review](evidence/2026-11-05-diagnostic-and-review.md). The supervisor loop now serves `serve_one_diagnostic` using the first admitted executable. A live session booted `.#initrd-multiprocess`, queued one admitted request, and received a canonical receipt (`completed`, exit 1, stderr 75 bytes, no truncation, counterfactual) with request, executable, and receipt digests; zero requests remained pending. Live execution exposed and fixed two guest defects: kernel-forwarded init argv was mistaken for a manifest path, and the host-clock sleep stalled deterministic progress.
- [x] [serial] Candidate-build validation campaign and reviewer receipt over original failure, replay, branch, and positive/negative candidates. r[chaoscontrol.agent_investigation.bug_event_reference]
  - Evidence: [diagnostic and review](evidence/2026-11-05-diagnostic-and-review.md). `validate-candidate` now records the selected extra cmdline in its profile. A positive candidate (`raft_bug=premature_commit`, 6 rounds) completed with 1 bug; the negative candidate (`raft_bug=none`) completed with 0. `review-candidate` bound the original failure, replay verdict (`snapshot_backed_reproduced`, valid snapshot), probe branch identity, both candidate receipts with artifact identities, and checkpoint identities into a `chaoscontrol.candidate_review.v1` receipt with verdict `ready` and digest `blake3:2423f194…06dd`.
- [x] [serial] Update the ChaosControl workflow skill and documentation for implemented operations only. r[chaoscontrol.agent_investigation.bug_event_reference]
  - Evidence: `docs/skills/chaoscontrol-workflow/references/triage.md` gains a read-only JSON investigation section limited to the implemented `locate`/`seek`/`inspect` operations and their non-claims; `README.md` documents the same runner, its versioned request, and exit codes. Live branch, diagnostic, and candidate operations are described as separate commands that still require parent artifacts.

## Evidence

Implementation commit and exact verification are recorded in the repository history.
The implemented slice adds `dedup_key` and `replay_parent_depth` to `BugFound`,
plumbs them from the emitted `BugReport`, and adds a `--dedup-key` filter to
`export-bugs`. It does not implement session admission, moment addressing, branch
creation, guest diagnostic execution, intervention persistence, candidate
validation, receipts, or any MCP/HTTP adapter. A matching key is a correlation
field; it does not prove run, VM, or history identity, snapshot admissibility, or
replay reproduction.

## Remaining phases and blockers

- Branch creation and intervention recording exist in `restore-probe` (fresh
  controller, branch identity, prior-value/boundary intervention records), but the
  task's live immutability proof requires the recorded parent artifacts, which are
  absent in this environment.
- The guest diagnostic operation requires a bootable guest supervisor artifact;
  no admitted guest bundle is present locally.
- The candidate validation campaign and receipt exist behind `validate-candidate`,
  but exercising positive and negative candidates requires real kernel/initrd
  candidates and a completed fresh run.
