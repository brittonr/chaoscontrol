## Tasks

- [x] [serial] Confirm that frontier forks and assertion identity exist but no guest-declared branch hook does. r[chaoscontrol.event_branching.marker_api]
- [ ] [depends:event-branching-baseline] Add the declared marker SDK surface with stable identity, structured details, and optional canonical state or logical-position refs. r[chaoscontrol.event_branching.marker_api]
- [ ] [depends:event-branching-marker] Add marker observation to the VMM with bounded, collapse-safe admission. r[chaoscontrol.event_branching.frontier_entry]
- [ ] [depends:event-branching-vmm] Add marker-driven frontier entries with rarity and novelty scoring. r[chaoscontrol.event_branching.frontier_entry]
- [ ] [depends:event-branching-frontier] Bind markers, optional state or logical-position refs, parent snapshots, and replay verdicts with identity validation. r[chaoscontrol.event_branching.evidence_binding]
- [ ] [depends:event-branching-evidence] Record per-marker reachability and emit coverage-gap evidence. r[chaoscontrol.event_branching.marker_gap]
- [ ] [parallel] Add positive rare-event fixtures and negative never-reached, budget, and identity-drift fixtures. r[chaoscontrol.event_branching.validation]
- [ ] [depends:event-branching-validation] Run focused SDK, explorer, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.event_branching.validation]
