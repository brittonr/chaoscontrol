## Context

`docs/replay-readiness-status.md` supports bounded snapshot-backed replay for named workloads and labels arbitrary guest/device determinism as unproven. `crates/chaoscontrol-vmm/src/bin/determinism_stress.rs` already runs repeated same-seed VM/controller configurations, but its current output is human-only, duplicated comparison logic, and not receipt-shaped.

## Goals / Non-Goals

**Goals:**
- Make repeated-run VM drift detection machine-readable.
- Keep comparison logic pure and unit-testable outside KVM.
- Let operators opt into dlog structural comparison without forcing bulky logs into normal runs.

**Non-Goals:**
- No claim of universal determinism across arbitrary guests/devices.
- No mandatory slow KVM gate in the default local test suite.
- No raw dlog/checkpoint artifact commitment requirement.

## Decisions

### 1. Pure receipt/comparison core

**Choice:** Add `chaoscontrol_vmm::determinism_gate` with serializable fingerprints, case reports, mismatch details, and a top-level receipt.

**Rationale:** The drift gate should be testable without KVM and reusable by future wrappers or Nix checks.

**Alternative:** Keep all logic inside the binary. Rejected because it would preserve duplicated untestable comparison code.

### 2. Bounded binary extension

**Choice:** Extend `determinism_stress` with `--receipt <path>` and `--dlog-dir <dir>` while retaining positional compatibility for `<kernel> <initrd> [N]`.

**Rationale:** Existing ad-hoc usage keeps working, while evidence runs can archive a stable receipt and optional dlogs.

**Alternative:** Add a separate binary. Rejected because it would split the existing stress workflow from the evidence workflow.

### 3. CRC32 artifact fingerprints

**Choice:** Record deterministic `crc32:<hex>` input fingerprints using the crate's existing `crc32fast` dependency rather than introducing a SHA dependency.

**Rationale:** This is a drift-gate receipt, not a public tamper-evidence artifact contract. Avoiding dependency churn keeps the slice narrow.

**Alternative:** Add `sha2`. Deferred until broader evidence contracts require cryptographic hashes for this receipt class.

## Risks / Trade-offs

**Dlog overhead** → Dlogs remain opt-in via `--dlog-dir`; the default stress path only compares fingerprints.

**Overclaiming** → The spec and receipt name this as `vm-determinism-drift`, a bounded drift detector, not a universal proof.

**KVM availability** → Unit tests cover pure receipt logic; live KVM proof remains an operator-invoked gate with explicit kernel/initrd inputs.
