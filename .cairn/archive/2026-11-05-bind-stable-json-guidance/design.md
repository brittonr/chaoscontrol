## Decision

Use `assertion_catalog::resolve_stable(namespace, key, kind, message)` for the stable JSON entry point. `cc_guidance_json!` forwards namespace/key directly rather than formatting and hashing them. Fail closed on unresolved or conflicting catalog identity, consistent with other guidance APIs.

Factor the existing private guidance writer into an alias resolver and a bound-identity writer. Numeric, boolean, compatibility JSON, and initialization records keep their current resolution and serialized shape; the stable entry point shares only the bound writer. Preserve existing location, payload, maximize, hit, catalog token and fingerprint fields.

The new `emit_json_with_key` API parallels `assert_raw_with_key`. The alias-based `emit_json` remains a distinct supported compatibility API, not an obsolete fallback. There is no new identity cache, allocation of a formatted alias, protocol or dependency.

## Verification

Use the existing fixed cross-namespace FNV collision, with the same message and kind at both sites so alias-only binding is ambiguous. Compare each JSON guidance fingerprint/token with its corresponding accepted assertion hit and verify payload, direction and location. Reject missing namespace/key and wrong kind/message. Run existing guidance tests, feature-matrix SDK tests, and an actual temporary local-output example. Remove that example after execution. Scope evidence to local instrumentation; no KVM campaign is required.
