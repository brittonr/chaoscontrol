## ADDED Requirements

### Requirement: Guest assertion catalog carries density metadata
The assertion catalog SHALL allow guest assertions to register a guest name and a density category alongside the existing id, message, kind, file, and line metadata. Supported categories SHALL be `invariant`, `branch`, `operation`, and `recovery`. Assertions emitted without the new metadata SHALL remain valid and SHALL be reported as `uncategorized`.

#### Scenario: Categorized assertion is preserved before first execution
- **WHEN** a guest assertion site is compiled with guest name `raft` and category `branch`
- **THEN** the emitted catalog entry includes `guest = "raft"` and `category = "branch"`
- **AND** the oracle preserves that metadata even if the assertion is never exercised

#### Scenario: Legacy assertion remains visible
- **WHEN** a guest assertion site is emitted without density metadata
- **THEN** the assertion is still registered in the catalog
- **AND** reports classify it as `uncategorized` rather than dropping it

### Requirement: Reports summarize assertion exercise by guest and category
Single-run and campaign reports SHALL summarize assertion exercise by guest and density category, including at least cataloged count, exercised count, and failed count for each group.

#### Scenario: Single guest report shows category summary
- **WHEN** an exploration report is generated for a guest with `operation`, `branch`, and `recovery` assertions
- **THEN** the report includes one summary row per category for that guest
- **AND** each row shows cataloged assertions and exercised assertions separately

#### Scenario: Campaign report merges groups across seeds
- **WHEN** a campaign runs multiple seeds for the same guest
- **THEN** the campaign report merges assertion counts by `(guest, category)`
- **AND** the merged summary reflects the worst verdict seen for each group
