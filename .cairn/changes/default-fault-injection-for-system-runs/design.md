# Design: default fault injection for system runs

## Context

The VMM controller builds a `FaultEngine` for every system run. It passed
`random_faults: false`, so the engine only executed the faults its schedule
named. The engine already supported seed-derived random injection with a
deterministic source, and `EngineConfig::default()` keeps the library default
off so that a library caller is never surprised.

The change belongs at the system-run configuration boundary, not in the fault
engine's own default.

## Goal and Success Contract

Completion requires these results:

- A system run injects random faults unless the operator explicitly opts out.
- The opt-out is one named configuration value.
- The injection is deterministic under a fixed seed.
- The harnesses that assert undisturbed behaviour opt out explicitly, so the
  default does not silently break them.
- The change is documented on the fault surface and carries its own lifecycle
  package.

False completion includes these results:

- Flipping the fault engine's library default and changing every caller.
- An opt-out that is implicit, such as an absent field.
- Non-deterministic injection.
- A default that leaves the repository's own assertions unreliable.

## Decisions

### Decision: Put the knob on the simulation configuration

**Choice:** `SimulationConfig` gains `random_faults: bool`, defaulting to on,
and the controller passes it to the engine.

**Rationale:** The engine's own default stays off, so a library caller that
builds an engine directly is unaffected. The decision that a *system run*
injects faults belongs to the system run, and it is visible in one place: the
configuration the operator hands to the run.

### Decision: Keep the opt-out explicit at every harness

**Choice:** The VM integration harness and the performance benchmark set
`random_faults: false` with a comment.

**Rationale:** Those harnesses assert undisturbed system behaviour, such as a
clean boot and a round-robin progress invariant. Fault injection is covered
by the fault crate's own tests and by the schedule-driven tests. Making the
opt-out explicit at each site also demonstrates that the escape hatch works
and keeps the choice visible in review.

### Decision: Do not change the engine or the exploration profile

**Choice:** The fault engine, its schedule model, and the exploration
profile's `mode` are untouched.

**Rationale:** The engine already does the work; the profile already names
fault schedule, input tree, or hybrid explicitly. Changing either would widen
the change without changing the default's meaning.

## Risks / Trade-offs

- A system run now behaves differently from a fault-free run by default. A
  downstream harness that assumed a fault-free system must opt out, and that
  opt-out is now a visible, reviewable choice rather than an accident.
- The KVM-level system runs were not exercised in this environment. The
  change is covered by unit-level tests over the configuration and the engine
  wiring; the guest-visible effect of an injected fault is the fault engine's
  existing, already tested behaviour.

## Verification Strategy

- Unit tests assert the default is on, that the configured value reaches the
  engine, and that the engine's injection stays seed-derived.
- The VMM crate builds with the new field, and every configuration site
  compiles.
- The repository's own gates and the Cairn gates run on the change.
