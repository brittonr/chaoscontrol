## ADDED Requirements

### Requirement: In-process deterministic simulator contract [r[in-process-simulator.contract]]
The system MUST provide an explicit in-process deterministic simulator contract for supported adapter-based workloads without claiming support for arbitrary unmodified guest binaries.

#### Scenario: Simulator config binds deterministic sources [r[in-process-simulator.contract.config]]
- **GIVEN** an operator runs a supported workload through the simulator
- **WHEN** the simulator constructs its run config
- **THEN** the config records the seed, workload identity, adapter version, scheduler policy, virtual clock policy, RNG policy, simulated network profile, simulated disk profile, and fault schedule reference

#### Scenario: Unbound nondeterminism fails closed [r[in-process-simulator.contract.nondeterminism-fails]]
- **GIVEN** a simulator workload attempts to use wall-clock time, host randomness, or unregistered external I/O
- **WHEN** the simulator validation or negative fixture runs
- **THEN** it MUST reject the run as unsupported simulator evidence

### Requirement: Simulator reproducibility receipt [r[in-process-simulator.receipt]]
The system MUST emit a reproducibility receipt for in-process simulator runs that binds input config, output history, fault schedule, and artifact digests.

#### Scenario: Identical simulator runs reproduce [r[in-process-simulator.receipt.reproduce]]
- **GIVEN** two simulator runs with the same config and seed
- **WHEN** their receipts are compared
- **THEN** the receipts record matching schedule, history, and output digests

#### Scenario: Divergent simulator runs report bounded mismatch [r[in-process-simulator.receipt.mismatch]]
- **GIVEN** two simulator runs with the same config and seed but different observed histories
- **WHEN** the receipt checker compares them
- **THEN** it MUST fail and identify the first bounded mismatch class without raw-log scraping

### Requirement: Simulator evidence boundary [r[in-process-simulator.evidence-boundary]]
The system MUST keep in-process simulator evidence separate from VMM snapshot replay proof, arbitrary guest support, and full FoundationDB-style simulator parity.

#### Scenario: Simulator support label remains experimental [r[in-process-simulator.evidence-boundary.experimental]]
- **GIVEN** a readiness surface includes in-process simulator results
- **WHEN** the promotion gate evaluates support labels
- **THEN** it MUST describe the rail as adapter-based simulator evidence and MUST NOT promote VM replay, arbitrary binary support, or full FoundationDB parity from that evidence alone
