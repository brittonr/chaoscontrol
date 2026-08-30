# Findability survival statistics

Findability statistics estimate how often a supplied bug was found in bounded exploration subtrees.

The result is statistical evidence. It does not prove that the bug is absent.

## Observation model

Each subtree records:

- one run generation;
- one subtree identity;
- one independence-group identity;
- its observed survival time;
- zero or more bug instances with BLAKE3 identities.

The core sorts bug instances and keeps only the first instance in each subtree. It records the discarded count.

One report cannot mix run generations. The shell rejects missing, duplicate, reordered, or identity-drifted subtree records.

## Exponential fit

Let `M` be the number of subtrees with a first bug. Let `T` be the sum of survival time before the first bug or censoring.

The fitted rate is `M / T`. The mean time-to-bug is `T / M`.

If `M` is zero, the report uses `no_bug_observed`. It does not invent a finite rate or confidence projection.

A single subtree uses `insufficient_samples` and has no confidence projection.

## Conservative confidence tail

The reviewed input policy supplies a gamma prior shape and rate, a confidence target, and a projection bound.

The posterior parameters are:

- shape: `prior_shape + M`;
- rate: `prior_rate + T`.

The predictive survival tail for added exposure `x` is:

```text
(rate / (rate + x)) ^ shape
```

This is the Lomax posterior survival curve. The report states `p_survival_next_run`, the target confidence, and projected additional runs.

A projection beyond the declared run bound is reported as capped, not as an unbounded integer.

## Independence checks

A bug in every subtree is flagged as `independence_violation`. The report lists those subtree identities and emits no confidence projection.

Repeated independence-group identities are also flagged. They indicate correlated trials.

## Shell and evidence

`chaoscontrol-sim-core::findability` owns the pure model. It performs no file, clock, process, network, or environment access.

The `check-findability` shell reads one bounded regular JSON artifact without following symbolic links. The artifact includes the model policy and round observations.

BLAKE3 identities bind the artifact, each assembled observation, the observation set, the model policy, and the report.

```bash
cargo run -p chaoscontrol-evidence --bin check-findability -- \
  validate findability-rounds.json

cargo run -p chaoscontrol-evidence --bin check-findability -- \
  check findability-rounds.json findability-report.json
```

Exit status `2` means the independence assumptions failed. Exit status `3` means the sample count is insufficient.

## Assumptions and non-claims

The fitted model assumes a constant discovery rate within one generation and independent subtree groups.

It is not proof of bug absence, code correctness, deterministic replay, or release eligibility.
