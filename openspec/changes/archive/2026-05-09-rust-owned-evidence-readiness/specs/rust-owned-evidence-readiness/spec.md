## ADDED Requirements

### Requirement: Rust-owned evidence/readiness tooling [r[rust-owned-evidence-readiness.tooling]]

ChaosControl MUST provide Rust-owned library and CLI tooling for structured committed-evidence validation and readiness report generation so proof policy is not encoded primarily in Python or Bash scripts.

#### Scenario: Rust core validates accepted replay proof coverage [r[rust-owned-evidence-readiness.tooling.replay-proof-coverage]]

- GIVEN `dogfood-results/accepted-workload-proofs.json` and the committed evidence directories it references
- WHEN the Rust replay proof coverage validator runs
- THEN it validates accepted summaries, bug artifacts, replay verdicts, replay parent depth, snapshot references, raw snapshot artifacts, and chunk manifests using the same or stricter fail-closed semantics as the current accepted proof gate
- AND it exits nonzero before reporting coverage when any required artifact is missing, stale, tampered, path-unsafe, or has the wrong digest

#### Scenario: Rust core renders replay proof coverage documentation [r[rust-owned-evidence-readiness.tooling.coverage-doc]]

- GIVEN the accepted workload proof manifest changes
- WHEN the Rust coverage documentation command runs in check mode
- THEN it fails unless `docs/replay-proof-coverage.md` exactly matches the manifest-derived supported workload table and bounded anti-claim text
- AND write mode regenerates only that derived coverage document without hand-authored evidence claims

#### Scenario: Rust core materializes chunked snapshots [r[rust-owned-evidence-readiness.tooling.chunk-materialization]]

- GIVEN a snapshot chunk manifest for an accepted proof
- WHEN the Rust chunk materializer validates or reconstructs the logical snapshot
- THEN it verifies schema version, confined paths, chunk count, part ordering, per-part size and SHA-256 digest, final size, final SHA-256 digest, and failure cleanup before producing a raw `.snapshot.bin`

### Requirement: Compatibility-first migration [r[rust-owned-evidence-readiness.compatibility]]

Each migrated evidence/readiness command MUST preserve the existing operator-facing success/failure semantics, bounded anti-claim language, and Nix check coverage until an intentional public interface change is specified separately.

#### Scenario: Migrated command proves positive and negative parity [r[rust-owned-evidence-readiness.compatibility.parity]]

- GIVEN a Python or Bash proof-policy command is being replaced by Rust
- WHEN the migration slice is reviewed
- THEN it includes positive evidence that the Rust command accepts current committed evidence
- AND negative evidence that malformed, stale, missing, or tampered inputs still fail closed
- AND the old command is not removed from Nix/docs until the Rust command is wired into equivalent or stronger checks

#### Scenario: Transitional aliases avoid operator breakage [r[rust-owned-evidence-readiness.compatibility.aliases]]

- GIVEN README, docs, CI, or Nix apps expose an existing command name
- WHEN the Rust implementation replaces the command internals
- THEN the public invocation remains available through a Rust binary, Nix app, or documented transitional alias until documentation is updated in the same change

### Requirement: Proof policy stays out of shell glue [r[rust-owned-evidence-readiness.no-shell-policy]]

Proof validation, report rendering, artifact hashing, chunk validation, and anti-overclaim classification MUST live in Rust-owned code rather than Bash glue after the corresponding migration slice is complete.

#### Scenario: Shell wrapper remains orchestration-only [r[rust-owned-evidence-readiness.no-shell-policy.orchestration-only]]

- GIVEN a Nix app or dogfood wrapper still needs to launch a VM workload or compose commands
- WHEN the wrapper participates in readiness or dogfood evidence flows
- THEN any retained shell glue only handles process orchestration, argument forwarding, or environment setup
- AND it delegates structured evidence decisions to Rust-owned validators or emitters
