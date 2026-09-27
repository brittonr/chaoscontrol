# Guest probe configuration review

The evidence shell currently changes two fields after `VmConfig::default()`. Octet rejects that implicit external policy dependency.

The VMM will own an explicit single-vCPU configuration constructor. It will build every VM and CPU field directly. The caller will supply the run seed and TSC visibility. The compatibility `Default` implementation will use the same constructor with the existing zero seed and visible TSC.

The evidence shell will select hidden TSC explicitly. No wrapper will call `VmConfig::default()` on its behalf. No source allow attribute or disabled check is permitted.

The constructor must preserve the existing memory size, CPU identity, clock constants, SMP settings, boot command, and absence of optional disk, affinity, and diagnostic outputs. Tests must cover ordinary and boundary seeds, both TSC modes, and the compatibility default. Existing malformed and missing probe tests must remain intact.

The constructor does not execute KVM, authorize guest inputs, alter the Campaign adapter, or prove VM determinism. The evidence shell retains guest selection and execution. The VMM retains machine configuration and admission.

## Verification

The existing default-configuration test and guest probe tests passed before implementation. The VMM now builds every CPU field directly. It does not inherit `CpuConfig::default()` inside the constructor.

Three new tests check every VM and CPU field through exhaustive destructuring. They cover the compatibility default, the full-width seed boundary, both TSC modes, the exact boot command, and the absence of optional effects. Existing malformed and missing probe tests remain unchanged.

The VMM library passed 497 tests with nine existing ignores. The evidence library passed 144 tests without ignores. Explorer passed 214 tests with its existing ignored placeholder. Strict Clippy passed for VMM and evidence libraries and tests. Workspace formatting passed.

The previously failing `tigerstyle-chaoscontrol-focused` Nix check completed successfully. Its broader existing warnings remain. This is not a zero-finding claim for the whole workspace.

Logs remain under `campaign/.pi/complete-20260906/`: `probe-config-owner-baseline.log`, `probe-config-consumer-baseline.log`, `probe-config-library-tests.log`, `probe-config-explore-tests.log`, `probe-config-clippy.log`, `probe-config-format.log`, and `probe-config-octet.log`. Full frozen workspace acceptance remains a separate check.
