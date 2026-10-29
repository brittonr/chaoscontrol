## Why

Antithesis's raw assertion interface uses caller-supplied string IDs. ChaosControl currently rejects a second distinct raw ID if its 32-bit compatibility hash matches an existing ID. Exact registration checks prevent misattribution but unnecessarily limit the admitted string-ID surface even though the existing protocol supports alias-free stable descriptors.

## What Changes

Runtime raw assertions use alias-free descriptors and resolve accepted identities by their exact string key in the `raw` namespace. Registration reuse is keyed by namespace/logical key and still requires an exact descriptor and source/display/must-hit match. Share the existing stable emission path, including sparse sampling, bounded serialization and optional-always flags. Remove the runtime raw hash-routing path. Compiled macros and their compatibility aliases are unchanged.

## Impact

Owner: ChaosControl SDK maintainers. Current consumer: third-party raw assertion framework adapters. Immediate outcome: distinct raw string IDs with colliding historical hashes can both emit, with independent observations and sparse samples. Durable capability: native string-key identity over the existing fingerprint-bound transport. Public call shapes and JSON record schemas stay unchanged; runtime raw descriptor fingerprints/catalog tokens change because compatibility aliases become absent. Regenerate runtime raw artifacts; do not reinterpret older identities. Local instrumentation only, not VM execution, replay or release readiness.
