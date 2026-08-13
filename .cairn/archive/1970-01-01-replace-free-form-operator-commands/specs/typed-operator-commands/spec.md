# Typed Operator Commands Specification

## Purpose

Execute evidence plans through explicit bounded process requests without shell command parsing.

## ADDED Requirements

### Requirement: Operator commands use typed plans

r[chaoscontrol.typed_operator_commands.plan] Each executable plan MUST name an executable reference, ordered arguments, admitted working directory, environment mode, explicit environment, input mode, execution limits, accepted exits, and termination scope.

#### Scenario: Complete plan is admitted
- GIVEN every required field and finite limit is valid
- WHEN plan admission runs
- THEN it MUST produce one deterministic command request.

#### Scenario: Plan omits a bound
- GIVEN a timeout, input, output, polling, or teardown bound is absent
- WHEN plan admission runs
- THEN admission MUST fail before process creation.

### Requirement: Process mechanics use an admitted mechanism

r[chaoscontrol.typed_operator_commands.mechanism] ChaosControl MUST use an exact published `bounded-exec` revision for bounded process setup, I/O, deadlines, cancellation, and owned teardown. ChaosControl MUST retain authorization and evidence policy.

#### Scenario: Mechanism revision matches
- GIVEN the dependency revision and source identity match the admitted record
- WHEN command execution starts
- THEN the receipt MUST retain that mechanism identity.

#### Scenario: Mechanism revision drifts
- GIVEN source, revision, or supported platform behavior differs
- WHEN dependency admission runs
- THEN evidence-eligible execution MUST fail.

### Requirement: Execution bypasses command interpreters

r[chaoscontrol.typed_operator_commands.execution] The process shell MUST pass the executable and argument vector directly. It MUST NOT invoke `sh -c` or reinterpret metacharacters, pipes, redirection, expansion, or compound syntax.

#### Scenario: Argument contains shell metacharacters
- GIVEN one argument contains shell syntax characters
- WHEN execution runs
- THEN the child MUST receive those characters as one literal argument.

### Requirement: Environment and paths are explicit

r[chaoscontrol.typed_operator_commands.boundary] Evidence-eligible execution MUST use an admitted executable, capability-relative working directory, and explicit environment projection. Success MUST NOT imply sandboxing, hermeticity, or executable trust.

#### Scenario: Plan requests an ambient secret
- GIVEN an environment entry is not present in the admitted projection
- WHEN plan validation runs
- THEN the request MUST fail before process creation.

### Requirement: Command decisions have a functional core

r[chaoscontrol.typed_operator_commands.functional_core] Plan shape, limit, path, identity, and outcome decisions MUST be pure deterministic logic. Artifact resolution and process execution MUST remain in shells.

#### Scenario: Identical command facts are classified twice
- GIVEN identical plan and observation facts
- WHEN the core evaluates them twice
- THEN both classifications MUST be identical.

### Requirement: Receipts bind command observations

r[chaoscontrol.typed_operator_commands.evidence] Receipts MUST bind executable and argument identities, directory, environment projection, input identity, limits, mechanism revision, exit class, timeout, truncation, cancellation, and teardown observations.

#### Scenario: Output exceeds its limit
- GIVEN a child writes beyond an admitted output bound
- WHEN execution completes
- THEN the receipt MUST record truncation and apply the selected outcome policy.

### Requirement: Legacy free-form plans cannot execute

r[chaoscontrol.typed_operator_commands.legacy] Legacy free-form command records MAY be read as diagnostic data, but they MUST NOT execute or become typed by whitespace splitting.

#### Scenario: Legacy record enters execution
- GIVEN a plan contains only command text
- WHEN execution admission runs
- THEN admission MUST reject it without invoking a shell.

### Requirement: Typed command validation is adversarial

r[chaoscontrol.typed_operator_commands.validation] Validation MUST include positive literal arguments and negative legacy, path, environment, identity, timeout, output flood, signal, cancellation, teardown, and overclaim cases.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to remove free-form execution
- WHEN focused and mechanism validation runs
- THEN every positive and negative class MUST produce its expected result.
