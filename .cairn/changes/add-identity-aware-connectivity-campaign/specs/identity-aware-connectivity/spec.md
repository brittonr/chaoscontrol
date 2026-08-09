# Identity-Aware Connectivity Specification Delta

## ADDED Requirements

### Requirement: Connectivity campaigns use typed bounded profiles

r[chaoscontrol.identity_connectivity.profile] ChaosControl MUST define a typed Nickel profile that binds the exact producer contract and adapter, workload identities, guest artifacts, topology, path cases, protocol facts, expected-decision fixture, fault schedule, observation policy, named execution and artifact bounds, evidence scope, and non-claims. Runtime events and traces MUST remain Rust-owned.

#### Scenario: A complete campaign is admitted

- GIVEN a campaign names supported immutable producer and adapter identities, bounded workloads, topology, cases, faults, observations, artifacts, and non-claims
- WHEN profile validation and projection run
- THEN ChaosControl MUST emit one deterministic runtime input bound to the exact source profile
- AND the runtime MUST check the exported input before activation.

#### Scenario: Campaign intent is incomplete or unbounded

- GIVEN a profile omits a required identity, path fact, expected outcome, cleanup rule, finite bound, or evidence scope
- WHEN profile admission runs
- THEN admission MUST fail before guest or VMM activation.

### Requirement: The OnixOS producer adapter is immutable and narrow

r[chaoscontrol.identity_connectivity.producer_adapter] ChaosControl MUST consume the OnixOS network-policy contract through one versioned adapter bound to an immutable repository revision, schema identities, selected fixtures, and source-manifest BLAKE3. The adapter MUST preserve stable workload, decision, explanation, flow, and completeness fields without importing OnixOS authority, lifecycle, status truth, or release policy.

#### Scenario: Producer contract matches the reviewed adapter

- GIVEN the exact published OnixOS revision and schemas match the selected adapter
- WHEN adapter admission runs
- THEN the mapped values MUST preserve their source identities and declared meaning
- AND the consumer receipt MUST identify the producer and adapter separately.

#### Scenario: Producer schema drifts

- GIVEN the producer revision, schema, field set, enum, identity domain, or fixture identity differs from the reviewed adapter
- WHEN adapter admission runs
- THEN the campaign MUST fail before execution
- AND it MUST NOT guess compatibility from similar field names.

### Requirement: Expected decisions use an independent oracle

r[chaoscontrol.identity_connectivity.oracle] Each campaign case MUST bind an independently reviewed expected-decision fixture. Campaign success MUST NOT use the OnixOS production policy evaluator as its only expected-decision oracle. Agreement between producer explanation and independent expectation MUST remain bounded comparison evidence.

#### Scenario: Independent expectation matches observed policy behavior

- GIVEN a frozen expectation names the source, destination, direction, protocol, verdict, decisive tier, and rule references
- WHEN the campaign collects a matching producer explanation and runtime observation
- THEN the case MAY satisfy its bounded expected-decision comparison
- AND the receipt MUST bind both expectation and producer artifacts.

#### Scenario: The producer evaluator supplies its own expected result

- GIVEN a campaign derives the expected verdict only by calling the same evaluator under observation
- WHEN oracle admission runs
- THEN the case MUST be rejected as tautological
- AND it MUST NOT satisfy policy-path evidence.

### Requirement: The campaign matrix preserves exact path semantics

r[chaoscontrol.identity_connectivity.matrix] ChaosControl MUST model each supported same-VM, cross-VM, direct, relay, ingress, egress, expected-allow, and expected-deny case as an explicit path. One path class MUST NOT silently satisfy another. Unsupported transports, address families, relay semantics, or Layer 7 protocols MUST remain unsupported.

#### Scenario: Cross-VM direct allow path succeeds

- GIVEN two admitted workload identities on different VMs, a supported direct link, and an expected allow decision
- WHEN the bounded trigger crosses the selected link and reaches the exact destination
- THEN the case MAY report expected allow observed
- AND the receipt MUST bind the exact source, destination, link, direction, and protocol.

#### Scenario: Direct delivery is used for a relay case

- GIVEN a case requires one reviewed relay path
- WHEN traffic reaches the destination through a direct path
- THEN the relay case MUST fail or remain unsupported
- AND direct delivery MUST NOT satisfy relay evidence.

### Requirement: Outcome classification separates policy and transport failures

r[chaoscontrol.identity_connectivity.outcomes] ChaosControl MUST classify expected allow observed, expected deny observed, policy mismatch, transport drop, routing failure, guest failure, timeout, partial observation, unsupported, and indeterminate outcomes separately. Packet absence alone MUST NOT establish policy denial. A delivered packet MUST NOT pass an expected-deny case.

#### Scenario: Policy emits the expected deny verdict

- GIVEN the independent oracle expects deny and an admitted policy-owned observation records the matching deny reason and references
- WHEN pure outcome classification runs
- THEN the case MAY report expected deny observed
- AND delivery absence can remain supporting transport observation only.

#### Scenario: A packet disappears during an expected deny case

- GIVEN the independent oracle expects deny but no admitted policy verdict is observed
- AND the network fabric reports loss or provides incomplete observation
- WHEN outcome classification runs
- THEN the case MUST report transport drop, partial observation, or indeterminate
- AND it MUST NOT report expected policy deny observed.

### Requirement: Fault and heal evidence remains path-specific

r[chaoscontrol.identity_connectivity.faults] Identity-aware campaigns MUST preserve selected, applicable, applied, failed, observed, healed, and indeterminate fault stages for exact links and directions. Policy verdicts MUST remain separate from partition, loss, corruption, duplication, reordering, delay, bandwidth, and heal outcomes.

#### Scenario: A partition blocks an allowed path

- GIVEN policy allows one path and an admitted partition applies to its exact link
- WHEN delivery fails during the partition and succeeds after a recorded heal
- THEN the campaign MUST classify the faulted result as a transport outcome
- AND it MAY record bounded heal recovery without changing the policy verdict.

#### Scenario: A selected fault does not affect the path

- GIVEN a fault targets another link, direction, tick range, or unsupported capability
- WHEN the campaign evaluates the selected path
- THEN it MUST NOT record the fault as observed for that path.

### Requirement: Observation accounting detects loss and ordering limits

r[chaoscontrol.identity_connectivity.observation] Each evidence-eligible campaign MUST bind selected observation producers, generations, source-local sequences, event classes, bounds, terminal accounting, and delivery provenance. Any required sequence gap, reservation loss, queue loss, malformed or unknown event, overflow, truncation, parse failure, or missing final accounting MUST prevent complete classification. Multi-producer timestamps or callback order MUST NOT create a semantic total order.

#### Scenario: Complete observation reconciles

- GIVEN every required producer has continuous required source sequences, checked counters, zero loss, and final accounting
- WHEN pure accounting runs
- THEN the selected campaign window MAY be complete for its exact scope.

#### Scenario: One event source loses records

- GIVEN a required producer reports loss or a sequence gap
- WHEN accounting and outcome classification run
- THEN the observation MUST be partial, failed, or unsupported
- AND it MUST NOT satisfy complete policy-path evidence.

### Requirement: Campaign evidence is canonical and narrow

r[chaoscontrol.identity_connectivity.evidence] ChaosControl MUST emit domain-separated BLAKE3 campaign, matrix, run, observation, replay, and receipt identities. Receipts MUST bind the producer contract, adapter, independent oracle, topology, case, schedule, guest artifacts, expected decision, observed outcome, fault and heal stages, accounting, replay references, terminal class, blockers, and non-claims.

#### Scenario: Snapshot-backed replay reproduces a path outcome

- GIVEN one exact snapshot, campaign, schedule, guest cohort, oracle, and observation set reproduce the selected outcome
- WHEN replay evidence is classified
- THEN the receipt MAY report reproduction for that bounded campaign case
- AND it MUST NOT claim universal policy correctness, network determinism, or physical behavior.

#### Scenario: Campaign evidence is promoted to physical readiness

- GIVEN a consumer presents simulated or KVM connectivity evidence as proof of physical switches, NICs, firmware, external relays, production availability, or release eligibility
- WHEN evidence-scope checking runs
- THEN the promotion MUST be rejected.

### Requirement: Connectivity campaign ownership remains explicit

r[chaoscontrol.identity_connectivity.boundary] ChaosControl MUST retain simulation, fault, replay, trace, and campaign evidence meaning. OnixOS MUST retain network-policy, workload-identity, BPF lifecycle, authority, status truth, and release meaning. Cilium and Hubble MUST remain design references only.

#### Scenario: Campaign verdict is used as authority

- GIVEN a campaign reports an expected allow path
- WHEN a protected operation lacks its required UCAN or Basalt authority
- THEN the operation MUST remain denied
- AND the campaign receipt MUST NOT grant transport, network, or operation authority.

### Requirement: Identity-aware connectivity has positive and negative verification

r[chaoscontrol.identity_connectivity.verification] The change MUST pair positive allow, deny, fault, heal, accounting, adapter, oracle, matrix, replay, and evidence checks with negative stale, tautological, mismatched, lossy, malformed, wrong-path, unsupported, cleanup, missing-prerequisite, and overclaim checks. Cheap checks MUST remain separate from KVM behavior evidence.

#### Scenario: Closeout checks run

- GIVEN maintainers intend to sync and archive the change
- WHEN pure, Nickel, adapter, network, fault, trace, replay, Cairn, KVM, and relevant Nix checks run
- THEN the required positive and negative classes MUST produce their expected results
- AND wiring-only or missing-KVM results MUST NOT satisfy behavior evidence.