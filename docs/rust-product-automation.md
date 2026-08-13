# Rust-owned product automation

ChaosControl uses compiled Rust tools for product automation. Nix wrappers only pass explicit paths and arguments.

r[impl chaoscontrol.rust_automation.inventory] r[impl chaoscontrol.rust_automation.boundary]

## Ownership inventory

| Former Python surface | Rust owner | Inputs | Outputs and exit class | Bounds and effects | Callers and parity checks |
| --- | --- | --- | --- | --- | --- |
| `accepted-snapshot-verdict-dogfood.py` | `accepted_dogfood` core and `accepted-snapshot-verdict-dogfood` shell | Cohort, workload profile, KVM artifacts, typed command arguments | Existing summaries, proof receipt, verdict artifacts; exits 0 or 2 | Typed `bounded-exec` plans, explicit time and output limits; shell owns KVM checks, files, hashing, and process effects | Accepted-verdict Nix apps; positive and stale or malformed tests are beside the core |
| `materialize-dogfood-receipt.py` | `dogfood_receipt` core and `materialize-dogfood-receipt` shell | Checkpoint, assertion, bug, replay, and revision facts | Existing `run-config.json` and `receipt.json`; exits 0 or 1 | JSON and hash reads are bounded; shell owns reads and writes | Operator command; complete and malformed fixture tests are beside the core |
| `summarize-accepted-dogfood-output.py` | `dogfood_summary` core and `summarize-accepted-dogfood-output` shell | Accepted or attempts summary | Existing JSON or one-line summary; exits 0 or 2 | JSON reads are bounded by the selected files; shell owns reads and output | Replay-readiness wrapper; accepted, attempts, malformed, and empty tests are beside the core |
| `local-multi-hypervisor-kvm-smoke.py` | Typed command core and `local-multi-hypervisor-kvm-smoke` shell | Workloads, explicit executable paths, output path, extra dogfood arguments | Existing campaign plan, receipt, metadata, and summary; preserves child exit | Typed `bounded-exec` owns process limits and teardown; shell owns KVM and file effects | Nix app and KVM check; distinct and duplicate workload tests plus the KVM check |
| `check-cargo-audit-report.py` | `audit` core and `check-cargo-audit-report` shell | Cargo-audit report and allowlist | Existing summary or policy error; exits 0 or 1 | File reads occur in the shell; policy comparison is pure | Dependency-audit check; matched, vulnerability, untriaged, stale, and malformed tests |
| Matrix summary inline block | `vm_determinism` core and `render-vm-determinism-matrix-summary` shell | Matrix receipt | Existing summary text; exits 0 or 1 | Shell owns bounded file I/O | VM matrix app; pass and malformed row tests |
| Replay-readiness receipt inline block | `readiness_receipt` core and `materialize-replay-readiness-receipt` shell | Gate states, dogfood summary, expectations, timestamps | Existing receipt schema; exits 0 or 1 | Shell owns environment and atomic file effects | Replay-readiness app; matched, missing, and mismatched expectation tests |
| Workload scaffold inline block | `scaffold` core and `scaffold-rust-workload` shell | Destination, workload, template, source root | Existing scaffold tree and manifest; exits 0, 1, or 2 | Entry counts are bounded; shell owns copy and write effects | Scaffold app; positive, invalid-name, existing-destination, and write-failure checks |
| Drift receipt inline block | `vm_determinism` core and `check-vm-determinism-drift-receipt` shell | Drift receipt | Existing summary text; exits 0 or 1 | Shell owns bounded file I/O | Drift receipt check; accepted and tampered receipt tests |

## Compatibility boundary

Public command names, JSON field names, receipt meanings, and exit classes stay stable. Historical command strings remain display data. Rust does not split or execute them.

The migration improves ownership and testability. It does not prove orchestration correctness, KVM behavior, sandboxing, complete audit coverage, release eligibility, or absence of defects.
