# chaoscontrol-sdk ↔ antithesis_sdk Rust Parity Mapping

This document maps the ChaosControl guest SDK (`chaoscontrol-sdk`) to the
Antithesis Rust SDK (`antithesis_sdk`). It records, for each surface, what
each SDK provides and how the two compare.

This is a comparison and design aid. Antithesis presence is not a
ChaosControl requirement. The AGENTS.md design reference
(`docs/references/antithesis-documentation.md`) stays the source of truth
for the Antithesis material, and this parity map does not create parity
claims against it.

Versions compared: `antithesis_sdk` 0.2.9 (crates.io), `chaoscontrol-sdk`
0.1.0 (workspace). Public surface inventory reviewed on 2026-10-28 against
the versioned upstream crate and `crates/chaoscontrol-sdk/src/lib.rs`.

## Parity status codes

- **equivalent** — same surface, same semantic.
- **superset** — ChaosControl provides the Antithesis surface plus more.
- **subset** — ChaosControl provides part of the Antithesis surface.
- **divergent** — same concept, different signature or behavior.
- **absent** — only one SDK provides it; the populated column names the owner.

## Request (init and lifecycle)

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Init | `antithesis_init()` | `chaoscontrol_init()` | equivalent |
| VM detection | (none) | `is_in_vm()`, `is_local_output()` | absent |
| Guest shell | (none) | `runtime::guest_init()` | absent |
| Setup complete | `lifecycle::setup_complete(&Value)` | `lifecycle::setup_complete(&Value)` with `full` | equivalent |
| Named event | `lifecycle::send_event(name, &Value)` | `lifecycle::send_event(name, &Value)` with `full` | equivalent |
| Local output env | `ANTITHESIS_SDK_LOCAL_OUTPUT` | `CHAOSCONTROL_SDK_LOCAL_OUTPUT` | divergent |
| Convenience imports | `prelude` | `prelude` with ChaosControl-specific extensions | divergent |

## Assertion macros

| Concept | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Always | `assert_always!(cond, msg[, details])` | `cc_assert_always!(cond, msg[, details])` | equivalent |
| Always-or-unreachable | `assert_always_or_unreachable!` | `cc_assert_always_or_unreachable!` | equivalent |
| Sometimes | `assert_sometimes!` | `cc_assert_sometimes!` | equivalent |
| Reachable | `assert_reachable!` | `cc_assert_reachable!` | equivalent |
| Unreachable | `assert_unreachable!` | `cc_assert_unreachable!` | equivalent |
| Raw / third-party | `assert::assert_raw(13 args)` | `cc_assert_raw!(kind, cond, msg[, details])` | divergent |
| Implication | (none) | `cc_assert_implies!` | absent |
| Result ok | (none) | `cc_assert_{always,sometimes}_ok!` | absent |
| Result err | (none) | `cc_assert_{always,sometimes}_err!` | absent |
| Option some | `assert_always_some!` (boolean bundle, OR) | `cc_assert_{always,sometimes}_some!` (Option) | divergent |
| Multi-condition any | `assert_always_some!({name: cond, ...})` (OR) | `cc_assert_always_any!({name: cond, ...})` (OR) | divergent |
| Multi-condition all | `assert_sometimes_all!({name: cond, ...})` (AND) | `cc_assert_sometimes_all!({name: cond, ...})` (AND) | equivalent |
| Stable identity | (none) | `cc_assert_{always,sometimes,reachable,unreachable}_stable!` | absent |
| Category/guest | (none) | `cc_assert_{always,sometimes,reachable}_category!` | absent |

### Numeric comparison macros

| Relation | Antithesis (with guidance) | ChaosControl (with left/right details) | Status |
|---|---|---|---|
| `>` | `assert_always_greater_than` | `cc_assert_always_gt` | divergent |
| `>=` | `assert_always_greater_than_or_equal_to` | `cc_assert_always_ge` | divergent |
| `<` | `assert_always_less_than` | `cc_assert_always_lt` | divergent |
| `<=` | `assert_always_less_than_or_equal_to` | `cc_assert_always_le` | divergent |
| `==` | (none) | `cc_assert_always_eq` / `sometimes_eq` | absent |
| `!=` | (none) | `cc_assert_always_ne` / `sometimes_ne` | absent |
| Sometimes variants | `assert_sometimes_<rel>` | `cc_assert_sometimes_<rel>` | divergent |

Antithesis numeric macros attach **guidance** (maximize/minimize watermarks).
ChaosControl numeric macros capture `left`/`right` into failure details.
See the guidance decision below.

## Random

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| u64 | `random::get_random()` | `random::get_random()` | equivalent |
| Choice | `random_choice(&[T]) -> Option<&T>` | `random_choice(n) -> usize` | divergent |
| Choice from slice | (same as above) | `random_choice_from(&[T]) -> Option<&T>` | equivalent |
| Fill bytes | `fill_bytes` (internal) | `random::fill_bytes(&mut [u8])` (public) | divergent |
| rand integration | `AntithesisRng` (rand_core 0.6/0.9/0.10 via `rand_v0_8/9/10` features) | `ChaosControlRng` (rand 0.8 `RngCore` + `CryptoRng`) | subset |

## Transport and runtime modes

| Mode | Antithesis | ChaosControl | Status |
|---|---|---|---|
| In-VM / platform | `dlopen("/usr/lib/libvoidstar.so")`, call `fuzz_json_data` / `fuzz_get_random` / `fuzz_flush` | mmap hypercall page via `/dev/mem`, trigger `vmcall` (port-I/O fallback) | divergent |
| Local output | `LocalHandler` writes JSONL to env path | `LocalOutput` writer writes JSONL to env path | divergent |
| No-op | `NoOpHandler`, `default-features = false` | `Noop` transport, `default-features = false`, crate becomes `no_std` | divergent |
| Init catalog | registers `ANTITHESIS_CATALOG` + `ANTITHESIS_GUIDANCE_CATALOG` at `antithesis_init` | registers assertion catalog (begin/descriptor/complete) at `chaoscontrol_init` | divergent |

## Assertion identity

| Aspect | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Site identity | `message` string; `id = message` | versioned namespace + stable logical key + category + guest + compatibility alias + BLAKE3 fingerprint + catalog token | divergent |
| Catalog | simple distributed slice | begin → descriptors → complete admission with token, conflict detection, `MAX_ASSERTION_CATALOG_ENTRIES` | divergent |
| Aggregation | by `message` | by accepted descriptor (namespace + key + kind + message) | divergent |
| Dedup / tracking | per-site atomic pass/fail counters, emit on first pass / first fail | assertions bound to accepted catalog before emission; unbound events rejected | divergent |

## Coverage

| Aspect | Antithesis | ChaosControl | Status |
|---|---|---|---|
| User-space coverage | (none; separate instrumented build) | `coverage::{init, record_edge, record_hit, record_state, reset_state}`, SanCov hooks | absent |
| Kernel coverage | (none in SDK) | `kcov::{init, collect, total_pcs_collected, is_active, disable}` | absent |

## Workload and evidence

| Aspect | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Workload harness | (none; test templates/composer outside SDK) | `workload::{WorkloadHarness, WorkloadAdapterIdentity, WorkloadEvidenceClass, LocalDryRunReport, AssertionCoverage}` | absent |
| Details helpers | (none) | `details::{node, log, network, fault, merge, keys}` | absent |

The private `bounded_json` and `local_json_security` modules enforce local
output limits; they are not additional public SDK entry points.

## Branch markers, cooperating processes, and resources

| Aspect | Antithesis 0.2.9 | ChaosControl 0.1.0 | Status |
|---|---|---|---|
| Stable branch marker | (none) | `marker::branch_marker`, `cc_branch_marker!`; namespace/key identity with optional state/logical-position references | absent |
| Cooperating-process fault polling | (none) | `process::poll_fault() -> Result<Option<ProcessFaultCommand>, String>` | absent |
| Process supervision | (none) | `supervisor::{Supervisor, StdProcessRuntime, RuntimeError, prepare_guest_shared_storage, MountedSharedStorage}` | absent |
| Admitted memory observation | (none) | `resources::memory_ceiling_bytes() -> Option<NonZeroU64>` | absent |

Markers guide bounded exploration; emission does not prove a selected retained
branch or replay. Cooperating-process records identify their observed owner,
not a whole-guest guarantee. The supervisor prepares admitted guest storage
and process lifecycle effects; it is not a container runtime. Memory observations
come from the VMM; a disabled SDK returns `None`, not an invented ceiling.

## Local output schema

Both emit JSONL. ChaosControl emits the same `antithesis_assert`,
`antithesis_setup`, and named event records plus `chaoscontrol_*` extension
records (catalog begin/descriptor/complete, random choice) and identity
fields (`catalog_token`, `assertion_fingerprint`, `identity_version`,
`catalog_status`). The ChaosControl surface is a superset of the Antithesis
fallback schema. A strict Antithesis schema validator sees extra fields.

| Aspect | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Fallback JSONL schema | assertion/setup/named-event records | those records plus catalog/random/identity extensions | superset |
| Guidance records | `antithesis_guidance` watermarks | comparison failure details and `coverage::record_state`, not matching guidance watermarks | divergent |

## Guidance decision

Antithesis numeric and boolean assertion macros emit `antithesis_guidance`
records. A numeric guidance uses an atomic watermark guard
(`fetch_min`/`fetch_max`) so the SDK reports only the most extreme violation
per site. The `maximize` flag says whether the SDK maximizes or minimizes
the reported difference.

**Decision: guidance watermarks are not a current ChaosControl requirement.**

Rationale:

- ChaosControl already captures `left`/`right` comparison values in failure
  details and has `record_state` protocol-state hashing for the explorer.
- The Antithesis material is a comparison source, not a requirement source,
  per AGENTS.md.
- Guidance is an explorer signal, and the explorer coverage model is
  internal to ChaosControl; matching Antithesis guidance exactly is not
  needed for the current deterministic VMM.

Recorded as a MAY-level item for future explorer work. If the explorer
later needs closeness or extremal-violation signals, this decision can be
reopened with a new change.

## Notes and sharp edges

- Antithesis uses the name `assert_always_some!` for the boolean bundle
  meaning "at least one named condition". ChaosControl uses
  `cc_assert_always_some!` for `Option::is_some`, so the bundle form is
  `cc_assert_always_any!`; the conjunction bundle is
  `cc_assert_sometimes_all!`. Both ChaosControl bundle macros evaluate
  every condition (no short-circuiting) and merge each named boolean into
  the assertion details, matching the Antithesis bundle behavior. An empty
  bundle is admitted: the any-bundle is `false` and the all-bundle is
  `true`.
- `random_choice` has the same name in both SDKs but a different signature:
  Antithesis takes a slice and returns `Option<&T>`, ChaosControl takes a
  bound `n` and returns a `usize` index. Use `random_choice_from` for slice
  parity.
- ChaosControl no-op mode changes the `setup_complete` parameter type
  (`&Value` → `&()`), so a direct call compiled with
  `default-features = false` does not typecheck. Antithesis keeps the
  signature stable in no-op mode.
- ChaosControl supports `no_std` when `full` is disabled; Antithesis is std.
- ChaosControl serves rand 0.8 only; Antithesis supports rand 0.8/0.9/0.10
  and multiple versions at once.
