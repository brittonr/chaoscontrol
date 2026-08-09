# Design: Identity-aware connectivity campaigns

## Producer contract

The campaign adapter consumes one immutable OnixOS policy-contract revision. It records the producer repository, revision, schema identities, selected fixtures, adapter identity, and BLAKE3 source manifest.

The adapter maps only stable workload identities, normalized connection tuples, expected verdict classes, policy and rule references, flow-event fields, and completeness classes. It does not import OnixOS authority, status truth, BPF lifecycle, or release policy.

The reviewed Cilium revision is `8c0423e970e62706bcd5dd3a57e1ffaee697439c`. Cilium supplies design reference material only.

## Campaign profile

Nickel owns the human-authored campaign contract. One profile binds:

- schema and campaign identity;
- producer contract and adapter identities;
- exact guest, kernel, initrd, workload, and harness identities;
- stable workload identities and VM placement;
- simulated links, relay capabilities, address families, and path classes;
- normalized ingress and egress cases;
- protocol and bounded endpoint facts;
- independent expected-decision fixture identity;
- allowed fault classes and schedules;
- observation and accounting requirements;
- named execution, packet, event, queue, snapshot, replay, artifact, and time bounds;
- evidence scope and non-claims.

Runtime traces remain Rust-owned. Nickel does not author packet events, BPF records, checkpoints, or replay traces.

## Independent oracle

The campaign must not classify success by calling the same producer evaluator that generated the expected policy decision.

A frozen expected-decision fixture names each case, normalized source and destination identities, direction, protocol facts, expected verdict, expected decisive tier and rule references, and expected unsupported conditions. Fixture generation and review are separate from runtime campaign execution.

The campaign can compare a producer explanation artifact with the independent expectation. Agreement is evidence that the selected artifacts agree for the case. It is not proof that either policy model is correct.

Negative fixtures deliberately alter tier, rule, identity, direction, protocol, or expected verdict. The campaign must detect each mismatch.

## Path matrix

The initial matrix can include:

- source and destination on one VM;
- source and destination on different VMs;
- direct simulated delivery;
- admitted relay delivery when a reviewed relay adapter exists;
- ingress and egress observation;
- expected allow;
- expected policy deny;
- unsupported transport or protocol;
- observation unavailable or partial.

Each case names the required path facts. A direct path cannot silently satisfy a relay case. A host probe cannot silently satisfy a workload path. Unsupported Layer 7 semantics remain unsupported.

## Outcome classification

The pure core consumes expected decisions and already-collected observations. It returns one typed class:

- expected allow observed;
- expected deny observed;
- policy mismatch;
- transport drop;
- routing failure;
- guest failure;
- timeout;
- observation partial;
- unsupported;
- indeterminate.

A packet absence alone is not a policy-deny observation. A deny needs a policy verdict or another admitted policy-owned signal. A delivered packet cannot pass an expected-deny case.

## Fault and heal phases

The deterministic network fabric supplies explicit selected, applicable, applied, failed, observed, healed, and indeterminate fault stages.

Campaigns can use partition, packet loss, corruption, duplication, reordering, delay, bandwidth limit, and heal when the selected fabric supports them. Every fault names exact links, direction, activation tick, duration or heal condition, and bounds.

Fault effects and policy decisions remain separate. A partitioned expected-allow path reports a transport outcome. It does not relabel the path as policy denied.

## Flow and loss evidence

Simulated observations retain producer identity, generation, source-local sequence, event kind, and delivery provenance. Optional live eBPF capture reuses the accepted `ebpf-trace-evidence` profile.

Any required sequence gap, reservation loss, queue loss, malformed event, unknown event, overflow, truncation, or missing final accounting prevents complete classification. Multi-producer timestamps do not establish a total semantic order.

## Replay and receipts

Campaign receipts bind:

- campaign, producer contract, adapter, oracle, topology, case, and schedule identities;
- guest, kernel, initrd, workload, and BPF identities;
- expected decision and observed outcome;
- fault and heal stages;
- flow accounting and completeness;
- snapshot and replay references when selected;
- terminal class, blockers, and non-claims;
- a domain-separated BLAKE3 receipt identity.

Snapshot-backed replay can establish reproduction of the named bounded campaign outcome. It does not prove policy correctness, universal replay, or physical-network behavior.

## Functional core and imperative shell

Pure cores own profile admission, adapter checking, matrix expansion, expected-decision comparison, path classification, fault applicability, observation accounting, evidence classification, and receipt preimages.

Shells own Nickel export, file reads, KVM, guest execution, simulated device effects, eBPF loading and capture, clocks, persistence, and output publication.

## Checks

Positive cases cover expected allow, expected deny, direct path, cross-VM path, fault classification, heal recovery, complete accounting, and snapshot-backed replay.

Negative cases cover stale producer revision, adapter drift, tautological oracle use, identity mismatch, wrong tier or rule, delivered deny, absent-packet deny confusion, wrong path, unsupported protocol, loss, malformed events, false total order, failed cleanup, missing KVM, and claim promotion.