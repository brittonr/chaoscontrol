## Context

The explorer's coverage signal comes from three sources today:

1. **Code edges** (guest-side, AFL bitmap): saturates in round 1 for protocol code.
2. **Assertion-state enrichment** (explorer-side): hashes verdict + hit-count bucket. Extended useful rounds from 1→3 but plateaus because the same verdicts repeat.
3. **Schedule fingerprint** (explorer-side): distinguishes vCPU interleavings. Only useful for SMP.

The Raft guest manually calls `coverage::record_edge(9000 + alive_mask * 10 + partition_count)` to inject protocol state into the code-edge bitmap. This works but requires each guest to hand-code magic number schemes. The SDK's `send_event` and assertion `details` JSON carry the same state in structured form — the explorer just ignores it.

The OracleReport already collects events and assertion records with JSON details. These flow from guest → VMM → explorer on every branch. Zero new transport needed.

## Goals / Non-Goals

**Goals:**
- Protocol state differences between branches produce different coverage, keeping the frontier alive past code-edge saturation.
- Guest developers use `record_state` instead of hand-coding magic edge IDs.
- Explorer enrichment works with any guest that uses `send_event` or assertion details — no per-protocol configuration.

**Non-Goals:**
- Full state-space tracking or model checking. This is coverage guidance, not exhaustive enumeration.
- Removing `record_edge` from the SDK. It's still useful for custom instrumentation.
- Automatic property inference from state sequences.

## Decisions

### 1. SDK `record_state` hashes key-value pairs into existing bitmap

**Decision**: Add `coverage::record_state(pairs: &[(&str, &str)])` that hashes each pair into 1-2 bitmap slots using FNV-1a (same as assertion IDs). Writes go to the same 64KB guest-side bitmap as `record_edge`.

**Why not a separate bitmap?** The VMM already reads one 64KB bitmap per branch. Adding a second doubles the read cost and requires protocol changes. Sharing the bitmap means protocol-state edges compete with code edges for space, but the Raft guest produces ~15K code edges in a 32K-slot half — plenty of room.

**Why key-value pairs, not arbitrary bytes?** Keys provide structure — `("term", "3")` hashes differently from `("commit_index", "3")` without the guest needing to manage offset namespaces. The hash naturally domain-separates by key name.

**Hash scheme**: For each pair `(key, value)`:
```
slot1 = fnv1a(key ++ "=" ++ value) % CODE_REGION_END
bitmap[slot1] = saturating_add(1)

slot2 = fnv1a(value ++ ":" ++ key) % CODE_REGION_END   // reversed for diversity
bitmap[slot2] = saturating_add(1)
```

Two slots per pair reduces collision probability. Using `CODE_REGION_END` (32K) keeps state edges in the code region alongside `record_edge` — they're both guest-side signals.

### 2. Explorer enriches coverage from OracleReport events

**Decision**: After each branch, `enrich_with_protocol_events` hashes each `OracleEvent`'s name + selected detail keys into the assertion region `[CODE_REGION_END, ASSERTION_REGION_END)`. This is explorer-side enrichment, same pattern as assertion-state and schedule fingerprint.

**What gets hashed**: Event name, plus each top-level key-value pair in the details JSON. For `OracleEvent { name: "commit", details: {"index": 42, "term": 3} }`, the hashes are:
- `hash("event:commit")`
- `hash("event:commit:index=42")`
- `hash("event:commit:term=3")`

**Why not hash the entire JSON blob?** Small differences in JSON serialization (key order, whitespace) would produce different hashes for semantically identical states. Hashing individual key-value pairs is order-independent and stable.

**Why the assertion region, not code region?** Guest-side `record_state` already fills the code region. Explorer-side enrichment from events uses the assertion region to avoid double-counting the same state from both directions.

### 3. Explorer enriches coverage from assertion detail JSON

**Decision**: Extend `enrich_with_assertion_state` to also hash the JSON details from each assertion hit, not just verdict + hit-count bucket. For `cc_assert_always!(cond, "election safety", &json!({"term": 3, "leaders": 1}))`, the enrichment hashes `("election safety:term=3")`, `("election safety:leaders=1")` into assertion-region slots.

**Why extend the existing function?** It already iterates over oracle assertions. Adding detail hashing is 5-10 lines. A separate function would duplicate the iteration.

**Detail depth limit**: Only hash top-level keys. Nested JSON objects are stringified (e.g., `"nested={\"a\":1}"`). This prevents unbounded recursion and keeps the hash count predictable.

### 4. Raft guest migration to `record_state`

**Decision**: Replace the 10+ `record_edge(MAGIC + ...)` calls in `chaoscontrol-raft-guest/src/main.rs` with `record_state` calls. Example:

Before:
```rust
coverage::record_edge(10000 + leader_count.min(3));
coverage::record_edge(10100 + term_spread.min(10) as usize);
coverage::record_edge(10200 + (max_log - min_log).min(20));
```

After:
```rust
coverage::record_state(&[
    ("leaders", &leader_count.min(3).to_string()),
    ("term_spread", &term_spread.min(10).to_string()),
    ("log_divergence", &(max_log - min_log).min(20).to_string()),
]);
```

Keep the generic `record_edge(6000 + tick * 7 + active * 3)` call that tracks simulation tick × active node — that's structural coverage, not protocol state.

## Risks / Trade-offs

**[Risk] Bitmap saturation from too many state hashes** → Each `record_state` call with N pairs writes 2N slots. A guest calling it every tick with 5 pairs writes 10 slots/tick × 1000 ticks = 10K writes. With 32K code-region slots, this fills ~30%. Mitigation: saturating counters prevent overflow; `classify()` buckets absorb high hit counts; guests should call `record_state` at meaningful state transitions, not every tick.

**[Risk] Event-based enrichment overwhelmed by high-frequency events** → A guest sending 100 events per branch with 5 detail keys each = 600 hashes into the 16K assertion region. Manageable. Mitigation: if event volume grows, cap at first N events per branch (configurable).

**[Risk] Hash collisions between state pairs and assertion edges** → Both share the assertion region. Mitigation: domain separator in the hash (prefix "event:" for events, "assert:" for assertion details) prevents systematic collisions.

**[Risk] Raft guest migration changes coverage profile** → Different hash scheme means the global coverage bitmap from round 1 will differ from pre-migration runs. Checkpoints from before migration won't resume correctly. Mitigation: this is expected — protocol-state coverage is a new signal. Old checkpoints should be discarded.
