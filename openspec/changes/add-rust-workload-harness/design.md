## Context

The SDK primitives are already strong enough for Rust-first use: `chaoscontrol_init`, `guest_init`, assertion macros, compile-time assertion catalog entries, lifecycle events, guided randomness, `rand::RngCore` integration, local output, and no-op builds. The practical gap is the repeated glue between a normal Rust repository and a ChaosControl run.

This change keeps the scope intentionally Rust-only and local-first. The target user is the repository owner applying ChaosControl across their own Rust projects, not a general hosted product audience.

## Goals / Non-Goals

**Goals:**

- Make adding ChaosControl to a Rust project feel like adding a test harness crate/template.
- Provide a dry-run feedback loop before booting a VM.
- Package a downstream Rust guest through Nix without copying ChaosControl internals.
- Produce concise reports that show whether SDK instrumentation is useful.
- Preserve evidence-backed promotion rules for replay claims.

**Non-Goals:**

- Multi-language SDKs.
- Docker/Kubernetes onboarding.
- Hosted dashboards or SaaS launchers.
- Automatic property synthesis.

## Decisions

### 1. Rust harness over SDK rewrite

**Choice:** Add a harness/template layer around `chaoscontrol-sdk` rather than replacing the existing SDK primitives.

**Rationale:** The current SDK maps well to Antithesis-style Rust concepts. The missing value is consistent project setup, packaging, and reporting.

**Alternative:** Rename/restructure the SDK into a new high-level API first. Rejected because it risks churn before the downstream workflow is proven.

**Implementation:** Introduce a small reusable harness crate/module and/or template that calls the existing SDK APIs, defines scenario/setup conventions, and emits metadata consumed by local and VM runs.

### 2. Local dry-run is mandatory before VM proof

**Choice:** The harness must support a local run that exercises init, assertion catalog registration, lifecycle events, and guided randomness logging without requiring a ChaosControl VM.

**Rationale:** The fastest cross-project loop is discovering missing setup signals, uncovered assertions, and weak sometimes/reachable checks before launching KVM campaigns.

**Alternative:** Only validate after VM campaign execution. Rejected because it makes SDK adoption too slow for frequent use across projects.

### 3. Nix/CLI rail owns packaging and execution glue

**Choice:** Downstream projects should use a single helper command or flake helper that builds the guest binary, composes the initrd/kernel inputs, and invokes the explorer with bounded defaults.

**Rationale:** The user's goal is repeat use across Rust projects, so the harness must hide repetitive guest packaging and command construction while preserving inspectable derivations and receipts.

**Alternative:** Document manual `nix build` and `chaoscontrol-explore` invocations only. Rejected because that leaves the main friction untouched.

### 4. Reports connect SDK quality to replay evidence

**Choice:** The harness report must include assertion catalog coverage, reached/unreached assertions, sometimes progress, lifecycle readiness, random-choice sites, replay verdict path, and evidence directory when a VM run exists.

**Rationale:** This is the Rust-only equivalent of product value: it tells the user what to instrument next and whether a run produced durable evidence.

**Alternative:** Emit only raw JSON logs. Rejected because raw logs do not guide cross-project adoption.

## Risks / Trade-offs

**Harness API churn** → Keep the first version narrow and sample-driven; avoid stabilizing a large macro DSL until at least one external project uses it.

**Overclaiming replay support** → Reports must distinguish local dry-run success, bounded VM run success, and accepted snapshot-backed replay proof.

**Nix coupling** → Keep Rust harness APIs independent of Nix while providing Nix helpers as the default packaging path.

**Assertion noise** → Density/reached reports should identify uncategorized or never-reached assertions without forcing a universal assertion taxonomy on every project.

## Validation Plan

- Add a sample downstream-style Rust workload using the harness.
- Run local dry-run and verify structured output/report contents.
- Build/package the sample guest via Nix helper or CLI rail.
- Run a bounded ChaosControl campaign and capture replay/evidence paths.
- Run existing SDK/report/evidence checks plus `git diff --check` before landing implementation.