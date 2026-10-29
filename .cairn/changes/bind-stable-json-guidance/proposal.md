## Why

Stable JSON guidance currently hashes `namespace:key` and resolves only the compact compatibility alias, kind and message. Two accepted stable sites in different namespaces may share that alias; with the same kind/message their guidance is dropped as ambiguous. This undermines Antithesis-style guidance observations even though full stable identity is available.

## What changes

Resolve `cc_guidance_json!` with the existing stable namespace/key catalog lookup and emit the accepted fingerprint through the common guidance writer. Retain the alias-based `emit_json` API for compatibility callers and numeric/boolean/catalog behavior. Add an explicit stable-key JSON entry point and remove the macro's temporary formatted alias allocation.

## Consumer, owner, adoption and scope

Consumer: Rust workloads using `cc_guidance_json!` for exploration guidance. Owner: ChaosControl SDK maintainers. Existing macro callers adopt the correction without source changes. Immediate outcome: admitted colliding stable sites retain correctly attributed observations. Durable capability: exact stable binding for guidance, covered by positive and rejection regressions.

This is local SDK instrumentation parity work, not hosted-platform parity, improved exploration effectiveness, guest execution, replay or release readiness. No VMM semantics, catalog identity model, guidance scheduling or runtime protocols change.
