## Decision

Reuse the existing `__cc_bundle_details!` helper in two phases. Its normal arm clones the caller's JSON details once, before binding any named conditions. An internal owned arm merges the evaluated conditions into that owned value without cloning again. Existing helper/default-details callers still use the original clone-and-merge call shape.

Apply the early snapshot to all three explicit-details bundle forms. Leave Option overloads, no-details aggregate behavior, condition ordering, overwrite precedence and guidance dispatch unchanged. No-full mode retains no serialization/JSON allocation and evaluates caller details once before conditions, so shared source does not depend on condition-name shadowing in either feature set.

## Verification

Reproduce a behavioral order failure using caller details that set state read by conditions. Cover all three bundle spellings with positive/negative aggregates and distinct payloads. Also compile and execute overlapping caller/condition identifiers, borrowed and temporary details, preserving unrelated caller keys and overwriting only named boolean keys without mutating caller data. Check exactly-once side effects and non-short-circuiting. Run an actual local-output example plus default/all-feature SDK suites, no-full shape checks, workspace check, SDK Clippy, formatting and evidence contracts.

The upstream comparison is the 0.3.0 boolean guidance helper, which captures/clones details before named bindings. Evidence is local instrumentation only; retain unrelated lifecycle blockers and leave this change active.
