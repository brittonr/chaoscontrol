## MODIFIED Requirements

### Requirement: Immutable compatible source pins
r[chaoscontrol.campaign_policy.source_pins] Cargo and Nix MUST select compatible immutable published Campaign and Choregraph history revisions, from endpoints that serve them, for the supported path.

#### Scenario: Exact sources pass
r[chaoscontrol.campaign_policy.source_pins.valid]
- GIVEN Cargo, Nix, provenance, and adapter declarations
- WHEN source validation runs
- THEN every declaration selects the same full Campaign revision and the same compatible Choregraph revision

#### Scenario: Dependency is unpublished or moving
r[chaoscontrol.campaign_policy.source_pins.invalid]
- GIVEN an unpublished contract, branch, unresolved tag, sibling path, mismatched revision, or a declared endpoint that no longer serves the pinned revision
- WHEN source validation runs
- THEN adapter implementation or supported publication remains blocked

#### Scenario: Pinned sources are acquirable
r[chaoscontrol.campaign_policy.source_pins.acquirable]
- GIVEN a Cargo and Nix Git cache that does not yet hold the pinned Campaign revision
- WHEN Cargo resolves the locked Campaign packages and Nix locks the `campaign-src` input
- THEN both fetch the pinned full revision from the declared endpoint, and `cargo deny` admits that endpoint and no other Campaign source

## ADDED Requirements

### Requirement: Campaign journal state framing
r[chaoscontrol.campaign_policy.journal_framing] ChaosControl MUST mark every Campaign journal epoch with the genesis schema of the Campaign state-identity framing its control records carry, and MUST refuse to replay an epoch recorded under another framing before any of its moment or control records replays.

#### Scenario: Current framing replays
r[chaoscontrol.campaign_policy.journal_framing.valid]
- GIVEN a journal epoch whose genesis names `chaoscontrol.campaign-genesis.v2`
- WHEN recovery replays it
- THEN its moments and control records replay under Campaign's chained state identity, and recovery closes the epoch and starts the next one

#### Scenario: Earlier or unknown framing is refused
r[chaoscontrol.campaign_policy.journal_framing.invalid]
- GIVEN a journal epoch whose genesis names `chaoscontrol.campaign-genesis.v1`, the schema of Campaign's earlier whole-state framing, or any other unknown schema
- WHEN recovery replays it
- THEN recovery fails at the genesis record with `EarlierStateFraming` for the earlier framing and `UnknownSchema` otherwise, replays no other record, and writes no segment or epoch
