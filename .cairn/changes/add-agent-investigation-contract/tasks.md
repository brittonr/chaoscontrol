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
- [x] [serial] Add versioned checkpoint admission that validates structure and unique referenced snapshot artifacts with a fail-closed verdict. r[chaoscontrol.agent_investigation.verify_checkpoint]
- [x] [serial] Add a structure-only admission mode distinct from full snapshot validation. r[chaoscontrol.agent_investigation.admission_modes]
- [x] [serial] Bind admission to an expected BLAKE3 session digest with a typed mismatch failure. r[chaoscontrol.agent_investigation.session_digest_binding]
- [ ] [serial] Read-only investigation MVP: admit a retained session in a fresh process and expose locate/seek/inspect through versioned JSON requests. r[chaoscontrol.agent_investigation.bug_event_reference]
- [ ] [serial] Branch creation and intervention recording with disposable branch identity and original-immutability tests. r[chaoscontrol.agent_investigation.bug_event_reference]
- [ ] [serial] Opt-in guest diagnostic operation bound to the existing Rust guest supervisor, with admission, bounds, and artifact closure. r[chaoscontrol.agent_investigation.bug_event_reference]
- [ ] [serial] Candidate-build validation campaign and reviewer receipt over original failure, replay, branch, and positive/negative candidates. r[chaoscontrol.agent_investigation.bug_event_reference]
- [ ] [serial] Update the ChaosControl workflow skill and documentation for implemented operations only. r[chaoscontrol.agent_investigation.bug_event_reference]

## Evidence

Implementation commit and exact verification are recorded in the repository history.
The implemented slice adds `dedup_key` and `replay_parent_depth` to `BugFound`,
plumbs them from the emitted `BugReport`, and adds a `--dedup-key` filter to
`export-bugs`. It does not implement session admission, moment addressing, branch
creation, guest diagnostic execution, intervention persistence, candidate
validation, receipts, or any MCP/HTTP adapter. A matching key is a correlation
field; it does not prove run, VM, or history identity, snapshot admissibility, or
replay reproduction.
