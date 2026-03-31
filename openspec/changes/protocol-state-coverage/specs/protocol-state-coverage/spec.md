## ADDED Requirements

### Requirement: SDK record_state function
The SDK SHALL provide `coverage::record_state(pairs: &[(&str, &str)])` that hashes each key-value pair into 2 bitmap slots in the code region `[0, CODE_REGION_END)`. The hash function SHALL be FNV-1a. Each pair SHALL produce 2 slots using different hash orderings for collision resistance. The function SHALL be a no-op when coverage is not initialized.

#### Scenario: Hashing key-value pairs
- **WHEN** `record_state(&[("term", "3"), ("role", "leader")])` is called
- **THEN** 4 bitmap slots MUST be written (2 per pair) in `[0, CODE_REGION_END)`

#### Scenario: Different values produce different slots
- **WHEN** `record_state(&[("term", "3")])` and `record_state(&[("term", "5")])` are called in separate branches
- **THEN** they MUST write to different bitmap slots (with high probability)

#### Scenario: No-op without coverage init
- **WHEN** `record_state` is called before `coverage::init()`
- **THEN** no bitmap write SHALL occur and no crash SHALL happen

#### Scenario: Keys provide domain separation
- **WHEN** `record_state(&[("term", "3")])` and `record_state(&[("index", "3")])` are called
- **THEN** they MUST write to different bitmap slots because the key differs

### Requirement: Explorer enrichment from oracle events
The explorer SHALL hash `OracleEvent` names and detail key-value pairs from the `OracleReport` into coverage edges in the assertion region `[CODE_REGION_END, ASSERTION_REGION_END)` after each branch. Each event SHALL produce hashes for the event name and each top-level key-value pair in its details JSON.

#### Scenario: Different event sequences produce different coverage
- **WHEN** branch A produces events `[("commit", {index: 1})]` and branch B produces `[("commit", {index: 5})]`
- **THEN** their enriched coverage bitmaps MUST differ in the assertion region

#### Scenario: No events produce no enrichment
- **WHEN** a branch produces zero oracle events
- **THEN** no additional slots SHALL be written in the assertion region from event enrichment

#### Scenario: Event name is hashed
- **WHEN** a branch produces event `("leader_elected", {})` and another produces `("follower_timeout", {})`
- **THEN** their enriched coverage bitmaps MUST differ

### Requirement: Explorer enrichment from assertion details
The explorer SHALL hash the JSON details from each assertion's hits into coverage edges in the assertion region. For each assertion with non-null details, each top-level key-value pair SHALL be hashed with the assertion message as a domain separator.

#### Scenario: Different detail values produce different coverage
- **WHEN** assertion "election safety" fires with `{term: 3}` in branch A and `{term: 5}` in branch B
- **THEN** their enriched coverage bitmaps MUST differ in the assertion region

#### Scenario: Only top-level keys are hashed
- **WHEN** assertion details contain nested JSON `{outer: {inner: 1}}`
- **THEN** only the top-level key "outer" SHALL be hashed (with its stringified value)

### Requirement: Raft guest uses record_state
The Raft guest SHALL use `coverage::record_state` for protocol-state coverage instead of manual `coverage::record_edge(MAGIC + ...)` calls. Structural coverage edges (tick × active node) MAY remain as `record_edge` calls.

#### Scenario: Protocol state tracked via record_state
- **WHEN** the Raft guest computes leader count, term spread, and log divergence
- **THEN** it SHALL call `record_state` with named key-value pairs instead of magic-number edge IDs

#### Scenario: Exploration finds same bugs after migration
- **WHEN** exploration runs against the migrated Raft guest with equivalent parameters
- **THEN** the same bug variants (skip_truncate, fig8_commit, etc.) SHALL still be found
