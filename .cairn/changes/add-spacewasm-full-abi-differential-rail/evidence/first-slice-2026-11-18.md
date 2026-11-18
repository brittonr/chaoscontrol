# First slice, 2026-11-18

Branch `cairn/spacewasm-full-abi-rail-20261118` from `origin/main` `ec8aa799`.
This slice covers the typed profile and pure module admission only. No engine
ran, no Mantle bundle was read, and no scenario executed.

## Profile contract

`contracts/evidence/spacewasm-full-abi-differential-profile.ncl` types the
profile. The example
`contracts/evidence/examples/spacewasm-full-abi-differential-profile.ncl` pins
Mantle `67fe04fe88edce20cc1bdf3faa7e51fcd8bfbeef`, the bundle identity verified
on 2026-10-30 (`774331f6c2defe75e30c354b2b3080838cf56438bbd99268779fe4590f8c9f39`)
with publication still `release-authority-receipt-required`, and a one-page
memory budget, matching Mantle's `molten-abi-memory-limit` fixture.

With the pinned Nickel 1.17.0 (`github:nickel-lang/nickel/1320a983e6c3d1e2fb53dd2464b084b4903b1426`):

| Command | Result |
| --- | --- |
| `nickel export --format json contracts/evidence/examples/spacewasm-full-abi-differential-profile.ncl` | exit 0; committed as the example JSON (2,842 bytes) |
| `nickel export` of `…widened-hostcalls.invalid.ncl` | rejected: contract broken by the value of `hostcalls` |
| `nickel export` of `…table.invalid.ncl` | rejected: contract broken by the value of `tables` |
| `nickel export` of `…reply-overflow.invalid.ncl` | rejected: the reply bound does not fit the one-page memory |
| `nickel export` of the MVP example, compared with its committed JSON (control) | identical |

`nickel-cohort-exact` now repeats the export comparison and the three
rejections.

## Pure admission

`chaoscontrol_wasm_differential::full_abi` validates the profile at the
consumption boundary and admits modules only on the exact ABI surface. The
tests encode modules shaped like Mantle's `molten-abi-*` fixtures:

| Test | Covers |
| --- | --- |
| `example_profile_is_the_exact_molten_abi` | the example JSON parses and equals the ABI constants |
| `widened_or_incoherent_profiles_are_rejected` | an extra hostcall, a table, an oversized reply, an unpublished bundle and a missing non-claim each fail validation and deny admission |
| `fixture_shaped_abi_modules_are_admitted` | the roundtrip shape and a module importing both hostcalls are admitted, with exact digest, memory and run index |
| `the_mvp_profile_never_admits_a_full_abi_module` | the MVP admission denies both shapes |
| `undeclared_or_misdeclared_hostcalls_are_denied` | an undeclared hostcall, a WASI import, a wrong signature and a duplicate hostcall |
| `memory_table_start_and_growth_outside_the_abi_are_denied` | two pages under a one-page budget, an unbounded memory, no memory, a table, a start function and `memory.grow` |
| `exports_must_be_the_exact_abi_set` | a missing export and a wrong `molten_run` signature |
| `input_bound_precedes_parsing` | the input byte bound |

In `nix develop` (rustc 1.98.0):

| Command | Result |
| --- | --- |
| `cargo test -p chaoscontrol-wasm-differential` | 42 and 13 passed, including the 8 tests above |
| `cargo clippy -p chaoscontrol-wasm-differential --all-targets -- -D warnings` | passed |
| `cargo fmt -p chaoscontrol-wasm-differential -- --check` | passed |

## Not executed

The Wasmtime 49.0.5 embedding harness, the SpaceWasm full-ABI host, the
hostcall model, bundle admission and every scenario remain unimplemented.
Evidence-bearing runs also need release authority to publish the Mantle
bundle.
