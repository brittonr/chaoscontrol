# Causality analysis

Causality analysis minimizes a reproduced interleaving and ranks supplied cause candidates.

The result is bounded diagnostic evidence. A ranking is not proof of a unique cause.

## Pure core

`chaoscontrol-sim-core::causality` owns two deterministic mechanisms.

### Interleaving minimization

`DdminState` first tests the empty scheduling delta. It then applies bounded delta debugging to complements of the current step set.

The core emits one candidate at a time. A shell supplies the replay outcome for that exact BLAKE3-bound candidate.

If the execution budget ends, the result is partial and sets `budget_exhausted`. It does not claim that the current set is minimal.

### Candidate attribution

Attribution candidates use one of these classes:

- seed;
- fault schedule;
- declared event;
- variant policy.

The shell neutralizes each candidate and records whether the failure still reproduced. The core ranks candidates by the observed fraction of prevented failures.

If no neutralization changes the outcome, the report keeps equivalent rankings and emits no probable cause.

## Imperative shell

`chaoscontrol-evidence::causality_shell` owns orchestration through the `CausalityExecutor` port.

A concrete executor receives exact minimization or neutralization plans. It returns only observed replay facts.

The shell checks every execution against the admitted replay-verdict and snapshot identities. Identity drift stops the analysis.

The request carries separate minimization and attribution budgets. Executor failure returns an error and emits no successful receipt.

## Evidence

The receipt binds:

- the request identity;
- replay-verdict and snapshot identities;
- the initial step set and candidate set;
- every planned candidate and observed result;
- budget use and partial status;
- the minimized step set;
- rankings and probable-cause labels;
- explicit non-claims.

Receipt validation replays the pure minimization state and attribution ranking. It fails if a candidate, outcome, budget, identity, or result changes.

Use the validator for stored artifacts:

```bash
cargo run -p chaoscontrol-evidence --bin check-causality-analysis -- \
  validate request.json receipt.json
```

## Claim boundary

The analysis is valid only for the supplied replay outcomes and budget. It does not prove a unique cause, complete minimization, program correctness, or release eligibility.
