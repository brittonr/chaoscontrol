# Fault-mechanism fixture audit

Date: 2026-11-06.

Existing and new fixtures were mapped to the two fixture tasks. No production
behavior changed in this audit except one added negative test.

## Direction, modes, heal, bounds, throttle

| Case | Fixture |
| --- | --- |
| canonical link identity, self-link rejection | `link_identity_is_canonical_and_self_links_fail` |
| both directions apply | `both_direction_faults_apply_to_each_direction` |
| per-mode parameters | `directional_modes_require_their_parameters` |
| scoped heal | `overlapping_cpu_throttle_is_rejected` peers plus `test_network_heal_clears_packet_faults` |
| queue bounds | `directional_modes_require_their_parameters` (0, max, max+1) |
| throttle clamp, exhaustion, release | `cpu_throttle_clamps_budget_and_releases_at_tick` |
| host-style throttle rejection | `ThrottleTarget` admits only `Vm` or named `Process`; `malformed_pause_and_throttle_identities_are_rejected` rejects an unnamed process target, and no host target is representable |

## Ambient, adapters, pause bounds

| Case | Fixture |
| --- | --- |
| ambient command strings | `custom_effects_reject_ambient_command_strings` |
| unadmitted adapter | `custom_effect_without_adapter_is_rejected` and invalid-parameter rejection |
| malformed pause window | `pause_control_enforces_the_window_boundary`, `pause_requests_are_bounded_and_unique` |
| malformed requester identity | `malformed_pause_and_throttle_identities_are_rejected` |
| overlapping pause windows | `pause_requests_reject_overlapping_windows` |
| out-of-horizon request | `pause_control_enforces_the_window_boundary` (max window + 1) |
| request count bound | `pause_requests_enforce_the_count_boundary` |

## Boundaries

These fixtures cover admission and controller-side application only. Ledger
mechanism/suppression/overlap accounting, snapshot/exact-replay fixtures, and the
single KVM interaction campaign remain open.
