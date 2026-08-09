## ADDED Requirements

### Requirement: Distributed-protocol simulation contract [r[protocol-fault-sim.contract]]
The system MUST provide an explicit distributed-protocol simulation contract for supported adapter-based protocols without claiming support for arbitrary protocols or arbitrary protocol correctness.

#### Scenario: Run config binds deterministic sources [r[protocol-fault-sim.contract.config]]
- **GIVEN** an operator runs a supported protocol through the simulator
- **WHEN** the simulator constructs its run config
- **THEN** the config records the seed, protocol identity, adapter version, scheduler policy, virtual clock policy, RNG policy, fault schedule reference, and artifact digests

#### Scenario: Unbound nondeterminism fails closed [r[protocol-fault-sim.contract.nondeterminism-fails]]
- **GIVEN** a simulated protocol attempts to use wall-clock time, host randomness, or unregistered external I/O
- **WHEN** the simulator validation or negative fixture runs
- **THEN** it MUST reject the run as unsupported protocol-simulation evidence

### Requirement: Bounded fault injection [r[protocol-fault-sim.faults]]
The system MUST inject bounded protocol faults through explicit hooks that cover node loss, message loss, reorder, duplication, and partition.

#### Scenario: Injected faults follow the schedule [r[protocol-fault-sim.faults.schedule]]
- **GIVEN** a fault schedule declares node loss, message loss, reorder, duplication, or partition events
- **WHEN** the simulator runs that schedule
- **THEN** every injected fault MUST follow the declared schedule and seed

#### Scenario: Unregistered effect fails [r[protocol-fault-sim.faults.unregistered]]
- **GIVEN** a protocol performs an effect outside the declared fault and I/O hooks
- **WHEN** the simulator evaluates the run
- **THEN** it MUST reject the run as unsupported protocol-simulation evidence

### Requirement: Single-schedule replay proof [r[protocol-fault-sim.replay]]
The system MUST emit a reproducibility receipt and prove that a failing schedule reproduces from its seed and schedule.

#### Scenario: Identical runs reproduce [r[protocol-fault-sim.replay.reproduce]]
- **GIVEN** two protocol runs with the same seed and schedule
- **WHEN** their receipts are compared
- **THEN** the receipts record matching schedule, history, and output digests

#### Scenario: Divergent runs report bounded mismatch [r[protocol-fault-sim.replay.mismatch]]
- **GIVEN** two protocol runs with the same seed and schedule but different observed histories
- **WHEN** the receipt checker compares them
- **THEN** it MUST fail and identify the first bounded mismatch class without raw-log scraping

### Requirement: Simulation evidence boundary [r[protocol-fault-sim.evidence-boundary]]
The system MUST keep protocol-simulation evidence separate from VMM snapshot replay proof and in-process simulator evidence.

#### Scenario: Support label remains bounded [r[protocol-fault-sim.evidence-boundary.experimental]]
- **GIVEN** a readiness surface includes protocol-simulation results
- **WHEN** the promotion gate evaluates support labels
- **THEN** it MUST describe the rail as adapter-based protocol-simulation evidence
- **AND** it MUST NOT promote VM replay, arbitrary protocol correctness, or Celld-equivalent behavior from that evidence alone
