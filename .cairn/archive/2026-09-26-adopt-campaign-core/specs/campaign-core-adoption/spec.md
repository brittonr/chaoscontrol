## ADDED Requirements

### Requirement: Adaptive exploration baseline
r[chaoscontrol.campaign_policy.baseline] ChaosControl MUST record the exact legacy frontier, input-tree, and exploration-loop revision plus focused positive and negative results before policy migration.

#### Scenario: Complete baseline passes
r[chaoscontrol.campaign_policy.baseline.valid]
- GIVEN the reviewed `frontier.rs`, `input_tree.rs`, and `explorer.rs` implementation
- WHEN focused baseline checks run
- THEN results bind the exact source revision, commands, fixtures, and bounded outputs

#### Scenario: Missing baseline blocks migration
r[chaoscontrol.campaign_policy.baseline.missing]
- GIVEN no current focused adaptive-policy baseline
- WHEN shared-path implementation starts
- THEN supported migration is blocked

### Requirement: Immutable compatible source pins
r[chaoscontrol.campaign_policy.source_pins] Cargo and Nix MUST select compatible immutable published Campaign and Choregraph history revisions for the supported path.

#### Scenario: Exact sources pass
r[chaoscontrol.campaign_policy.source_pins.valid]
- GIVEN Cargo, Nix, provenance, and adapter declarations
- WHEN source validation runs
- THEN every declaration selects the same full Campaign revision and the same compatible Choregraph revision

#### Scenario: Dependency is unpublished or moving
r[chaoscontrol.campaign_policy.source_pins.invalid]
- GIVEN an unpublished contract, branch, unresolved tag, sibling path, or mismatched revision
- WHEN source validation runs
- THEN adapter implementation or supported publication remains blocked

### Requirement: Policy-only adoption boundary
r[chaoscontrol.campaign_policy.boundary] ChaosControl MUST use Campaign only for frontier reconstruction, ranking, selection, budget, pruning, and structural stop decisions.

#### Scenario: Narrow boundary passes
r[chaoscontrol.campaign_policy.boundary.valid]
- GIVEN the ChaosControl adapter and shared Campaign crates
- WHEN dependency validation runs
- THEN shared crates contain no ChaosControl action, VM, snapshot, schedule, coverage, assertion, finding, replay, minimization, storage, or evidence types

#### Scenario: Multi-seed lifecycle enters Campaign
r[chaoscontrol.campaign_policy.boundary.seed_runner]
- GIVEN adoption moves seed dispatch, progress, resume, aggregation, or report projection from `campaign.rs` into Campaign
- WHEN scope validation runs
- THEN supported migration fails

#### Scenario: Product type leaks into Campaign
r[chaoscontrol.campaign_policy.boundary.invalid]
- GIVEN a shared API requires one ChaosControl product or infrastructure type
- WHEN dependency validation runs
- THEN supported migration fails

### Requirement: Choregraph exploration history
r[chaoscontrol.campaign_policy.history] ChaosControl MUST bind retained exploration moments and Campaign control events to exact immutable Choregraph history identities.

#### Scenario: Exact moment mapping passes
r[chaoscontrol.campaign_policy.history.valid]
- GIVEN one retained product moment and its exact parent and adapter identities
- WHEN history mapping runs
- THEN one immutable Choregraph event envelope is produced without snapshot bytes

#### Scenario: History mapping is stale or crossed
r[chaoscontrol.campaign_policy.history.invalid]
- GIVEN a moment has a missing parent, stale graph, wrong campaign, unsupported schema, or conflicting event identity
- WHEN mapping runs
- THEN mapping fails without product-state mutation

### Requirement: Exact expansion candidate adapter
r[chaoscontrol.campaign_policy.adapter] The adapter MUST bind one reusable frontier candidate to an exact parent, mode, operation-generation profile, branch budget, cost profile, and product adapter identity.

#### Scenario: Exact candidate passes
r[chaoscontrol.campaign_policy.adapter.valid]
- GIVEN one current frontier entry and admitted expansion profile
- WHEN the adapter constructs a Campaign candidate
- THEN it binds all required stable identities and no raw product DTO
- AND later selections can reuse the candidate with distinct selection identities

#### Scenario: Crossed candidate fails
r[chaoscontrol.campaign_policy.adapter.invalid]
- GIVEN a candidate uses another moment, mode, profile, campaign, guidance, or adapter identity
- WHEN mapping runs
- THEN mapping fails without frontier mutation

### Requirement: Checked integer rank conversion
r[chaoscontrol.campaign_policy.ranks] ChaosControl MUST convert product scores to bounded integer guidance records under one versioned adapter profile.

#### Scenario: Selected ordering is preserved
r[chaoscontrol.campaign_policy.ranks.valid]
- GIVEN legacy product scores and selection counts in the bounded migration corpus
- WHEN guidance conversion, Campaign decay, and ordering run
- THEN candidate order and tie classes agree with the selected legacy behavior

#### Scenario: Rank conversion is unsafe
r[chaoscontrol.campaign_policy.ranks.invalid]
- GIVEN a non-finite score, overflow, unsupported profile, unstable conversion, or unbound conversion identity
- WHEN rank conversion runs
- THEN the candidate is denied before selection

### Requirement: Explicit epsilon entropy
r[chaoscontrol.campaign_policy.entropy] ChaosControl MUST supply bounded seeded entropy and an exact source identity for Campaign exploratory selection.

#### Scenario: Equal entropy selects equally
r[chaoscontrol.campaign_policy.entropy.valid]
- GIVEN equal frontier, policy, entropy ticket, and entropy source
- WHEN Campaign selection runs
- THEN both runs choose the same ranked or exploratory candidate

#### Scenario: Entropy is missing or crossed
r[chaoscontrol.campaign_policy.entropy.invalid]
- GIVEN exploratory selection has no ticket or a ticket from another seed, campaign, state, or source
- WHEN selection runs
- THEN no candidate is selected

### Requirement: Restorable snapshot eligibility
r[chaoscontrol.campaign_policy.snapshot_eligibility] ChaosControl MUST mark a candidate executable only when it binds a current restorable snapshot identity or an admitted clean-bootstrap operation.

#### Scenario: Restorable moment is eligible
r[chaoscontrol.campaign_policy.snapshot_eligibility.valid]
- GIVEN a candidate with an exact current snapshot identity and retained product artifact
- WHEN product eligibility mapping runs
- THEN the candidate can enter Campaign ranking

#### Scenario: Structural moment lacks snapshot
r[chaoscontrol.campaign_policy.snapshot_eligibility.invalid]
- GIVEN Choregraph history contains a moment whose snapshot was not retained or is stale
- WHEN product eligibility mapping runs
- THEN that moment is not executable without an explicit clean-bootstrap operation

### Requirement: Frontier policy parity
r[chaoscontrol.campaign_policy.frontier_parity] The Campaign-backed path MUST preserve selected score-decay, ranked-choice, epsilon-choice, canonical-tie, and capacity-pruning behavior.

#### Scenario: Bounded policy corpus agrees
r[chaoscontrol.campaign_policy.frontier_parity.valid]
- GIVEN equal frontier entries, selection counts, ranks, entropy, and bounds
- WHEN legacy and shared policy evaluate the corpus
- THEN selected candidates, updated counts, retained candidates, and stop classes agree

#### Scenario: Policy decision drifts
r[chaoscontrol.campaign_policy.frontier_parity.invalid]
- GIVEN one selected candidate, tie class, pruning result, or stop class differs
- WHEN migration readiness is evaluated
- THEN shared cutover remains blocked

### Requirement: Durable selection publication fence
r[chaoscontrol.campaign_policy.publication_fence] ChaosControl MUST durably publish the exact Campaign selection event and fenced Choregraph branch update before expansion effects.

#### Scenario: Published selection can expand
r[chaoscontrol.campaign_policy.publication_fence.valid]
- GIVEN exact durable acceptance of the current selection event and branch plan
- WHEN the shell evaluates expansion eligibility
- THEN the selected expansion can execute

#### Scenario: Unpublished selection cannot expand
r[chaoscontrol.campaign_policy.publication_fence.invalid]
- GIVEN no exact publication evidence or a stale generation or head
- WHEN the shell evaluates expansion eligibility
- THEN mutation, snapshot restore, worker dispatch, and KVM execution do not start

### Requirement: ChaosControl effect authority
r[chaoscontrol.campaign_policy.effects] ChaosControl MUST retain schedule mutation, input-tree alternative generation, snapshot restoration, worker execution, KVM execution, clocks, signals, and cleanup.

#### Scenario: Published operation executes locally
r[chaoscontrol.campaign_policy.effects.valid]
- GIVEN a published Campaign expansion selection and current product eligibility
- WHEN ChaosControl executes it
- THEN only product-owned code creates and executes branch actions

#### Scenario: Shared crate executes an effect
r[chaoscontrol.campaign_policy.effects.invalid]
- GIVEN Campaign or Choregraph code mutates a schedule, restores a snapshot, starts a worker, or operates a VM
- WHEN architecture validation runs
- THEN supported migration fails

### Requirement: Exact product result mapping
r[chaoscontrol.campaign_policy.observations] ChaosControl MUST evaluate branch results and map only bounded child moments, guidance updates, resource use, and opaque finding references to Campaign.

#### Scenario: Expansion returns child moments
r[chaoscontrol.campaign_policy.observations.valid]
- GIVEN one outstanding selected expansion produces bounded interesting branch results
- WHEN product evaluation and adapter mapping run
- THEN every retained child moment binds the exact selection, product result, and Choregraph event identity
- AND the selection leaves the outstanding set

#### Scenario: Observation is crossed or false
r[chaoscontrol.campaign_policy.observations.invalid]
- GIVEN a result belongs to another selection, snapshot, schedule, seed, campaign, or adapter
- WHEN observation mapping runs
- THEN mapping fails without Campaign or product-state mutation

### Requirement: Product guidance remains local
r[chaoscontrol.campaign_policy.product_authority] ChaosControl MUST retain coverage, rare-edge, assertion-state, protocol-event, schedule-fingerprint, finding, and corpus-interest meaning.

#### Scenario: Valid product facts produce ranks
r[chaoscontrol.campaign_policy.product_authority.valid]
- GIVEN exact product observations under the selected adapter profile
- WHEN guidance mapping runs
- THEN ChaosControl can produce bounded rank facts for Campaign
- AND Campaign receives no product payload or semantic authority

#### Scenario: Shared policy interprets product facts
r[chaoscontrol.campaign_policy.product_authority.invalid]
- GIVEN Campaign code parses coverage bitmaps, assertion states, fault schedules, findings, or protocol events
- WHEN boundary validation runs
- THEN supported migration fails

### Requirement: Wait and stop mapping preserves authority
r[chaoscontrol.campaign_policy.stop] ChaosControl MUST map shared wait, frontier, and budget classes without transferring plateau, finding, signal, maximum-round, or operator meaning.

#### Scenario: Product stop maps structurally
r[chaoscontrol.campaign_policy.stop.valid]
- GIVEN an exact current product stop fact
- WHEN the adapter maps it
- THEN Campaign records only the selected structural stop class and bounded reason identity

#### Scenario: Parallel work remains outstanding
r[chaoscontrol.campaign_policy.stop.wait]
- GIVEN no expansion can start while one or more published expansions remain outstanding
- WHEN the next shared decision runs
- THEN the adapter preserves an explicit wait decision
- AND it does not report frontier or budget exhaustion

#### Scenario: Stop claims exhaustive search
r[chaoscontrol.campaign_policy.stop.invalid]
- GIVEN any frontier, budget, plateau, finding, signal, or maximum-round stop
- WHEN report projection runs
- THEN neither Campaign nor ChaosControl claims that all behavior was explored

### Requirement: Progress and reports remain local
r[chaoscontrol.campaign_policy.progress] ChaosControl MUST retain checkpoints, multi-seed progress, resume, campaign aggregation, and product report projection.

#### Scenario: Existing product state remains local
r[chaoscontrol.campaign_policy.progress.valid]
- GIVEN exploration or multi-seed progress and report data
- WHEN dependency validation runs
- THEN the data remains in ChaosControl storage and product types

#### Scenario: Shared checkpoint replaces product state
r[chaoscontrol.campaign_policy.progress.invalid]
- GIVEN Campaign or Choregraph becomes authoritative for VM snapshots, progress files, resume, aggregation, or reports
- WHEN architecture validation runs
- THEN supported migration fails

### Requirement: Evidence classification boundary
r[chaoscontrol.campaign_policy.evidence] Campaign and Choregraph structural receipts MUST NOT satisfy VM, fault, assertion, finding, replay, minimization, or release evidence requirements.

#### Scenario: Product evidence stays separate
r[chaoscontrol.campaign_policy.evidence.valid]
- GIVEN valid structural receipts and separate ChaosControl runtime artifacts
- WHEN evidence classification runs
- THEN only product-owned artifacts can satisfy product evidence classes

#### Scenario: Structural receipt promotion fails
r[chaoscontrol.campaign_policy.evidence.invalid]
- GIVEN only Campaign or Choregraph structural receipts
- WHEN product evidence admission runs
- THEN VM, fault, assertion, finding, replay, minimization, and release claims are denied

### Requirement: Adaptive KVM parity
r[chaoscontrol.campaign_policy.kvm_parity] Supported cutover MUST include one selected adaptive KVM exploration smoke receipt on a compatible host.

#### Scenario: KVM parity passes
r[chaoscontrol.campaign_policy.kvm_parity.valid]
- GIVEN a compatible host and equal selected exploration inputs
- WHEN legacy and shared policy paths run
- THEN bounded selection, child-moment, coverage, finding, pruning, and stop observations agree

#### Scenario: Missing KVM evidence blocks support
r[chaoscontrol.campaign_policy.kvm_parity.missing]
- GIVEN no current required KVM parity receipt
- WHEN supported cutover is evaluated
- THEN cutover remains blocked

### Requirement: Shared-policy cutover
r[chaoscontrol.campaign_policy.cutover] ChaosControl MUST select the Campaign-backed policy only after conformance, model parity, source pins, and required KVM evidence pass.

#### Scenario: Ready policy becomes supported
r[chaoscontrol.campaign_policy.cutover.valid]
- GIVEN current conformance, model-parity, source, and KVM receipts
- WHEN implementation selection runs
- THEN the Campaign-backed policy becomes supported

#### Scenario: Failed gate blocks selection
r[chaoscontrol.campaign_policy.cutover.invalid]
- GIVEN one missing, stale, or failing required receipt
- WHEN implementation selection runs
- THEN the shared policy remains unsupported

### Requirement: Bounded legacy rollback
r[chaoscontrol.campaign_policy.rollback] After supported cutover, legacy frontier policy MUST be removed or retained only as explicit diagnostic rollback code.

#### Scenario: Diagnostic rollback is bounded
r[chaoscontrol.campaign_policy.rollback.valid]
- GIVEN a reviewed need for temporary rollback
- WHEN an operator selects the legacy policy explicitly
- THEN output identifies diagnostic mode and cannot satisfy supported release gates

#### Scenario: Implicit legacy selection fails
r[chaoscontrol.campaign_policy.rollback.invalid]
- GIVEN a normal supported exploration request after cutover
- WHEN policy selection runs
- THEN the legacy path cannot be selected implicitly
