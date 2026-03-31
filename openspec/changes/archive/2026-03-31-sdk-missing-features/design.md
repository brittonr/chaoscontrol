## Context

The ChaosControl SDK (`chaoscontrol-sdk` crate, ~2,600 lines) provides the guest-side API for assertions, lifecycle events, guided randomness, and coverage. It communicates with the VMM through a shared-memory hypercall page at `0xFE000` using either `vmcall` or port I/O.

The protocol crate already defines `CMD_GUIDANCE = 0x07` but nothing uses it. The fault engine's `handle_hypercall` match has a catch-all `_cmd => STATUS_ERROR` arm that silently rejects guidance. The explorer (`chaoscontrol-explore`) does pure coverage-guided mutation today — no numeric gradient signal.

The assertion macros are well-structured: each `cc_assert_*!` macro computes a `location_id`, registers a `CatalogEntry` via `linkme`, and dispatches through `*_with_id` functions. The comparison variants (`cc_assert_always_lt!` etc.) follow the same pattern but lack a `$details` parameter.

## Goals / Non-Goals

**Goals:**
- Wire `CMD_GUIDANCE` end-to-end: SDK function → hypercall → fault engine storage, so the explorer can read guidance values in a future change.
- Eliminate boilerplate in guest assertion code by adding macros for common patterns (implies, Result, auto-capture comparison values).
- Keep all additions backwards-compatible — existing guests compile unchanged.

**Non-Goals:**
- Explorer-side consumption of guidance data. The explorer will use guidance in a future change; this change only stores it.
- Weighted random choice and virtual time APIs. These require deeper design work and VMM scheduler changes.
- Multi-VM coordination barriers (`all_setup_complete`). This is a VMM/orchestration concern, not an SDK concern.
- Panic classification (`cc_expect_panic!`). Requires VMM-side panic detection changes beyond this scope.

## Decisions

### 1. Guidance payload: f64 value + u32 assertion ID

The guidance API sends a floating-point distance-to-violation hint tied to a specific assertion site. The guest calls `guidance(assertion_id, distance)` where smaller distance means closer to violation.

The payload reuses the existing hypercall page layout: `id` field carries the assertion ID, `result` field (8 bytes) carries the `f64` distance as `to_le_bytes()`. No payload string needed — the assertion ID links back to the catalog entry which has the message.

**Alternative considered**: A richer payload with message + JSON details. Rejected because guidance is called frequently (potentially every loop iteration) and needs to be cheap. The assertion catalog already has the context.

### 2. Fault engine stores guidance as `HashMap<u32, f64>`

The fault engine keeps a `guidance_values: HashMap<u32, f64>` that maps assertion IDs to their last-reported distance. Overwritten on each call (most recent wins). The explorer reads this map after each execution quantum.

**Alternative considered**: A ring buffer of all guidance values. Rejected because the explorer only cares about the final state per-assertion — intermediate values during a single quantum are noise.

### 3. Comparison macros get a second form with auto-captured details

Each comparison macro (e.g., `cc_assert_always_lt!`) gains an optional `$details` form and a default form that auto-captures `left` and `right` values:

```rust
// Existing form (no details) — now auto-captures values
cc_assert_always_lt!(a, b, "a < b");
// Expanded form with explicit details
cc_assert_always_lt!(a, b, "a < b", &json!({"context": "extra"}));
```

The auto-capture evaluates `$left` and `$right` once (via `let` bindings in the macro), performs the comparison, and on failure emits `{"left": <val>, "right": <val>}`. On success, no details are serialized (the empty JSON path is taken).

**Alternative considered**: Always serializing details even on success. Rejected because assertion success is the hot path and serialization is expensive.

### 4. `cc_assert_implies!` desugars to `always` with `!p || q`

```rust
cc_assert_implies!(precondition, conclusion, "msg");
// Expands to:
cc_assert_always!(!precondition || conclusion, "msg", &details);
```

The macro auto-captures `precondition` and `conclusion` boolean values in the details on failure. This is a thin wrapper — no new assertion kind or protocol command needed.

### 5. Result macros use `is_ok()` / `is_err()` and capture Debug representation

```rust
cc_assert_always_ok!(result, "operation succeeds");
// Expands to:
// let r = result;
// let cond = r.is_ok();
// details = if !cond { json!({"error": format!("{:?}", r)}) } else { json!({}) };
// cc_assert_always!(cond, "msg", &details);
```

The `Debug` representation is captured only on failure to avoid allocating on the happy path. The macros require `Result` values where `E: Debug`.

### 6. New `guidance.rs` module in the SDK

Guidance gets its own module rather than living in `assert.rs`, because it's conceptually different — it's optimizer hints, not property assertions. The prelude re-exports `guidance` and `guidance_with_id`.

## Risks / Trade-offs

- **Guidance frequency overhead**: If guests call `guidance()` every iteration, the hypercall cost could dominate. → Mitigated by the minimal payload (no JSON serialization). Users can also batch by calling less frequently.
- **Comparison macro auto-capture requires `Debug`**: The `left`/`right` values need `Debug` for serialization. → This is the same constraint as `assert_eq!` in std. If a type doesn't impl `Debug`, users can still use the explicit `$details` form.
- **`f64` guidance values are imprecise**: NaN, infinity, negative values are all valid `f64`. → The explorer will treat NaN as "no guidance" and clamp negative values to 0. Document this in the SDK.
- **Macro hygiene**: The comparison and Result macros bind temporaries via `let`. → Use `__cc_` prefixed names and nested blocks to avoid shadowing user variables.
