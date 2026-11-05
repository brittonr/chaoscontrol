## Decision

Evaluate a profile-selected `tsc_progress_mode` with two values:

- `per-exit` (default, unchanged): `tsc += DEFAULT_TSC_ADVANCE` once per VM exit or
  scheduling quantum, as today.
- `counter` (opt-in, design only here): `tsc = base_tsc + scale(retired_guest_instructions)`,
  where the retired count comes from the same guest-only PMU progress the scheduler
  already admits, sampled at the existing quantum/exit boundaries.

`base_tsc` and the last admitted retired count are part of snapshot state, so a
restore resumes the same mapping instead of re-anchoring. Sampling happens only at
boundaries where the progress source is already read, so no new per-exit
instrumentation or host clock read is added.

Open design questions this change must answer before implementation:

1. **Skid.** `pmu_kvm_test` measured roughly 41 instructions of per-exit skid on
   Zen5. Counter mode is admissible only if skid is stable from one snapshot across
   repeats within the declared tolerance, or if the design rejects that host
   profile explicitly. Skid is never subtracted by guesswork.
2. **HLT deadline jumps.** Uniprocessor HLT currently jumps to the PIT deadline and
   SMP HLT calls `tick()`. Under counter mode a halted vCPU retires nothing, so the
   design must state whether the jump advances `base_tsc` to keep timer semantics
   (a declared halt advance) or forbids the mode with HLT-driven timeouts. Either
   answer is a bound, not a silent fallback.
3. **PIT `tsc_khz`.** Timer programming derives from `tsc_khz`; counter mode must
   keep the PIT consistent with the TSC mapping (same base/scale, no second clock).
4. **Snapshot/restore.** Snapshot state gains the mode, `base_tsc`, scale, and last
   retired count. Restoring a snapshot written under a different mode is rejected.
   Replay compares tick/event boundaries under the recorded mode only.
5. **Scheduler interaction.** The mode must not change scheduling decisions; the
   scheduler reads progress for its own policy, and the clock consumes the same
   value. A mode that changes chosen branches is a different experiment.

Acceptance fixture (design target, not yet implemented): a guest busy loop with a
TSC-based timeout that never exits. Under `counter` the timeout must fire at the
declared retired-instruction budget; under `per-exit` the same fixture must be
recorded as timing out only when something else forces exits, and the recorded
outcome must show the difference rather than a fabricated pass. Two identical runs
from one snapshot must produce identical observed expiry points, or the fixture
records the drift and the mode stays unadmitted.

## Verification

Design review gates only at this stage: proposal/design/tasks gates, dependency
review against `harden-pmu-transition-evidence` (all implementation tasks unchecked,
`claims_pmu_support = false`), and cross-checks that no existing clock requirement
in `guest-os-determinism-boundary` is contradicted. No runtime claim, no KVM run,
and no measurement is asserted by this change.

## Boundaries

Counter time is a time-model option, not a determinism proof. Retired-instruction
counters are host-dependent, may skid, and do not establish equal execution across
hosts. The admitted `guest-os-determinism-boundary` surface continues to hide direct
TSC reads; this change does not expose host time or relax that boundary.
