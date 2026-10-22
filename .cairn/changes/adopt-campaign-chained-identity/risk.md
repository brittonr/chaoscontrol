# Risk analysis for adopt-campaign-chained-identity

## Assets and hazards

| Asset | Hazard |
| --- | --- |
| Frontier decisions of the Campaign policy | The new Campaign revision changes a selection, prune, wait, or stop, so the same seed explores differently. |
| Campaign journals of existing output directories | An earlier-framing journal replays into a wrong history, or fails with an unexplained Campaign refusal. |
| Source acquisition | The new endpoint fails, serves another revision, or bypasses `cargo deny` source admission. |
| License admission | The relicensed Campaign crates enter without review, or an unrelated AGPL crate enters through a broad allowance. |
| Durability claims of the publication fence | The upgrade weakens the rule that a selection is durable before any KVM work. |
| Lifecycle evidence | Records claim more than the bounded tests, smoke, and measurement show. |

## Mitigations

- Decisions: the Campaign conformance suite still reproduces its 80 fixtures and both pinned transcript digests through the ChaosControl adapter; the frontier and adapter library tests pass; each pin's `check-frontier-parity` replays both pins' KVM run pairs with no decision drift; ranks end with the unique insertion dimension, so changed candidate identities never break a tie.
- Journals: the genesis schema moves to `v2`; an earlier-framing epoch fails at its genesis record with `EarlierStateFraming` before any record replays, and recovery writes nothing (negative test); a test pins the journal's state identities so a later framing change forces another schema bump; the README states that such directories need a fresh output directory.
- Acquisition: Cargo and Nix each pin the full revision; Nix asserts its own pin (`campaign-src.rev == campaignRevision`), and Cargo and Nix agreement rests on this change's record; `deny.toml` admits only the seed URL for Campaign, and the dependency-policy controls remove each admitted source and expect `source-not-allowed`; acquisition from an empty Cargo home and a refreshed Nix fetch are recorded.
- Licenses: `AGPL-3.0-or-later` is admitted only for `campaign-core` and `campaign-choregraph` by name, and the dependency-policy controls rename each exception and expect `failed to satisfy license requirements`.
- Durability: ChaosControl's session, segment fences, and journal publication are unchanged; only state reads moved to accessors.
- Evidence: verification separates the library tests, the KVM smoke (execution evidence for its recorded inputs only), and the append measurement (one filesystem, shared host).

## Residual risks

| Risk | Owner |
| --- | --- |
| Output directories whose journal predates this change cannot resume under the new binary. | ChaosControl maintainers |
| The public seed is the only Campaign endpoint; if it stops serving the repository, fresh acquisition fails until the pin moves. | ChaosControl maintainers |
| The A/B smoke covers one workload, seed, and host; other workloads may expose a decision change the suite and smoke miss. | ChaosControl maintainers |
| `ControlSession` stays unadopted, so ChaosControl's own session can drift from Campaign's reusable one; the measurement prices only appends, not crash recovery of split settlement segments. | ChaosControl maintainers |
| The Choregraph history dependency still comes from `git.onix.computer`, which serves it today. | ChaosControl maintainers |
| No check compares the Cargo and Nix Campaign revisions; their agreement rests on reviewed records. | ChaosControl maintainers |
| Live exploration on this host does not repeat, and a minutes-long idle-guest slowdown occurs at both pins at a rate that varies with the explorer binary for reasons not yet explained. | ChaosControl maintainers |

## Non-claims

- Passing tests and zero-drift parity receipts over one bounded run per policy and pin do not prove that every exploration decision is unchanged.
- The append measurement does not prove crash safety of a per-event journal layout.
- Fresh acquisition on this host does not prove availability from other networks.
