# Onboard a Rust Workload

Goal: scaffold one reviewed property through the external harness, then close instrumentation gaps before a VM campaign.

## Prerequisites

- The target is a Rust project.
- One reviewed candidate property is selected.
- The ChaosControl source is known.
- The target lifecycle permits the planned writes.

Read `docs/rust-workload-harness.md` and `docs/templates/rust-workload/README.md` in the ChaosControl source.

## Workflow

1. Run the relevant target tests before a core source change.
2. Start with the external harness and keep the service source unchanged.
3. Scaffold the workload with the repository command:

```bash
nix run path:$CHAOSCONTROL_SOURCE#scaffold-rust-workload -- "$OUTPUT" "$SERVICE"
```

4. Read `chaoscontrol-scaffold.json` from the generated output.
5. Run its local dry-run, report, and assertion-quality commands exactly.
6. Read registered, observed, unobserved, failing, uncategorized, and progress assertion rows.
7. Add in-process assertions only when a reviewed invariant is not observable through public behavior.
8. Run the target tests again after a core source change.

Use feature or configuration gates for in-process instrumentation. Tag external observations as `external-harness` and internal observations as `in-process-service`.

## Assertion rules

- Use `always` for safety properties.
- Use `sometimes` for meaningful progress conditions.
- Use `reachable` for distinct outcomes or branches.
- Use `unreachable` for forbidden paths.
- Keep assertion messages as unique constant strings.
- Include bounded structured details that help replay triage.
- Draw bounded inputs from property-specific boundary and configured-limit families.

Do not use `sometimes(true)` as a progress assertion. Use `reachable` for unconditional path observation.

## Positive and negative paths

For each selected property, exercise:

- The expected successful path.
- The expected assertion failure or rejection path.
- Malformed or missing inputs.
- Configured boundaries.
- Transient errors under the declared idempotency rules.
- Unavailable internal state when the external harness cannot observe it.

## Completion boundary

A passing local report proves instrumentation shape only. It does not prove VM execution, fault coverage, deterministic replay, or snapshot-backed reproduction.

Do not start a VM campaign until the assertion-quality command passes or records an exact reviewed blocker.
