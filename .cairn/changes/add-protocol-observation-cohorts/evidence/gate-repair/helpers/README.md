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

`install-hook-negative.nix.txt` is a missing-metadata-command fixture.
Its expected substitution failure guards against unnoticed changes to the pinned Crane hook.
