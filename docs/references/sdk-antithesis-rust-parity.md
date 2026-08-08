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
0.1.0 (workspace). Source reviewed on 2026-08-08.

## Parity status codes

- **Equivalent** — same surface, same semantic.
- **Superset** — ChaosControl provides the Antithesis surface plus more.
- **Subset** — ChaosControl provides part of the Antithesis surface.
- **Divergent** — same concept, different signature or behavior.
- **Absent** — only one SDK provides it.

## Request (init and lifecycle)

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Init | `antithesis_init()` | `chaoscontrol_init()` | Equivalent |
| VM detection | (none) | `is_in_vm()`, `is_local_output()` | ChaosControl only |
| Guest shell | (none) | `runtime::guest_init()` | ChaosControl only |
| Setup complete | `lifecycle::setup_complete(&Value)` | `lifecycle::setup_complete(&Value)` | Equivalent |
| Named event | `lifecycle::send_event(name, &Value)` | `lifecycle::send_event(name, &Value)` | Equivalent |
| Local output env | `ANTITHESIS_SDK_LOCAL_OUTPUT` | `CHAOSCONTROL_SDK_LOCAL_OUTPUT` | Equivalent (different name) |

## Assertion macros

| Concept | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Always | `assert_always!(cond, msg[, details])` | `cc_assert_always!(cond, msg[, details])` | Equivalent |
| Always-or-unreachable | `assert_always_or_unreachable!` | `cc_assert_always_or_unreachable!` | Equivalent |
| Sometimes | `assert_sometimes!` | `cc_assert_sometimes!` | Equivalent |
| Reachable | `assert_reachable!` | `cc_assert_reachable!` | Equivalent |
| Unreachable | `assert_unreachable!` | `cc_assert_unreachable!` | Equivalent |
| Raw / third-party | `assert::assert_raw(13 args)` | `cc_assert_raw!(kind, cond, msg[, details])` | Divergent |
| Implication | (none) | `cc_assert_implies!` | ChaosControl only |
| Result ok | (none) | `cc_assert_{always,sometimes}_ok!` | ChaosControl only |
| Result err | (none) | `cc_assert_{always,sometimes}_err!` | ChaosControl only |
| Option some | `assert_always_some!({name: cond, ...})` (boolean bundle) | `cc_assert_{always,sometimes}_some!` (Option) | Divergent |
| Multi-condition | `assert_sometimes_all!({name: cond, ...})` | (none) | Antithesis only |
| Stable identity | (none) | `cc_assert_{always,sometimes,reachable,unreachable}_stable!` | ChaosControl only |
| Category/guest | (none) | `cc_assert_{always,sometimes,reachable}_category!` | ChaosControl only |

### Numeric comparison macros

| Relation | Antithesis (with guidance) | ChaosControl (with left/right details) | Status |
|---|---|---|---|
| `>` | `assert_always_greater_than` | `cc_assert_always_gt` | Divergent |
| `>=` | `assert_always_greater_than_or_equal_to` | `cc_assert_always_ge` | Divergent |
| `<` | `assert_always_less_than` | `cc_assert_always_lt` | Divergent |
| `<=` | `assert_always_less_than_or_equal_to` | `cc_assert_always_le` | Divergent |
| `==` | (none) | `cc_assert_always_eq` / `sometimes_eq` | ChaosControl only |
| `!=` | (none) | `cc_assert_always_ne` / `sometimes_ne` | ChaosControl only |
| Sometimes variants | `assert_sometimes_<rel>` | `cc_assert_sometimes_<rel>` | Divergent |

Antithesis numeric macros attach **guidance** (maximize/minimize watermarks).
ChaosControl numeric macros capture `left`/`right` into failure details.
See the guidance decision below.

## Random

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| u64 | `random::get_random()` | `random::get_random()` | Equivalent |
| Choice | `random_choice(&[T]) -> Option<&T>` | `random_choice(n) -> usize` | Divergent |
| Choice from slice | (same as above) | `random_choice_from(&[T]) -> Option<&T>` | Equivalent |
| Fill bytes | `fill_bytes` (internal) | `random::fill_bytes(&mut [u8])` | Equivalent |
| rand integration | `AntithesisRng` (rand_core 0.6/0.9/0.10 via `rand_v0_8/9/10` features) | `ChaosControlRng` (rand 0.8 `RngCore` + `CryptoRng`) | Subset |

## Transport and runtime modes

| Mode | Antithesis | ChaosControl |
|---|---|---|
| In-VM / platform | `dlopen("/usr/lib/libvoidstar.so")`, call `fuzz_json_data` / `fuzz_get_random` / `fuzz_flush` | mmap hypercall page via `/dev/mem`, trigger `vmcall` (port-I/O fallback) |
| Local output | `LocalHandler` writes JSONL to env path | `LocalOutput` writer writes JSONL to env path |
| No-op | `NoOpHandler`, `default-features = false` | `Noop` transport, `default-features = false`, crate becomes `no_std` |
| Init catalog | registers `ANTITHESIS_CATALOG` + `ANTITHESIS_GUIDANCE_CATALOG` at `antithesis_init` | registers assertion catalog (begin/descriptor/complete) at `chaoscontrol_init` |

## Assertion identity

| Aspect | Antithesis | ChaosControl |
|---|---|---|
| Site identity | `message` string; `id = message` | versioned namespace + stable logical key + category + guest + compatibility alias + BLAKE3 fingerprint + catalog token |
| Catalog | simple distributed slice | begin → descriptors → complete admission with token, conflict detection, `MAX_ASSERTION_CATALOG_ENTRIES` |
| Aggregation | by `message` | by accepted descriptor (namespace + key + kind + message) |
| Dedup / tracking | per-site atomic pass/fail counters, emit on first pass / first fail | assertions bound to accepted catalog before emission; unbound events rejected |

## Coverage

| Aspect | Antithesis | ChaosControl |
|---|---|---|
| In SDK | (none; separate instrumented build) | `coverage` shared edge bitmap, SanCov hooks, `record_state`, `kcov` |

## Workload and evidence

| Aspect | Antithesis | ChaosControl |
|---|---|---|
| Workload harness | (none; test templates/composer outside SDK) | `workload::WorkloadHarness`, `WorkloadAdapterIdentity`, `LocalDryRunReport` |
| Details helpers | (none) | `details::{node, log, network, fault, merge}` |
| Bounded output | (none) | `bounded_json`, `local_json_security` caps |

## Local output schema

Both emit JSONL. ChaosControl emits the same `antithesis_assert`,
`antithesis_setup`, and named event records plus `chaoscontrol_*` extension
records (catalog begin/descriptor/complete, random choice) and identity
fields (`catalog_token`, `assertion_fingerprint`, `identity_version`,
`catalog_status`). The ChaosControl surface is a superset of the Antithesis
fallback schema. A strict Antithesis schema validator sees extra fields.

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
