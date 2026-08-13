## Context

Shell command text hides the intended argument boundary. It also makes exact plan identity depend on shell parsing and inherited host state.

## Decisions

### 1. Plans use typed command facts

A plan names an executable reference, ordered argument vector, capability-relative working directory, environment mode, explicit environment entries, stdin mode, execution bounds, accepted exits, and termination scope.

### 2. Nickel owns human-authored plans

Nickel contracts validate reviewable plan intent and export deterministic runtime input. Rust validates the exported DTO again before execution.

### 3. `bounded-exec` owns process mechanics

Adopt the exact published revision after license, source, platform, teardown, and output-bound checks. ChaosControl still owns executable discovery, authorization, identity, diagnostics, evidence, and policy.

### 4. The shell never invokes a command interpreter

The Rust shell passes the program and arguments directly. Shell metacharacters remain literal argument bytes. Environment inheritance is disabled unless the plan selects an admitted explicit mode.

### 5. Evidence eligibility requires identity

An evidence-eligible command binds the executable artifact, argument bytes, working directory, environment projection, input identity, limits, mechanism revision, and result class. A local diagnostic plan can use weaker identity only with a visible non-promoting class.

### 6. Legacy plans do not execute

Readers may classify old free-form plan records as legacy diagnostic data. They must not pass the text to a shell or convert it by splitting whitespace.

### 7. Validation is a pure core

The core validates plan shape, limits, path admission, identity completeness, and result classification. The shell resolves admitted artifacts and calls `bounded-exec`.

## Risks

Some existing plans rely on pipes, redirection, expansion, or compound shell syntax. Replace each with a dedicated Rust operation or an explicit trusted executable and arguments.
