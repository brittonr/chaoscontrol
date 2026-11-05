## Decision

Add `must_hit: Option<bool>` to assertion details. Omit unknown metadata on serialization and default absent historical fields to `None`; explicit null is rejected consistently with other optional authority-bearing summary fields. Both oracle projection paths copy the field without changing assertion identity.

For always, sometimes and reachable assertions with zero hits, `true` means failed, `false` means passed, and absent means unexercised. Unreachable remains passed without hits and failed when reached. Nonzero-hit verdicts and counter validity are unchanged. Campaign merge requires equal optional metadata rather than silently choosing one seed's semantics.

Keep the existing assertion-summary.v2 envelope and update all owned readers and the Nickel contract together. Old documents without the field retain their old behavior; old binaries using closed records require an upgrade to consume documents that include it.

## Risks and verification

Test both metadata values and absence, all four kinds, observed counter precedence, forged verdicts, malformed metadata, and campaign conflicts. Exercise strict oracle projection through both Rust summary readers. No KVM is required for this pure reporting change. The native change stays active; implementation evidence does not authorize archive or release.
