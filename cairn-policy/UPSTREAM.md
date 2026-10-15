# Cairn Policy Source

The policy source files come from [OnixResearch/cairn](https://github.com/OnixResearch/cairn) at revision `b6759012db4dab0158936e995b7bad39ac10465b`.

Imported files:

- Cairn `cairn-policy/contracts.ncl` becomes `cairn-policy/contracts.ncl`.
- Cairn `cairn-policy/default.ncl` becomes `cairn-policy/upstream-default.ncl`.

`cairn-policy/default.ncl` is the ChaosControl-owned overlay. It sets the local policy name, refresh command, and traceability profile.

Regenerate the projection with:

```text
cairn policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json
```

This record proves source identity only. It does not prove policy correctness, consumer compatibility, or release readiness.
