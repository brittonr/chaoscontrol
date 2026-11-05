## Tasks

- [x] [serial] Admit the agent investigation contract natively and record issue #1's phases, ownership, and non-goals. r[chaoscontrol.agent_investigation.bug_event_reference]
- [x] [serial] Add the stable failure reference to `BugFound` and select checkpoint exports by dedup key, with regressions. r[chaoscontrol.agent_investigation.bug_event_reference]
- [x] [serial] Verify focused tests, formatting, and denied-warning Clippy for the touched crates. r[chaoscontrol.agent_investigation.export_by_dedup_key]
- [x] [serial] Add a noninteractive checkpoint bug listing with stable records and assertion/dedup/tick filters. r[chaoscontrol.agent_investigation.locate_bugs]
- [x] [serial] Add single-failure read-only JSON inspection selected by checkpoint index or dedup key. r[chaoscontrol.agent_investigation.inspect_bug]
- [x] [serial] Add versioned checkpoint admission that validates structure and unique referenced snapshot artifacts with a fail-closed verdict. r[chaoscontrol.agent_investigation.verify_checkpoint]
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
