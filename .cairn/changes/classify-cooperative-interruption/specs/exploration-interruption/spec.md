# Exploration Interruption Specification

## ADDED Requirements

### Requirement: Interruption terms identify distinct lifecycle facts

r[chaoscontrol.exploration_interruption.terms] ChaosControl MUST distinguish stop request, cooperative interruption, finalization, terminal interruption, failed interruption, and forced exit. Reports and docs MUST NOT use request receipt as terminal completion evidence.

#### Scenario: First signal is observed

- GIVEN an exploration command is running and has no prior stop request
- WHEN the shell observes SIGINT or SIGTERM
- THEN it MUST classify the event as a stop request
- AND it MUST NOT classify the command as interrupted or finalized.

#### Scenario: Documentation describes graceful shutdown

- GIVEN operator guidance describes the normal signal path
- WHEN it uses the term graceful shutdown
- THEN it MUST identify cooperative interruption as the mechanism
- AND it MUST describe forced exit as a separate path without cleanup evidence.

### Requirement: The first signal requests a cooperative stop

r[chaoscontrol.exploration_interruption.request] The first SIGINT or SIGTERM MUST record a non-terminal stop request through async-signal-safe operations. The handler MUST return without logging, allocation, persistence, report generation, or blocking.

#### Scenario: Signal arrives during a round

- GIVEN an explorer is inside one admitted round
- WHEN the first stop signal arrives
- THEN the handler MUST record the request and return
- AND the current round MAY continue to its declared boundary.

#### Scenario: Request reaches an admission boundary

- GIVEN the shell has observed a stop request at a round or seed admission boundary
- WHEN it considers new work
- THEN it MUST reject admission of another round or seed.

### Requirement: Terminal interruption requires cooperative completion

r[chaoscontrol.exploration_interruption.completion] ChaosControl MUST publish terminal `interrupted` only after the declared work boundary completes. Required checkpoint, progress, and report finalization MUST also succeed under the selected output policy.

#### Scenario: Explorer completes interruption with output

- GIVEN the first signal requested a stop and an output directory requires a checkpoint
- WHEN the current round ends and checkpoint and report writes succeed
- THEN the command MAY publish terminal `interrupted`
- AND the result MUST identify the completed boundary and finalization dispositions.

#### Scenario: Campaign stops between admitted units

- GIVEN a campaign observes a stop request during its current documented work unit
- WHEN that unit and required finalization complete
- THEN the campaign MUST skip later seeds and aggregate only completed observations.

#### Scenario: Required finalization fails

- GIVEN the cooperative path reaches its boundary but a required checkpoint, progress, or report action fails
- WHEN terminal classification runs
- THEN the result MUST be a non-successful interruption class
- AND it MUST NOT claim terminal cooperative completion.

### Requirement: A repeated signal forces exit safely

r[chaoscontrol.exploration_interruption.forced_exit] A repeated SIGINT or SIGTERM before cooperative completion MUST use an established async-signal-safe immediate-exit mechanism. It MUST use a named nonzero status and MUST publish no cleanup, checkpoint, report, or terminal-interruption claim.

#### Scenario: Repeated signal arrives before finalization

- GIVEN the command received one stop request and has not completed cooperative finalization
- WHEN another stop signal arrives
- THEN the process MUST exit through the forced path with the named nonzero status
- AND it MUST NOT emit a cooperative terminal receipt after that signal.

### Requirement: Interruption decisions preserve core and shell ownership

r[chaoscontrol.exploration_interruption.boundary] Lifecycle transition, admission, terminal-classification, report-plan, and claim-limit decisions MUST be pure over supplied facts. Shells MUST own signals, atomics, process exit, clocks, files, KVM, checkpoints, logs, and reports.

#### Scenario: Equal lifecycle facts are reduced twice

- GIVEN two equal in-memory interruption states and observations
- WHEN the reducer evaluates both inputs
- THEN both decisions MUST be equal
- AND the reducer MUST perform no signal, process, filesystem, clock, KVM, persistence, log, or report effect.

### Requirement: Harness interruption and guest faults remain separate

r[chaoscontrol.exploration_interruption.scope] Operator interruption MUST NOT create guest `ProcessKill`, `ProcessRestart`, or storage-fault evidence. Guest fault application MUST NOT create a harness interruption result.

#### Scenario: Guest process restarts

- GIVEN a selected `ProcessKill` attempt is observed and a separate `ProcessRestart` attempt later runs
- WHEN fault and recovery evidence is emitted
- THEN both fault-stage identities MUST remain separate from the harness lifecycle
- AND restart observation MUST NOT claim consumer data integrity, protocol recovery, or progress.

#### Scenario: Harness receives a stop request

- GIVEN an exploration command receives an operator stop request without a scheduled guest process fault
- WHEN interruption evidence is emitted
- THEN it MUST contain no applied or observed guest process-fault record.

### Requirement: Interruption evidence is adversarial

r[chaoscontrol.exploration_interruption.verification] The rail MUST pair completed cooperative interruption with negative forced-exit, premature-terminal, failed-finalization, stale, duplicate, post-request admission, and fault-scope cases.

#### Scenario: Interruption behavior is proposed for acceptance

- GIVEN lifecycle logic, signal shell, explorer and campaign paths, reports, docs, and fixtures are complete
- WHEN focused, workspace, Clippy, Octet, Cairn, subprocess, and relevant Nix checks run
- THEN every cooperative case MUST produce its declared terminal result
- AND every forced, failed, stale, or overclaimed case MUST fail at its declared boundary.
