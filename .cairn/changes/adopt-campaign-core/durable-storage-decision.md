# Durable storage decision

## Existing contract

`ExplorerConfig.output_dir` is optional. Its default is `None`. `Explorer::new` returns `Self`, and the current run writes checkpoints only when an output directory exists.

Source: `crates/chaoscontrol-explore/src/explorer.rs`, the `ExplorerConfig` definition, its default, and the conditional checkpoint write in `Explorer::run`.

## Adopted requirement

The approved design forbids KVM expansion until ChaosControl durably accepts the exact selection event and branch update. The published adapter requires caller-supplied durable publication evidence.

An in-memory graph cannot supply that evidence. A hidden journal directory also introduces filesystem effects that the current no-output configuration does not declare.

## Approved decision

The user approved the proposed policy with `continue` after the explicit storage question.

The run must stop before KVM work unless the caller supplies journal authority. Existing output directories can authorize a dedicated journal within their boundary. A future injected journal capability can preserve non-filesystem storage choices.

This policy preserves method signatures but changes behavior for formerly accepted no-output runs. The user approved that behavior change. The initial implementation admits the existing `output_dir` field. It does not yet supply an injected journal API or prove durable publication.

Other choices require separate authority: an operator-selected default journal root, or an explicitly named legacy mode. Neither is selected here. There is no silent fallback and no in-memory durability claim.

## Required controls after the decision

- A supplied journal permits selection publication before expansion.
- Missing journal authority blocks before KVM, mutation, or snapshot effects.
- Failed or uncertain publication never permits expansion.
- Existing unrelated files remain unchanged.
- Stale branch generations require a fresh projection and plan.

Source acquisition is complete. The storage decision is complete. Durable journal implementation and Campaign runtime adoption remain open.
