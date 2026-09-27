# Adoption checkpoint

## Baseline and ownership

The isolated worktree starts at `31300fa1a2d29c7496e8316f065c156f80343143`. The refreshed origin has the same head. The primary checkout remains untouched.

The exploration library baseline passes 206 tests. Its existing KVM placeholder remains ignored. This includes frontier, input-tree, and Explorer tests. No ignore attribute changed.

The multi-seed runner in `campaign.rs` remains outside the adapter. ChaosControl retains execution, snapshots, coverage, findings, persistence, and report authority.

## Published source bindings

Cargo manifests and Nix inputs name exact published revisions:

- Campaign: `e23e3edf1dc6a8c612a4ea33a3b805bda1173e3b` at `https://git.onix.computer/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git`.
- Choregraph History: `b3e08e19750f53bdbcae970cdf58a47a791ed20b` at `https://git.onix.computer/zL2ncTUeASVYwcoGkEXv9JKgGbAF.git`.

Nix generated the lock changes. Cargo added only the three required packages to its existing lockfile. A trial full lockfile regeneration changed unrelated versions and was discarded.

The existing isolated SDK guest-install repair remains in `flake.nix`. Earlier legacy KVM evidence covers that repair, not Campaign adoption.

## Rank conversion

`crates/chaoscontrol-explore/src/campaign_adapter.rs` owns a pure, versioned conversion to Campaign integer ranks. It does not select or execute candidates.

The rank tuple preserves effective score, original score, and stable insertion order. Finite nonnegative IEEE-754 bit order preserves score order without quantization. Signed zero is normalized. Every admitted selection count converts exactly to `f64`.

The adapter applies the legacy division-based decay. It rejects a nonzero Campaign subtractive penalty rather than applying decay twice. It also rejects invalid scores and exhausted selection counters.

Five added tests cover repeated selection, a bounded score/count grid, tie breaks, signed zero, full-width insertion IDs, invalid scores, counter exhaustion, and double decay. The library now passes 211 tests with the same existing ignored placeholder. Focused strict Clippy passes after replacing a constant assertion with a runtime before/after ordering check.

This is not yet a complete guidance adapter. Moment identities, entropy binding, durable publication, shell integration, and KVM parity remain open. Explorer still uses its original frontier.

## Fresh Cargo acquisition blocker

A direct Git fetch of the exact Campaign revision succeeds. Nix acquisition with `allRefs=1` also succeeds. Cargo resolution from an empty Git cache fails, even with the generated lockfile and Git CLI transport.

The Cargo trace fetches only ordinary branch refs and tags. The Campaign checkpoint is reachable through a Radicle namespace, not the exported ordinary branch refs. The error is:

```text
revspec 'e23e3edf1dc6a8c612a4ea33a3b805bda1173e3b' not found
```

An explicit fetch into the local Cargo cache permitted lockfile generation and the local tests. The fresh-cache control rejects that bootstrap as sufficient acceptance evidence. No clean-consumer or release claim follows from those cached results.

The existing Radicle HTTP server supports `/{rid}.git/{nid}/...` and sets `GIT_NAMESPACE` for that route. The current reviewed nginx policy exposes only exact non-namespaced Git endpoints. A reviewed publisher-bound route can expose ordinary branch refs without changing the source commit or weakening signed references. No such route or live override was added in this checkpoint.

Onix Core owns that transport-policy follow-up. Do not work around it with a sibling checkout, a moving revision, or a preloaded cache as a deployment requirement.

## Retained operator evidence

```text
adoption-resume-baseline.log
adoption-pin-nix.log
adoption-cargo-fetch-trace.log
adoption-fresh-fetch.log
adoption-pinned-tests.log
adoption-rank-final-tests.log
adoption-rank-final-clippy.log
```

The dependency task remains open until fresh Cargo acquisition passes. Full Octet, full-flake acceptance, actual-adapter conformance, and Campaign-backed KVM evidence remain unproven.
