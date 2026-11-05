## Decision

Reuse the existing hidden serde_json re-export convention for `linkme`, gated on `full`. For automatic assertions, stable assertions and guidance registration, use `#[$crate::linkme::distributed_slice(...)]` and `#[linkme(crate = $crate::linkme)]`, matching the upstream pattern. The second attribute is required because the proc-macro expansion itself otherwise resolves a caller-side dependency. Internal distributed-slice declarations keep their direct SDK-owned dependency.

Migrate every current non-SDK direct `linkme` manifest entry and the workload template. Keep the SDK dependency and historical receipts unchanged. Let Cargo update the workspace/template lockfiles without source-pin changes. No compatibility shim or alternate registration convention remains.

The actual template smoke exposed an existing strict-reader error: one shared
details object repeated `category = workload-driver` for sites cataloged as
`operation` and `branch`. Remove that redundant field from the shared details;
the category-specific macro catalog remains authoritative. Do not weaken the
reader or change assertion categories/outcomes. Re-run the actual template and
record its report/quality verdict, including any host-random unhit branch.

## Verification

Run baseline SDK catalog tests and reproduce compilation failure in an external SDK-only consumer with a renamed dependency before editing. Afterward exercise all three registration paths, linked unexecuted declarations, assertion metadata/outcomes and numeric/boolean guidance. Run the same consumer without full and verify no output; preserve const-message rejection. Build/run the actual template without direct linkme and admit its report through the strict reader/quality CLI.

The migrated guest manifests plus workspace compilation are the durable regression rail; do not add tests that only pin forwarding/source text or dependency wiring. Run full SDK/guest tests, no-full/sparse cases, workspace check, Clippy, formatting, product-scope/evidence checks and native Cairn gates. Record local instrumentation evidence only, remove temporary smoke artifacts, and preserve unrelated lifecycle failures and user changes.
