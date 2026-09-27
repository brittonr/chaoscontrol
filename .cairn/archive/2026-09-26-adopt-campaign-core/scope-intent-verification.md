# Active change scope declarations

The frozen check of `56b861a9f3aa893cc647afb28de2eb994dcbcf40` passed the earlier code checks. The evidence-contract check then rejected an active change without a scope intent.

Eleven active changes lacked registry entries, including Campaign adoption. Each new entry records its consumer-side owner, experimental target, evidence prerequisites, and non-claims from the existing proposal. These entries describe intended work. They do not mark implementation tasks complete or promote current capabilities.

The existing capability records and prohibited-current-claim list remain unchanged. Historical intent entries remain available. No validator or scope check changed.

Nickel regenerated `contracts/product-scope/generated/product-scope.json`. The product-scope CLI regenerated the documentation from repository facts. This also corrected the stale workspace count from 21 to 23 and refreshed the active-change table. The count is not completion evidence.

The existing product-scope positive and negative unit tests pass. The real scope check passes without write mode. Its invalid Nickel fixtures still run. The previously failing `checks.x86_64-linux.evidence-contracts` check now passes.

The initial full-check denial remains a negative control for missing active intent. The unit suite also rejects unknown changes, duplicate entries, unsupported current claims, incomplete records, and stale documents.

Logs remain under `campaign/.pi/complete-20260906/`: `probe-config-full-nix.log`, `scope-intents-regenerate.log`, `scope-intents-tests.log`, `scope-intents-check.log`, and `scope-intents-evidence-check.log`.

Full frozen workspace acceptance and actual Campaign runtime adoption remain separate obligations.
