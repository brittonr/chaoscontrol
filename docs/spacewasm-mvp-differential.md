# SpaceWasm MVP differential rail

This rail compares bounded WebAssembly 1.0 core-module observations from SpaceWasm and Wasmtime. It is diagnostic evidence only.

## Admitted cohort

The Nickel profile binds these inputs:

- Mantle commit `57fa0f5c2f8fb1edeed6d6113b57d32357612e34`, the stable-evidence producer, checked against the locked Mantle flake input;
- NASA SpaceWasm commit `e24cf09355a90497148eb5029fdb8e3400bd63e3`;
- Mantle bundle identity `86142d4286235ab4c6e8fbf8b08daafa2c921363d7968ea0302ed5212b649896`;
- bundle manifest BLAKE3 `2de0a78cb1655918e2c42880b21bf03f71aa36152382ef416ce56ab625180d5e` and exact member BLAKE3 values;
- SpaceWasm runner BLAKE3 `732da9b7276cb2c36dc0e5e308e8fd8e6bc88ddd665346c5afb1f351eda2a579`;
- Wasmtime `41.0.3` from the ChaosControl Nix lock;
- the `wasm1` and `mutable-globals` feature intersection;
- a hostless `run: () -> ()` ABI with no imports, memory, table, or `memory.grow` admission;
- complete-byte and one-byte chunk schedules;
- fixed process, input, output, member, fuel, resume, case, generator, and shrink bounds.

The shell remeasures all bundle members before execution. It does not use a fallback source, runtime, module, or profile.

These digests were measured from the bundle built in the locked ChaosControl Nix environment. They depend on that environment: a different lock can produce different bytes.

## Stable producer evidence

Mantle binds each upstream test run to a canonical stable report (`mantle-spacewasm-stable-report-v1`) and a receipt. Raw stdout, stderr, and run records stay in Mantle's separate run archive. The consumer admits a bundle only when:

- both suites, `upstream-unit-tests` and `upstream-spectest-address`, carry a stable report and a receipt at the exact `reports/checks/` paths with role `check-receipt`;
- no member is a raw capture or run record: `stdout.txt`, `stderr.txt` or `run-record.json` suffixes, the `runs/` prefix, or a raw-capture role;
- each stable report parses under the pinned schema with encoding version 1 and has unique, canonically ordered, passed tests;
- each stable identity recomputes from the report's facts;
- each receipt records `passed` status and binds the same report and command.

## Execution and comparison

The fixed corpus covers completion, malformed input, truncated streaming input, unreachable traps, and fuel exhaustion. A fixed seed also produces bounded valid modules and malformed-magic variants.

The Nix rail builds a consumer-owned resume probe offline from the bundle source, dependency closure, and Rust toolchain. The probe compares uninterrupted execution with repeated one-instruction segments across out-of-fuel boundaries. It also compares complete-byte and one-byte streaming decode of the same generated module.

SpaceWasm runs through the exact Mantle host runner. Wasmtime runs with all post-MVP proposals disabled, explicit fuel, and a timeout. Neither engine receives WASI, filesystem, network, environment, clock, random, or host-function authority from the guest profile.

The pure core normalizes these facts:

- completion or module rejection;
- trap class;
- return values;
- state identity when the profile exposes state;
- resource exhaustion or timeout.

The initial hostless corpus has no return values or observable guest state. The report records `null` state identity instead of inventing a state claim. A mismatch records the first differing normalized field and retains both engine observations. Generated mismatches use bounded typed shrinking. The report retains the minimized bytes, identities, attempt count, and preserved first-difference predicate.

## Run the rail

Use the Nix check for the exact producer and reference-engine cohort:

```sh
nix build .#checks.x86_64-linux.spacewasm-mvp-differential -L
```

For a local diagnostic run, provide a verified Mantle bundle and an exact Wasmtime binary:

```sh
chaoscontrol-wasm-differential \
  --profile contracts/evidence/examples/spacewasm-mvp-differential-profile.json \
  --mantle-revision 57fa0f5c2f8fb1edeed6d6113b57d32357612e34 \
  --bundle /path/to/mantle-spacewasm-reference-bundle \
  --wasmtime /path/to/wasmtime \
  --out report.json \
  --artifacts generated-modules
```

The command fails closed for profile drift, producer revision drift, bundle drift, missing members, missing or tampered stable reports, raw run captures, failed or skipped producer tests, post-MVP features, timeouts, malformed reports, and comparison mismatches. The Nix check also runs admission denial controls against the pinned bundle for a wrong producer pin and for stale manifest, identity, and runner digests.

## Evidence boundary

A matching report proves only that the two selected engine builds produced the same normalized observations for the admitted bounded corpus. It does not prove SpaceWasm correctness, SpaceWasm/Wasmtime equivalence, memory safety, WebAssembly conformance, flight qualification, sandbox effectiveness, production readiness, or release eligibility.
