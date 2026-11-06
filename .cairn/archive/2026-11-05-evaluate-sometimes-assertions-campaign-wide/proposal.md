# Evaluate Sometimes Assertions Campaign-Wide

## Why

`AssertionRecord::verdict` is documented as the cross-run verdict (`oracle.rs:125`). A `sometimes` assertion fails when it was hit but never true. `Explorer::extract_bugs` (`explorer.rs:1464`) applies that verdict to one branch's oracle report and turns every `Failed` record into a `BugReport`. A liveness-style `sometimes` assertion that one bounded branch did not satisfy therefore becomes a bug.

A 2026-09-25 local Raft hunt shows the noise. The run used seed 42, 10 rounds, 8 branches, and 2000 ticks per branch. The control variant, which has no planted bug, reported 58 bugs, and all 58 were `sometimes` assertions such as `leader elected` and `value committed`. Each planted variant that produced no safety failure still reported 40 to 56 `sometimes` bugs. The bug count, bug files, the explorer's exit status, and `scripts/bug-hunt.sh` all treated those records as findings.

The bug identity also multiplies one failure. The dedup key combines the assertion fingerprint with the branch's sorted fault-type set, so one safety failure produced 27 separate bug records.

## What Changes

- Create per-branch bug records only for safety violations: an `always` assertion observed false or an `unreachable` assertion reached. r[chaoscontrol.exploration_findings.safety_bugs]
- Evaluate `sometimes` and `reachable` assertions over the whole campaign. Report an assertion that no branch satisfied as a coverage gap, not as a bug. r[chaoscontrol.exploration_findings.campaign_liveness]
- Report distinct failing assertion identities next to the bug-record count. r[chaoscontrol.exploration_findings.distinct_counts]
- Keep the campaign assertion summary and replay contracts unchanged for safety findings. r[chaoscontrol.exploration_findings.compatibility]

## Impact

- **Code**: explorer bug extraction, campaign aggregation, the human report, the CLI exit decision, and `scripts/bug-hunt.sh`.
- **Evidence**: bug files for `sometimes` and `reachable` assertions stop appearing. Assertion-readiness reports still show campaign-level `sometimes` verdicts.
- **Compatibility**: replay of safety bugs is unchanged. Consumers that counted `sometimes` bug files lose those records.

## Non-Goals

- No change to guest SDK macros or assertion identity.
- No change to the dedup key used for replay candidates. This change only adds a distinct-assertion count.
- No change to coverage enrichment, which still hashes per-branch assertion states into the bitmap.
