# SpaceWasm MVP differential rail

This rail compares bounded WebAssembly 1.0 core-module observations from SpaceWasm and Wasmtime. It is diagnostic evidence only.

## Admitted cohort

The Nickel profile binds these inputs:

- Mantle commit `a141fcbaafe41f9a413a81275a33fe915bfca370`;
- NASA SpaceWasm commit `e24cf09355a90497148eb5029fdb8e3400bd63e3`;
- Mantle bundle identity `cee7190f2f78321b07f3d1f493baaa5b2cb74d517eb4f229c7e7a6094b877342`;
- exact bundle manifest and member BLAKE3 values;
- SpaceWasm runner BLAKE3 `be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221`;
- Wasmtime `41.0.3` from the ChaosControl Nix lock;
- the `wasm1` and `mutable-globals` feature intersection;
- a hostless `run: () -> ()` ABI with no imports, memory, table, or `memory.grow` admission;
- complete-byte and one-byte chunk schedules;
- fixed process, input, output, member, fuel, resume, case, generator, and shrink bounds.

The shell remeasures all bundle members before execution. It does not use a fallback source, runtime, module, or profile.

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
  --bundle /path/to/mantle-spacewasm-reference-bundle \
  --wasmtime /path/to/wasmtime \
  --out report.json \
  --artifacts generated-modules
```

The command fails closed for profile drift, bundle drift, missing members, post-MVP features, timeouts, malformed reports, and comparison mismatches.

## Evidence boundary

A matching report proves only that the two selected engine builds produced the same normalized observations for the admitted bounded corpus. It does not prove SpaceWasm correctness, SpaceWasm/Wasmtime equivalence, memory safety, WebAssembly conformance, flight qualification, sandbox effectiveness, production readiness, or release eligibility.
