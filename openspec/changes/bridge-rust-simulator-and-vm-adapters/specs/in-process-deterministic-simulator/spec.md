## ADDED Requirements

### Requirement: Simulator to VM receipt bridge [r[in-process-simulator.vm-receipt-bridge]]

The in-process simulator rail MUST emit bridge metadata that lets operators compare simulator runs with VM/hypervisor campaign receipts for the same Rust workload without treating simulator evidence as snapshot-backed replay proof.

#### Scenario: Bridge links comparable workload evidence [r[in-process-simulator.vm-receipt-bridge.links]]

- GIVEN a Rust workload has a simulator receipt and a VM campaign or replay receipt
- WHEN the bridge checker compares them
- THEN it reports matching or mismatched workload identity, adapter version, scenario, seed/fault schedule identity, and artifact digests
- AND it keeps each receipt's original evidence class in the summary

#### Scenario: Bridge rejects replay overclaim [r[in-process-simulator.vm-receipt-bridge.overclaim]]

- GIVEN a readiness surface cites only simulator receipts for a workload
- WHEN the promotion gate evaluates VM replay support
- THEN it rejects any claim that the simulator receipt proves snapshot-backed VM replay, arbitrary binary support, or full FoundationDB parity
