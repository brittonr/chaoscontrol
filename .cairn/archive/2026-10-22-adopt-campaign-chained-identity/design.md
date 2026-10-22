## Context

The archived `adopt-campaign-core` change (2026-09-26) moved ChaosControl's adaptive frontier onto Campaign `e7757a51` with Choregraph history `b3e08e19`. ChaosControl keeps a crate-private control session that applies Campaign control events incrementally, journals every control event, moment, and product record in `campaign-journal/epoch-NNNN/seg-NNNNNNNN.cbor` segments, and replays an epoch on resume by re-planning each recorded control event through Campaign's publication planner.

Since `e7757a51`, Campaign archived four changes that matter here: `prepare-public-source` (relicensed Campaign as `AGPL-3.0-or-later` in `6dc0c59a`), `seal-frontier-identity` (private `FrontierState` fields with read accessors, chained state identity, default bounds 32,768), `add-control-session` (a reusable `ControlSession` in `campaign-choregraph`), and `improve-consumer-api` (contextual typed errors, `Display`, `core::error::Error`). Campaign's conformance suite still pins 80 fixtures and the reference transcript digests `28b873e9…b51b` and `37b9ff95…5bae`.

Building ChaosControl against `4a669ead` fails only on 67 reads of private `FrontierState` fields in 16 files. No ChaosControl code writes a Campaign state, matches the error variants that gained fields, or matches `campaign_choregraph::AdapterError` exhaustively.

## Goals

- Acquire Campaign from an endpoint that serves it.
- Build and pass ChaosControl's Campaign tests on `4a669ead` without changing a frontier decision.
- Fail clearly when a journal recorded under the earlier framing is resumed.
- Record the durable cost that a per-event `ControlSession` adoption would add.

## Non-goals

- Adopting `ControlSession`, migrating old journals, changing Campaign, or moving the Choregraph history source.

## Decisions

### Decision: Fetch Campaign from its public Radicle seed

**Choice:** Cargo and the Nix `campaign-src` input fetch `https://seed.radicle.garden/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git` at the full revision `4a669eadaeac50b48efd77383219372be122f692`. The seed advertises `HEAD` and `refs/heads/main` at that revision, so stock Cargo resolution and a plain `git+https` flake input find it; the seed URL does not enter `net.git-fetch-exact-revisions`, and the unused `git.onix.computer` Campaign entry leaves that list. `deny.toml` and `tools/check-campaign-dependency-policy.rs` admit the seed URL in place of the old endpoint.

**Rationale:** Campaign's own `Cargo.toml` names this seed as its repository address, and the old endpoint no longer serves the repository. Adding a URL to the exact-revision list only helps for revisions the server does not advertise, and a server that refuses an unadvertised object turns that into an error with no fallback.

### Decision: Admit Campaign's AGPL license per crate

**Choice:** `deny.toml` gains `[[licenses.exceptions]]` entries allowing `AGPL-3.0-or-later` for `campaign-core` and `campaign-choregraph`, next to the existing `choregraph-history` exception, and `tools/check-campaign-dependency-policy.rs` lists both crates among the exceptions whose removal must fail the license check. `campaign-conformance` gets no exception: the license check skips dev-dependencies, `cargo deny` reported an exception for it as `license-exception-not-encountered`, and its negative control was accepted.

**Rationale:** ChaosControl is `AGPL-3.0-or-later`, so the relicensed dependency is compatible. The accepted requirement `chaoscontrol.unified_agpl.dependency_policy` keeps third-party review strict, so AGPL dependencies enter only through named per-crate exceptions and an unrelated third-party AGPL crate still fails the check.

### Decision: Read Campaign state only through the sealed accessors

**Choice:** Every read of a `FrontierState` field becomes the accessor of the same name: `admission()`, `candidates()`, `outstanding()`, `accepted_results()`, `settled()`, `findings()`, `reserved_resources()`, `stopped()`, `selection_count()`, and `state_id()`. Reads of the stop decision use the `Option<&StopDecision>` the accessor returns.

**Rationale:** The accessors return the same values the fields held, so the change is mechanical. ChaosControl never built or edited a Campaign state, so no test or recipe needs rebuilding through events.

### Decision: Pin the journal's state identities

**Choice:** A frontier test pins the Campaign state identity after the genesis segment and after the first published selection of one fixed journal (seed, configuration, scripted publication). Its message says to bump `GENESIS_SCHEMA` and refuse the previous value before re-pinning.

**Rationale:** The library suite passed unchanged before the journal change, so nothing tied the genesis schema to the identities a journal records. A later Campaign or adapter change that moves them would otherwise keep writing `v2` journals that the next binary cannot replay. At `a154ddd` the same journal yields different identities, so the test would have failed on this re-pin.

### Decision: Version the journal epoch header instead of migrating old journals

**Choice:** `GENESIS_SCHEMA` becomes `chaoscontrol.campaign-genesis.v2`. Replay refuses a genesis that names `chaoscontrol.campaign-genesis.v1` with a new `ReplayFailure::EarlierStateFraming`, and any other unknown schema with `UnknownSchema`, before it reads the root moment. The README tells operators to rerun such a configuration into a fresh output directory.

**Rationale:** An earlier-framing epoch's control records carry `state_before_id` values, its selection and round records carry selection identities, and its moment and entry records carry child-moment and candidate identities framed from selection and operation identities, all of which Campaign's chained framing derives differently. Replay would stop at the first post-admission control record with a Campaign `StaleState` refusal that does not say why. Rewriting the records would forge history the journal exists to protect. The genesis record is read first, so a schema mark refuses the whole epoch before anything replays.

### Decision: Keep ChaosControl's control session and measure per-event durability

**Choice:** ChaosControl keeps its own session, which plans several control events on planned branch values inside one journal segment. An ignored benchmark, `explorer::journal::tests::durable_append_cost_by_records_per_segment`, times 200 durable `CampaignJournal::append` calls of 2,048-byte records per case, for one record per segment (one control event per segment) and for the largest segment the KVM smoke's Campaign journal holds, and reports mean, median, 90th percentile, and maximum. A census of that journal counts records per segment. Together they price one segment per control event, the precondition for `ControlSession` recorded in Campaign's `provenance/CONTROL-SESSION-ADOPTION.md`; the measurement is evidence for that later decision, not a requirement of this change.

**Rationale:** `ControlSession::commit` advances one publication at a time on durable evidence. Adopting it needs either one journal segment per control event or a Campaign staged commit. The measurement prices the first option; the crash-recovery consequences of splitting a settlement segment need their own design.

### Decision: Show unchanged decisions by replaying KVM traces through both pins

**Choice:** Build the release explorer and `check-frontier-parity` from clean trees at the earlier pin (`a154ddd`, buildable only offline from a preserved Cargo Git cache, since `e7757a51` is no longer served) and at this change's committed head. Run the bounded rust-workload exploration of the 2026-09-26 smoke (`--vms 1 --rounds 5 --branches 4 --ticks 500 --seed 42 --mode fault-schedule --memory-mb 128`) once per frontier policy with each explorer, against one kernel and one initrd built from a committed tree. Check each pin's run pair with its own checker, and check each pin's run pair with the other pin's checker. Bind binaries, kernel, initrd, traces, and receipts by digest.

**Rationale:** `check-frontier-parity` replays each run's recorded observations and draws through the other policy and compares every decision field, so it is deterministic over recorded inputs. Replay drives only the policy a run is replayed through, so the Campaign backend runs only on each pin's legacy-run observations, and the campaign-run direction exercises legacy code, which is identical at both pins. Each pin's checker over both pins' legacy runs therefore shows that the `e7757a51` and `4a669ead` Campaign backends make the legacy policy's decisions, and so each other's, on each pin's legacy-run observations. Live traces cannot carry that claim: they record VM observations (new edges, snapshots, score bits), and the archived 2026-09-25 smoke showed that they do not repeat across runs on this host even under one policy. The legacy-diagnostic runs, which run identical code at both pins, serve as the repeatability control, and cross-pin trace digests are recorded as observations only.

## Risks / Trade-offs

- Resuming an output directory whose last epoch predates this change fails; the refusal is typed and documented. Only someone who kept the earlier binary, or a Cargo or Nix cache that still holds `e7757a51`, can resume it.
- The seed is one public endpoint; if it stops serving Campaign, fresh acquisition fails again until the pin moves.
- The KVM smoke covers one bounded workload and seed on one host; seed 42 draws no explore and prunes nothing in the 2026-09-26 runs, so those branches are not exercised by it.
- The durability measurement prices appends on one filesystem under shared load; it is not a model of crash recovery.
