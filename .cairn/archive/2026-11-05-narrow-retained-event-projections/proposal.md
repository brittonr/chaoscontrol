## Why

Antithesis narrow retains selected synthetic event fields. ChaosControl can add projections with Map and MapText but cannot remove them before combining events or producing a compact timeline. Add a bounded selection of derived keys without discarding immutable source evidence.

## What Changes

Introduce Narrow { input, fields } to keep only listed derived keys. Empty lists clear projections; missing names are ignored; repeated names and order have no effect on selection. Preserve canonical event fields, source identity, and payload. Validate the list even on empty input and validate intermediate rows before pruning. Share existing structural/work/result/moment limits and receipt binding.

## Impact

Owner: chaoscontrol-replay maintainers. Consumer: retained log/timeline triage that projects diagnostic fields and then keeps a compact subset for downstream filters and results. Adoption: use Narrow around existing queries through the public enum or query-events JSON CLI. Existing queries are unchanged; new readers are required for this variant. This is only a local analogue of upstream narrow: canonical text/details remain evidence and cannot be removed. No hosted service, JavaScript, branch-lineage, VM/replay or capture-completeness claim.
