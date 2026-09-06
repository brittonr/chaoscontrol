# Edit-helper review artifacts

The `.rs.txt` files retain the one-off Rust edit helpers.
They are not product tools or a Rust name resolver.
The source and compiler checks remain the acceptance evidence for each batch.

The current helper controls used the repository Nix shell and explicit Cargo and Rust paths from `nightly-2026-07-22`.
Older shebangs remain in the retained sources. Those shebangs are not evidence of a supported standalone invocation.
The helpers preserve strings and comments and reject the listed ambiguous scopes.
The protocol rename helper performs token replacement only. Manual source review and public compatibility exports complete that change.

The report-based helper selects only compiler-reported bindings in the supplied workspace files.
It does not edit generated BPF skeletons or unknown absolute paths.
It does not remove those findings from Octet or change the lint scope.
The logs retain proposals, applied edits, and positive and negative controls.

The later binding batch retains `qualify-owner-bindings.rs.txt`.
It processes reported bindings separately and preserves unrelated exports and conditional imports.
Its controls reject namespace conflicts, selected conditional exports, opaque macro uses, comment overlap, and string-valued attribute references.
The explicit Fenix revision is `03864c059200a8a96f2ee6bb050c69eae96f57ca`, inside the repository development shell.
This toolchain runs the helper only. Product compiler and Cargo selection remain unchanged.

`binding-exact-syntax-attempt.rs.txt` retains a failed comparison method.
It rejects formatter changes to single-item import groups and punctuation. It is not acceptance evidence or the current helper.
`binding-formatted-plan.log` instead records exact comparisons after reconstruction from Git source and the same formatter.
Neither method proves Rust name resolution or semantic equivalence.

`install-hook-negative.nix.txt` is a missing-metadata-command fixture.
Its expected substitution failure guards against unnoticed changes to the pinned Crane hook.
