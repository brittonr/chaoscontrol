# chaoscontrol-sdk ↔ antithesis_sdk Rust Parity Mapping

This document maps the ChaosControl guest SDK (`chaoscontrol-sdk`) to the
Antithesis Rust SDK (`antithesis_sdk`). It records, for each surface, what
each SDK provides and how the two compare.

This is a comparison and design aid. Antithesis presence is not a
ChaosControl requirement. The AGENTS.md design reference
(`docs/references/antithesis-documentation.md`) stays the source of truth
for the Antithesis material, and this parity map does not create parity
claims against it.

Versions compared: `antithesis_sdk` 0.2.9 and 0.3.0 (crates.io),
`chaoscontrol-sdk` 0.1.0 (workspace). Public surface inventory reviewed on
2026-10-28 against the versioned upstream crates and
`crates/chaoscontrol-sdk/src/lib.rs`.

## Parity status codes

- **equivalent** — same surface, same semantic.
- **superset** — ChaosControl provides the Antithesis surface plus more.
- **subset** — ChaosControl provides part of the Antithesis surface.
- **divergent** — same concept, different signature or behavior.
- **absent** — only one SDK provides it; the populated column names the owner.

## Request (init and lifecycle)

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| Init | `antithesis_init()`; lazy output/random handler | `chaoscontrol_init()`; lazy initialization before assertion/lifecycle output, entropy requests and direct guidance hits | equivalent |
| VM detection | (none) | `is_in_vm()`, `is_local_output()` | absent |
| Guest shell | (none) | `runtime::guest_init()` | absent |
| Setup complete | `lifecycle::setup_complete(&Value)` | `lifecycle::setup_complete(&Value)` with `full` | equivalent |
| Named event | `lifecycle::send_event(name, &Value)` | `lifecycle::send_event(name, &Value)` with `full`; records names verbatim (0.3.0 behavior) | equivalent |
| Local output env | `ANTITHESIS_SDK_LOCAL_OUTPUT` | `CHAOSCONTROL_SDK_LOCAL_OUTPUT` (primary) or `ANTITHESIS_SDK_LOCAL_OUTPUT` (fallback) | equivalent |
| Local output constant | `LOCAL_OUTPUT` | `LOCAL_OUTPUT` and `ANTITHESIS_LOCAL_OUTPUT` at crate root in every feature set | superset |
| Convenience imports | `prelude` | `prelude` with both `cc_*` and unprefixed Antithesis names, `lifecycle`/`random` modules, plus the `compat` module | superset |

With `full`, the first lifecycle output, entropy-consuming random request or
direct guidance hit emits SDK metadata and the assertion catalog before the
observation, even when no assertion body executes. Repeated/concurrent calls
and later explicit initialization do not duplicate the catalog. Register raw
sites before any of these initialization triggers. Empty/singleton choices,
empty byte filling and no-full no-op operations do not initialize output.

Setup details follow the full JSON-value contract of
[`setup_complete` in Antithesis 0.3.0](https://docs.rs/antithesis_sdk/0.3.0/src/antithesis_sdk/lifecycle.rs.html):
objects, arrays, strings, numbers, booleans and explicit null all pass from the
ChaosControl SDK into strict local evidence reports. Only a top-level object
contributes adoption metadata; scalar text and nested array objects do not.
Both strict local evidence and SDK diagnostic readers follow the documented
first-call-wins setup transition: repeated valid completions neither replace the
first record's adoption metadata nor increment its semantic completion count.
The SDK still emits each call. Every record, including repeats, must retain
the required `details` field, complete status and exact envelope. Strict
evidence object metadata remains bounded to 64 fields with nonempty keys of
at most 128 UTF-8 bytes; each reader enforces its generic JSON/stream bounds
before ignoring repeats. Subsequent assertions/events remain observable.
This is local input-order instrumentation evidence, not a change to VM setup
behavior or a claim about cross-process global ordering.

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
| Tracking counters | `assert::TrackingInfo` | `assert::TrackingInfo` with pass/fail atomics | equivalent |
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

Assertion details accept any `serde::Serialize` value, matching the
[0.3.0 surface](https://docs.rs/antithesis_sdk/0.3.0/src/antithesis_sdk/assert/macros.rs.html#106-129);
`serde_json::Value` remains supported. Unscoped bound transport and strict
local reports preserve explicit null, booleans, numbers, strings, arrays and
objects without suppressing observations. Only top-level object fields carry
metadata overrides; non-object details use the accepted descriptor's guest
and category. Legacy records still cannot establish collision-safe evidence.
Required details, object metadata consistency, identity and JSON/resource
bounds remain enforced. Assertion messages must be
const-evaluable, as in 0.3.0. Antithesis 0.3.0 emits only a site's first
pass and first fail; ChaosControl emits every hit by default so the oracle
and explorer retain exact counts. The `sparse_emission` feature matches the
upstream sampling policy, including raw assertions, when exact per-hit
counts are not required. Sampling is keyed by accepted descriptor fingerprint,
not the compact alias: different namespaces with colliding aliases keep
independent first pass/fail observations. Stable and compatibility calls for
the same identity share the sample state; unresolved calls cannot consume
it, and catalog declarations remain unsampled. Antithesis numeric macros
attach **guidance** (maximize/minimize watermarks).
ChaosControl numeric macros merge `left`/`right` into the assertion details
on every call, keeping caller-supplied keys and replacing only `left`/`right`.
The Antithesis-named macros forward to the short-name macros, so condition
and detail behavior is identical; guidance remains omitted. Operands must
implement `serde_json::Serialize`; Antithesis requires `Into<Value>`.
See the guidance decision below.

The borrowed `Serialize + ?Sized` details contract also applies to all four
ChaosControl stable-identity assertion macros and `assert::assert_raw_with_key`.
Typed structs, unsized strings and slices serialize directly through the bounded
writer after stable identity resolution and sparse sampling; no intermediate
`Value` is required. Unknown identities and sampled-out outcomes do not invoke
the serializer. Existing `Value` callers and the no-full generic borrowed API
remain supported. Stable identity is a ChaosControl extension, not an upstream
Antithesis API.

## Random

| Function | Antithesis | ChaosControl | Status |
|---|---|---|---|
| u64 | `random::get_random()` | `random::get_random()` | equivalent |
| Choice from slice | `random_choice(&[T]) -> Option<&T>` with rejection sampling | `random_choice(&[T]) -> Option<&T>` with the same unbiased rejection sampling | equivalent |
| Choice index (extension) | (none) | `random_choice_index(n) -> usize` | absent |
| Fill bytes | `fill_bytes` (internal, native-endian chunks) | `random::fill_bytes(&mut [u8])` (public, same chunking and native endianness) | superset |
| rand integration | `AntithesisRng` (rand_core 0.6/0.9/0.10 via `rand_v0_8/9/10` features) | `ChaosControlRng` plus a distinct, constructible `AntithesisRng` (rand_core 0.6 via `rand_core_v0_6`/`rand_v0_8`; rand_core 0.9/0.10 via `rand_core_v0_9`/`rand_v0_9` and `rand_core_v0_10`/`rand_v0_10`) | equivalent |

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
| SDK metadata record | `antithesis_sdk` language/sdk/protocol versions at init | same record shape in local output; a named `antithesis_sdk` event in VM traces; `protocol_version` is the semver fallback-schema version | equivalent |
| Catalog declarations | `antithesis_assert` records with `hit = false` at init | same records after catalog completion, with identity extensions | superset |
| Must-hit verdicts | unexercised must-hit properties fail | local evidence and the VMM oracle apply `must_hit` to unexercised verdicts | equivalent |
| Display type and must-hit | `Always`/`AlwaysOrUnreachable`/`Sometimes`/`Reachable`/`Unreachable` with Antithesis `must_hit` values | same display types and `must_hit` mapping | equivalent |
| Assertion location | `location` with class, function, file, line, column | same object, resolved through the accepted descriptor rather than an ambiguous compact alias | equivalent |
| Raw assertion ID | caller-supplied string | original stable key in the `raw` namespace for declarations and hits; strict local reports use the same ID | equivalent |
| Guidance records | `antithesis_guidance` watermarks, JSON guidance, and initialization catalog | same record shape, typed `GuidanceType`, per-site watermarks, `hit = false` catalog records, and `cc_guidance_json!` for stable sites | equivalent |

Local assertion hits select source metadata through their accepted fingerprint;
declarations use the descriptor being declared. Distinct stable namespaces
with colliding aliases and identical kind/message retain their own locations.
This does not make unqualified ambiguous aliases resolvable or admit unknown
sites. Raw source metadata, display type and must-hit overrides are preserved.

Raw assertion IDs, including quoted and Unicode text, round-trip unchanged
through local JSONL and strict evidence reports. This follows the upstream
[`assert_raw` 0.3.0 example](https://docs.rs/antithesis_sdk/0.3.0/antithesis_sdk/assert/fn.assert_raw.html),
additionally checked on 2026-10-28. Stable keys in the `raw` namespace have
this report-ID meaning for both compiled and runtime registration. Compiled
sites outside that namespace retain compact hexadecimal alias IDs (or full
fingerprints without an alias). VM compatibility aliases, fingerprints and
catalog tokens are unchanged. The reader validates `hit = false` declarations
against the accepted catalog without counting them as observations, including
reachable declarations whose condition is false.

This is a clean raw-ID representation cutover: regenerate earlier raw-namespace
JSONL and derived reports that used hashed aliases; the strict reader does not
accept those aliases as alternate raw IDs. Ordinary compiled streams are
unchanged. Local records and reports remain instrumentation evidence only.

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

JSON guidance is available through `guidance::emit_json` for compatibility
aliases and `guidance::emit_json_with_key` / `cc_guidance_json!` for stable
sites. The stable APIs resolve the accepted namespace, logical key, kind and
message directly: colliding compact aliases neither drop valid observations
nor bind an undeclared site to another assertion. Unknown or mismatched
stable sites emit no guidance; the JSON record shape is unchanged.

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
  `compat::assert` mirrors the upstream `assert::{AssertType, assert_raw,
  guidance}` paths.
- Assertion and guidance catalog macros resolve `linkme` through the SDK, as in
  Antithesis 0.3.0. Downstream crates need no direct `linkme` dependency; renamed
  SDK dependencies preserve registration, assertion identity and guidance output.
- Local evidence applies `must_hit` from declarations; the SDK sends one
  `chaoscontrol_must_hit` metadata event per descriptor in VM mode, and the
  VMM oracle applies the same unexercised verdicts. Exploration, dashboard,
  and campaign assertion details retain this optional boolean. Rust and
  Nickel `assertion-summary.v2` validators use it when recomputing zero-hit
  verdicts: required properties fail, optional properties pass, and absent
  metadata keeps the historical `unexercised` result. `unreachable` retains
  its existing no-hit pass / observed-hit failure rule. Observed violations
  cannot be overridden by `must_hit`, and conflicting metadata across seeds
  is rejected rather than silently merged. Explicit null/nonboolean metadata
  is invalid. Historical documents remain readable; older closed-record
  consumers need an upgrade to read documents containing the new field.
- `assert_raw` registers runtime descriptors before catalog emission. A new
  raw site after initialization is rejected; hits for declared sites still
  emit. Caller-supplied `display_type` and `must_hit` are preserved in local
  records; the VMM AOU flag is set when an `Always` raw site has
  `must_hit = false`. With `full` disabled, a no-op `assert_raw` with the
  same call shape keeps feature-independent code compiling.
- `cc_assert_always_some!` accepts both the Antithesis brace-bundle shape
  (`{name: cond, ...}`, meaning "at least one") and the ChaosControl
  single-expression `Option::is_some` shape. `cc_assert_always_any!` is the
  unambiguous spelling for the bundle, and `cc_assert_sometimes_all!` is the
  conjunction bundle. All bundle forms evaluate caller-supplied details once
  before binding the named conditions, matching Antithesis's full-mode order.
  Condition names cannot shadow variables used by the details expression.
  Every condition is then evaluated once (no short-circuiting); its boolean
  replaces any same-named detail key while other keys and the borrowed caller
  value remain unchanged. The captured JSON value is merged without another
  deep clone. No-full mode keeps this evaluation order without JSON work.
  An empty bundle is admitted: any is `false` and all is `true`.
- Numeric macros merge `left`/`right` into details and, with `full`, require
  `guidance::GuidanceDiff` (primitive numeric types) to emit watermarks.
- Process-scoped assertions (`CHAOSCONTROL_PROCESS_ID`) still require object
  details and a valid process token, because process ownership is carried in
  object metadata. Non-object scoped details remain rejected; this change
  does not invent a wrapper or erase process ownership to admit them.
- `random_choice` now has the Antithesis slice signature. Use the
  `random_choice_index(n)` extension when an index bound is more natural.
- ChaosControl's no-op lifecycle and assertion functions, including
  `assert_always_or_unreachable!`, accept generic borrowed details, including
  unsized strings/slices and non-serializable types. Disabling `full` preserves
  the detail-bearing call shape and exactly-once condition-before-details
  evaluation without JSON work or output. Enabled assertions retain their
  serialization requirements and optional-property semantics.
- ChaosControl supports `no_std` when `full` is disabled; Antithesis is std.
- ChaosControl exposes both `rand_core_v0_6/9/10` and `rand_v0_8/9/10`
  feature names like Antithesis. The default enables `rand_v0_8`, and all
  three traits can be enabled at once.
