# Exact-revision transport review

## Goal

Fetch the declared Campaign revision from an empty Git cache without changing source refs, signed-reference policy, or the selected commit.

## Mechanisms and evidence

| Mechanism | Result |
| --- | --- |
| Stock ordinary-ref fetch | Cannot find the namespace-only checkpoint. |
| Stock namespace fetch | The isolated fixture reproduces missing HEAD. |
| Namespace HEAD mutation | Git advertises HEAD, but Radicle requires qualified namespace refs and validates them against signed refs. Rejected for live storage. |
| Unconditional direct-ID fetch | Local controls pass, but the behavior change extends to unreviewed servers. Rejected as the final scope. |
| Exact-URL transport capability | Selected. Complete object IDs use direct fetch only for the configured Campaign URL. Other remotes retain stock behavior. |

These are correlated serial passes, not independent reviewer approval. The bounded search used an isolated Git fixture, the pinned Radicle source, and the official Cargo 1.98.0 source.

The package runs 18 Git-related Cargo library tests. The Nix transport fixture compares stock and repaired Cargo. It checks exact payload execution, a second fresh locked acquisition, named branches, missing default HEAD, missing revisions, unlisted URLs, and malformed configuration.

No check accepts another revision after an error. The fixture never changes a real Radicle store. It retains the original synthetic source branch and does not add a namespace HEAD.

## Ownership and limits

ChaosControl owns the client configuration, source pin, package patch, and behavioral fixture. Onix Core owns public forge policy and host deployment. The server can still deny an object request. The client does not turn that denial into success.

The source revision remains `e23e3edf1dc6a8c612a4ea33a3b805bda1173e3b`. The development compiler remains Rust 1.98.0. Other Git URLs retain their existing fetch path.

See `docs/campaign-source-acquisition.md` for source identity and maintenance details. Fresh public acquisition, full workspace acceptance, the actual product adapter, and runtime cutover remain separate gates.
