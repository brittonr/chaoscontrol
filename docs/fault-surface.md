# Deterministic resource, CPU, and clock faults

ChaosControl executes four fault classes that previously returned an unsupported result.

## Clock freeze

`ClockFreeze` records the current virtual TSC and a finite release tick. The virtual counter does not advance during this window.
At the release tick, counter advancement continues from the frozen value. The release does not add elapsed host time.

## Clock jitter

`ClockJitter` adds a deterministic guest-visible offset to the virtual TSC. The absolute offset does not exceed `bound_tsc`.
The same counter and bound produce the same value. Set `bound_tsc` to zero to remove jitter.

## CPU stall

`CpuStall` marks one vCPU as not runnable until its release tick. Other admitted vCPUs can continue.
A single-vCPU VM records zero exits while the stall is active. The scheduler makes the vCPU runnable at the exact release tick.

## Memory pressure

`MemoryPressure` contains a nonzero ceiling and a finite duration. The planner rejects a ceiling that is zero or not below the admitted baseline.
The VMM exposes the current ceiling through `chaoscontrol_sdk::resources::memory_ceiling_bytes`. It restores the baseline at the release tick.

The ceiling is an observation and policy surface. It does not change mapped guest memory or guarantee an allocation failure.

## System runs inject faults by default

A system run configures its fault engine with `random_faults` set, and the default is on.
The engine derives the injected faults from the run's master seed, so the same seed and configuration inject the same faults on every repetition.
A run that needs a quiet system sets `random_faults: false` explicitly; the opt-out is a visible choice, not an omission.
The fault engine's own library default stays off, so a caller that builds an engine directly is unaffected.

Exploration runs take the same setting from the operator.
`chaoscontrol-explore run` and `campaign` accept `--random-faults true|false`, and a run profile accepts `exploration.random_faults`; both default to on.
The explorer, its workers, and the auto-minimizer build every system run with that value.

The run records the value it used, always as an explicit boolean, in `checkpoint.json`, `campaign_progress.json`, and every bug report; replay recordings carry it too.
`resume`, `campaign-resume`, `minimize`, `reproduce`, and replay use the recorded value, so a bug is reproduced under the setting that found it.
Records written before the setting existed carry no value and load as off, because those runs built their fault engine with random injection hard-coded off.
The dogfood `run-config.json` projection does not carry the setting.

## Evidence and replay

Each accepted effect records `Selected`, `Applicable`, `Applied`, and `Observed` stages. Invalid windows and unsupported profiles record typed rejections.
Snapshots retain active deadlines, limits, and clock settings. Restore rejects state that does not match an observed applied plan.

These records prove bounded VMM behavior for the selected run. They do not prove guest-kernel OOM behavior, host timing, or production readiness.
