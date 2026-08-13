## Why

Evidence plans currently execute operator-authored command text through `sh -c`. This boundary allows shell parsing and weakens executable, argument, environment, and receipt identity.

## What Changes

- Replace free-form command strings with typed executable, argument, directory, environment, input, and limit fields.
- Adopt the published `bounded-exec` process mechanism at an exact revision.
- Keep executable authorization, artifact identity, evidence meaning, and release policy in ChaosControl.
- Bind admitted command facts and observed outcomes into receipts.
- Reject legacy free-form plans for evidence-eligible execution.

## Impact

- **Code**: evidence plan DTOs, plan validation core, process shell, receipts, and renderers.
- **Configuration**: typed Nickel command-plan contracts and deterministic exports.
- **Dependency**: exact published `bounded-exec` revision, subject to adoption checks.
- **Testing**: literal arguments, bounds, timeouts, output floods, environment policy, teardown, and legacy rejection.

## Non-Goals

- No sandbox, hermeticity, or executable trust claim.
- No general shell language replacement.
- No execution of legacy command text during diagnostic import.
