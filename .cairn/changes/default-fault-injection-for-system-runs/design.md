# Design: default fault injection for system runs

## Context

The VMM controller builds a `FaultEngine` for every system run. It passed
`random_faults: false`, so the engine only executed the faults its schedule
named. The engine already supported seed-derived random injection with a
deterministic source, and `EngineConfig::default()` keeps the library default
off so that a library caller is never surprised.

The change belongs at the system-run configuration boundary, not in the fault
engine's own default.

The explorer, its workers, the minimizer, `reproduce`, and the replay crate
each build their own `SimulationConfig`. Their struct literals name every
field, so the new field left `chaoscontrol-explore` and `chaoscontrol-replay`
uncompiled until each site chose a value, and a run that is resumed,
minimized, reproduced, or replayed must use the value of the run it repeats.

## Goal and Success Contract

Completion requires these results:

- A system run injects random faults unless the operator explicitly opts out.
- The opt-out is one named configuration value.
- The injection is deterministic under a fixed seed.
- The harnesses that assert undisturbed behaviour opt out explicitly, so the
  default does not silently break them.
- The change is documented on the fault surface and carries its own lifecycle
  package.
- An exploration run takes the setting from its operator, records it, and
  every later run of the same exploration uses the recorded value.

False completion includes these results:

- Flipping the fault engine's library default and changing every caller.
- An opt-out that is implicit, such as an absent field.
- Non-deterministic injection.
- A default that leaves the repository's own assertions unreliable.
- A resumed, minimized, reproduced, or replayed run that takes the setting
  from a default instead of the record.

## Decisions

### Decision: Put the knob on the simulation configuration

**Choice:** `SimulationConfig` gains `random_faults: bool`, defaulting to on,
and the controller passes it to the engine.

**Rationale:** The engine's own default stays off, so a library caller that
builds an engine directly is unaffected. The decision that a *system run*
injects faults belongs to the system run, and it is visible in one place: the
configuration the operator hands to the run.

### Decision: Keep the opt-out explicit at every harness

**Choice:** The VM integration harness, the networking harness, and the
performance benchmark set `random_faults: false` with a comment.

**Rationale:** Those harnesses assert undisturbed system behaviour, such as a
clean boot and a round-robin progress invariant. Fault injection is covered
by the fault crate's own tests and by the schedule-driven tests. Making the
opt-out explicit at each site also demonstrates that the escape hatch works
and keeps the choice visible in review.

### Decision: Do not change the engine or the exploration mode

**Choice:** The fault engine, its schedule model, and the exploration
profile's `mode` are untouched.

**Rationale:** The engine already does the work; the profile already names
fault schedule, input tree, or hybrid explicitly. Changing either would widen
the change without changing the default's meaning.

### Decision: Give exploration the same explicit setting

**Choice:** `ExplorerConfig.random_faults` defaults to the shared
`DEFAULT_RANDOM_FAULTS`. `run` and `campaign` take `--random-faults
true|false`, which accepts only an explicit boolean, and the run profile takes
an optional `exploration.random_faults` whose absence means the default. The
explorer and its workers build every branch with the value.

**Rationale:** The explorer drives most system runs. A knob that only direct
controller callers can reach would leave the default without an operator
opt-out on the main path.

### Decision: Record the setting and repeat runs from the record

**Choice:** Checkpoints, campaign progress, bug reports, and replay recordings
always write the setting as an explicit boolean. Resumption takes it from the
checkpoint or progress file, and the minimizer, `reproduce`, and replay take
it from the bug or recording. They offer no override. The dogfood
`run-config.json` projection carries the checkpoint's recorded value and keeps
the legacy shape when the checkpoint predates the setting.

**Rationale:** A bug found under random injection reproduces only under the
same setting, and an override would invite a replay of a different run. An
explicit `false` keeps an opt-out visible in the record rather than leaving it
indistinguishable from an absent field.

### Decision: Records that predate the setting load as off

**Choice:** A checkpoint, progress file, bug report, or recording without the
field loads with random injection off.

**Rationale:** Those runs built their fault engine with random injection
hard-coded off, so off is what they executed. This is a fact about old runs,
not a default for new ones: every new record names the value.

## Risks / Trade-offs

- A system run now behaves differently from a fault-free run by default. A
  downstream harness that assumed a fault-free system must opt out, and that
  opt-out is now a visible, reviewable choice rather than an accident.
- The default changes what an exploration finds. The KVM smoke checks exercise
  the default-on path end to end; the injected faults' guest-visible effect is
  the fault engine's existing, already tested behaviour.

## Verification Strategy

- Unit tests assert the default is on, that the configured value reaches the
  engine, and that the engine's injection stays seed-derived.
- Tests assert the explorer, worker, and CLI default and explicit opt-out,
  the explicit-boolean admission of the flag, profile, and bug report, the
  recorded value in every record, the legacy load as off, and that resumption,
  minimization, and replay use the recorded value.
- The workspace builds with every configuration site naming the setting.
- The repository's own gates, including the KVM smoke checks, and the Cairn
  gates run on the change.
