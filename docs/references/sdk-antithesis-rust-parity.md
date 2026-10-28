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
| Init | `antithesis_init()` | `chaoscontrol_init()`; the first assertion also registers the catalog lazily | equivalent |
| VM detection | (none) | `is_in_vm()`, `is_local_output()` | absent |
| Guest shell | (none) | `runtime::guest_init()` | absent |
| Setup complete | `lifecycle::setup_complete(&Value)` | `lifecycle::setup_complete(&Value)` with `full` | equivalent |
| Named event | `lifecycle::send_event(name, &Value)` | `lifecycle::send_event(name, &Value)` with `full`; trims and maps empty names to `anonymous` | equivalent |
| Local output env | `ANTITHESIS_SDK_LOCAL_OUTPUT` | `CHAOSCONTROL_SDK_LOCAL_OUTPUT` (primary) or `ANTITHESIS_SDK_LOCAL_OUTPUT` (fallback) | equivalent |
| Local output constant | `LOCAL_OUTPUT` | `LOCAL_OUTPUT` and `ANTITHESIS_LOCAL_OUTPUT` at crate root | superset |
| Convenience imports | `prelude` | `prelude` plus `compat` module with unprefixed Antithesis names | superset |

## Assertion macros

| Concept | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Always | `assert_always!(cond, msg[, details])` | `cc_assert_always!(cond, msg[, details])` | equivalent |
| Always-or-unreachable | `assert_always_or_unreachable!` | `cc_assert_always_or_unreachable!` | equivalent |
| Sometimes | `assert_sometimes!` | `cc_assert_sometimes!` | equivalent |
| Reachable | `assert_reachable!` | `cc_assert_reachable!` | equivalent |
| Unreachable | `assert_unreachable!` | `cc_assert_unreachable!` | equivalent |
| Raw / third-party | `assert::assert_raw(13 args)` | `assert::assert_raw(13 args)` for sites declared before catalog emission; `cc_assert_raw!` for compiled sites | equivalent |
| Assertion type enum | `assert::AssertType` | `assert::AssertType` with lowercase serialization, `to_kind`, and `From<AssertionKind>` | equivalent |
| Implication | (none) | `cc_assert_implies!` | absent |
| Result ok | (none) | `cc_assert_{always,sometimes}_ok!` | absent |
| Result err | (none) | `cc_assert_{always,sometimes}_err!` | absent |
| Bundle any | `assert_always_some!({name: cond, ...})` (OR) | `cc_assert_always_some!({name: cond, ...})` or `cc_assert_always_any!` (OR) | equivalent |
| Option some (extension) | (none) | `cc_assert_{always,sometimes}_some!(expr)` (Option) | absent |
| Multi-condition all | `assert_sometimes_all!({name: cond, ...})` (AND) | `cc_assert_sometimes_all!({name: cond, ...})` (AND) | equivalent |
| Stable identity | (none) | `cc_assert_{always,sometimes,reachable,unreachable}_stable!` | absent |
| Category/guest | (none) | `cc_assert_{always,sometimes,reachable}_category!` | absent |

### Numeric comparison macros

| Relation | Antithesis (with guidance) | ChaosControl (with left/right details) | Status |
|---|---|---|---|
| `>` | `assert_always_greater_than` | `cc_assert_always_greater_than` (alias `cc_assert_always_gt`) | equivalent |
| `>=` | `assert_always_greater_than_or_equal_to` | `cc_assert_always_greater_than_or_equal_to` (alias `cc_assert_always_ge`) | equivalent |
| `<` | `assert_always_less_than` | `cc_assert_always_less_than` (alias `cc_assert_always_lt`) | equivalent |
| `<=` | `assert_always_less_than_or_equal_to` | `cc_assert_always_less_than_or_equal_to` (alias `cc_assert_always_le`) | equivalent |
| `==` | (none) | `cc_assert_always_eq` / `sometimes_eq` | absent |
| `!=` | (none) | `cc_assert_always_ne` / `sometimes_ne` | absent |
| Sometimes variants | `assert_sometimes_<rel>` | `cc_assert_sometimes_<rel>` (long and short names) | equivalent |

Antithesis numeric macros attach **guidance** (maximize/minimize watermarks).
ChaosControl numeric macros merge `left`/`right` into the assertion details
on every call, keeping caller-supplied keys and replacing only `left`/`right`.
The Antithesis-named macros forward to the short-name macros, so condition
and detail behavior is identical; guidance remains omitted. Operands must
implement `serde_json::Serialize`; Antithesis requires `Into<Value>`.
See the guidance decision below.

## Random

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| u64 | `random::get_random()` | `random::get_random()` | equivalent |
| Choice from slice | `random_choice(&[T]) -> Option<&T>` | `random_choice(&[T]) -> Option<&T>` | equivalent |
| Choice index (extension) | (none) | `random_choice_index(n) -> usize` | absent |
| Fill bytes | `fill_bytes` (internal, native-endian chunks) | `random::fill_bytes(&mut [u8])` (public, same chunking and native endianness) | superset |
| rand integration | `AntithesisRng` (rand_core 0.6/0.9/0.10 via `rand_v0_8/9/10` features) | `ChaosControlRng` with an `AntithesisRng` alias (rand 0.8 `RngCore` + `CryptoRng` with `full`; rand_core 0.9/0.10 via `rand_v0_9`/`rand_v0_10`) | equivalent |

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
| Display type and must-hit | `Always`/`AlwaysOrUnreachable`/`Sometimes`/`Reachable`/`Unreachable` with Antithesis `must_hit` values | same display types and `must_hit` mapping | equivalent |
| Assertion location | `location` with class, function, file, line, column | same object, from the call-site catalog entry | equivalent |
| Guidance records | `antithesis_guidance` watermarks, JSON guidance, and initialization catalog | same record shape, typed `GuidanceType`, per-site watermarks, `hit = false` catalog records, and `cc_guidance_json!` for stable sites | equivalent |

## Guidance decision

Antithesis numeric and boolean assertion macros emit `antithesis_guidance`
records. A numeric guidance uses an atomic watermark guard
(`fetch_min`/`fetch_max`) so the SDK reports only the most extreme violation
per site. The `maximize` flag says whether the SDK maximizes or minimizes
the reported difference.

ChaosControl emits the same record shape for its numeric (`gt`/`ge`/`lt`/
`le`) and boolean bundle macros, with an atomic per-site watermark and the
Antithesis maximize mapping. Initialization emits one `hit = false` record
with `guidance_data = null` per linked guidance site, like Antithesis.
Records travel through the structured event path, so local output is the
exact `{"antithesis_guidance": ...}` shape while the VMM records a named
event. Each record also carries the accepted catalog token, assertion
fingerprint, and caller function path, keeping guidance catalog-bound
rather than reintroducing the removed unbound command.

JSON guidance is available through `guidance::emit_json` and the
`cc_guidance_json!` macro; both bind to an existing stable assertion site.

Guidance events feed the existing protocol-event coverage enrichment:
their name and detail values hash into assertion-region edges, so novel
guidance values make a branch interesting to the frontier. The exploration
report aggregates the same observations by message, type, and maximize
direction, showing counts and the latest value. Emitting guidance does not
change the assertion oracle or prove exploration quality.

## Notes and sharp edges

- `chaoscontrol_sdk::compat` exposes the Antithesis assertion names
  (`assert_always!`, `assert_sometimes_all!`, numeric names,
  `antithesis_init`, `lifecycle`, `random`, `AntithesisRng`, `assert_raw`).
- `assert_raw` registers runtime descriptors before catalog emission. A new
  raw site after initialization is rejected; hits for declared sites still
  emit. Caller-supplied `display_type` and `must_hit` are preserved in local
  records; the VMM AOU flag is set when an `Always` raw site has
  `must_hit = false`.
- `cc_assert_always_some!` accepts both the Antithesis brace-bundle shape
  (`{name: cond, ...}`, meaning "at least one") and the ChaosControl
  single-expression `Option::is_some` shape. `cc_assert_always_any!` is the
  unambiguous spelling for the bundle, and `cc_assert_sometimes_all!` is the
  conjunction bundle. Both bundle macros evaluate every condition (no
  short-circuiting) and merge each named boolean into the assertion details,
  matching the Antithesis bundle behavior. An empty bundle is admitted: the
  any-bundle is `false` and the all-bundle is `true`.
- Numeric macros merge `left`/`right` into details and, with `full`, require
  `guidance::GuidanceDiff` (primitive numeric types) to emit watermarks.
- `random_choice` now has the Antithesis slice signature. Use the
  `random_choice_index(n)` extension when an index bound is more natural.
- ChaosControl's no-op lifecycle functions accept a generic detail
  reference, so `setup_complete(&details)` and `send_event(name, &details)`
  keep the same call shape with `default-features = false`.
- ChaosControl supports `no_std` when `full` is disabled; Antithesis is std.
- ChaosControl enables rand 0.8 through `full` and rand_core 0.9/0.10
  through the `rand_v0_9`/`rand_v0_10` features. The feature names differ
  from Antithesis (`rand_v0_8` is implicit in `full`), but all three traits
  can be enabled at once.
