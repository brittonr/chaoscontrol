## Why

Antithesis 0.3.0's boolean guidance helper clones caller details before binding named conditions. ChaosControl currently evaluates details afterward. A condition name can shadow an identifier used by the details expression, and details-side effects occur too late to influence the conditions as they do upstream.

## What changes

Capture the caller details once before condition bindings for `cc_assert_always_some!` (bundle form), `cc_assert_always_any!` and `cc_assert_sometimes_all!`. Merge the evaluated named booleans into that owned snapshot without a second deep clone. Preserve all-condition evaluation, aggregate logic, condition-key overwrite precedence, caller data and guidance records. Keep the existing helper call form and no-full call shapes.

## Consumer and ownership

Consumer: Rust workloads using Antithesis-style boolean bundles with computed details or overlapping caller/condition names. Owner: ChaosControl SDK maintainers. Existing macro callers adopt the fix unchanged. Immediate outcome: details are evaluated in caller scope before the named bindings. Durable capability: deterministic bundle argument evaluation with order/shadowing regressions.

This does not add generic struct detail support, alter numeric assertions or the protocol, or claim VM execution, replay, hosted parity or release eligibility.
