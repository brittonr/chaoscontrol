# Strict Admission Tasks

## Foundation

- [x] [serial] Define admission, early rejection, evidence boundaries and source-reference discovery fallback. r[chaoscontrol.event_entry_exact.admission] r[chaoscontrol.event_entry_exact.early] r[chaoscontrol.event_entry_exact.evidence]

## Implementation

- [x] [serial] Require full event-entry exactness for exact SMP and PMU exact remainder; reject portable KVM before mutation while preserving single-vCPU and pure transitions. r[chaoscontrol.event_entry_exact.admission] r[chaoscontrol.event_entry_exact.early]
- [x] [serial] Migrate affected production consumers to honest rejection and retain explicitly partial diagnostics and IRQ capability-negative evidence. r[chaoscontrol.event_entry_exact.evidence]
- [x] [serial] Run integrated positive single-vCPU, negative SMP, pure capability, affected consumer and diagnostic checks; record actual outcomes before completing implementation or archiving. r[chaoscontrol.event_entry_exact.admission] r[chaoscontrol.event_entry_exact.early] r[chaoscontrol.event_entry_exact.evidence]

## Exercised evidence and lifecycle boundary

`evidence/2026-11-06-strict-admission-proof.ncl` exported successfully through Nickel contracts. The focused sim-core/VMM/evidence all-targets run passed 1,392 tests with zero failures and 13 ignored cases; the known failing IRQ diagnostic remains explicitly ignored, with its original instruction-boundary expectation unchanged. Real SMP binaries reject with the typed event-entry capability reason before guest-file reads; the stress producer creates neither requested receipt nor log directory. A fresh query invocation matched 374/374 retained events, then a real single-vCPU checkpoint restore executed the allowlisted guest observation from tick 1210 to 1211; both receipts validated. The release fixture checker reports seven rows, one positive and thirteen negative controls. Final denied-warning Clippy and formatting checks passed. Initial compile, formatting and lint failures and their corrections are retained in the receipt.

This completes the strict admission repair, not instruction-exact SMP execution. The separate coarse change remains planning-only. Old SMP execution acceptance tasks and historical receipts are unchanged. The selected Cairn executable is unavailable and cannot be restored from configured substitutes; ambient foundation gates are advisory only. No selected-cohort lifecycle closure, accepted-spec sync, archive, full release-matrix execution, production approval or coarse-mode implementation is claimed.
