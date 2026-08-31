# Nickel toolchain cohort

ChaosControl uses Nickel CLI `1.17.0` from upstream commit `1320a983e6c3d1e2fb53dd2464b084b4903b1426`.

`flake.nix` declares that exact source. A Nix-generated lock entry binds the source archive. The package overlay makes profile, evidence, fixture, and developer commands use the same executable.

The cohort check rejects the prior `1.15.1` evaluator, ambient `nixpkgs#nickel` fallbacks, malformed source, missing imports, contract violations, unknown fields, invalid bounds, and unknown fault actions.

Profile projection receipts bind this evaluator identity:

```text
nickel-lang-cli nickel 1.17.0 (rev 1320a98)
blake3:bb5e202a62d399506f1eecaa8cb803108db19a0845505e927be416c0c442a09a
```

Nickel validates configuration shape. It does not grant run authority or define schedule, fault, guest, replay, assertion, or evidence meaning. ChaosControl still applies product admission before effects.

Passing compatibility fixtures proves only the selected configuration outcomes. It does not prove Nickel correctness, simulation correctness, workload correctness, replay success, finding truth, or release readiness.

<!--
r[impl chaoscontrol.nickel_toolchain.validation]
r[verify chaoscontrol.nickel_toolchain.boundary]
r[verify chaoscontrol.nickel_toolchain.compatibility]
-->
