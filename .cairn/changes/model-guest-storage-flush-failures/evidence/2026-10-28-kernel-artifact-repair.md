# Kernel artifact leaf repair — 2026-10-28

## Observed regression

`local-multi-hypervisor-kvm-smoke` failed with `failed_phase=dogfood` and
`dogfood summary failed: missing accepted-snapshot-verdict-summary.json or
attempts-summary.json`. The preceding line named the configured kernel:

```text
/nix/store/…-chaoscontrol-vmlinux/vmlinux: Too many levels of symbolic links (os error 40)
```

`mkChaosKernel` exposed `$out/vmlinux` as a symlink into `kernel.dev`, while a
later external change moved guest-artifact hashing to a no-follow open
(`O_NOFOLLOW`). The declared kernel leaf therefore fails before the dogfood
summary exists. Isolated runs show the failure is later than this change's work:

| Revision | Result |
|---|---|
| `473c0c7` (this change's storage work) | `status=recorded passed=2`, exit 0 |
| current HEAD before repair | three attempts `status=failed passed=0`, exit 1 |
| current HEAD after repair | `status=recorded passed=2`, exit 0 |

## Repair

`mkChaosKernel` now copies the kernel bytes into `$out/vmlinux` with
`cp --dereference` and marks the leaf read-only. The no-follow artifact hashing
contract is preserved; no evidence check was relaxed, and no external commit
was reverted, amended, or re-authored.

## Bounded exception

This is an urgent one-time repair outside `model-guest-storage-flush-failures`
because the failing rail blocks that change's campaign tasks.

- Reason: a no-follow hasher and a symlinked artifact leaf are incompatible.
- Recurrence: returns if a kernel output again exposes a symlink leaf, or if
  other guest artifacts (initrd, disk) are produced as symlinks.
- Revisit trigger: when ChaosControl owns a general artifact-leaf rule for
  Nix-produced inputs, fold this copy into that rule.
- Owner: ChaosControl kernel packaging and evidence maintainers.
- Non-claim: this repairs one artifact-shape regression. It does not prove KVM
  guest behavior, campaign coverage, or release readiness.

## Campaign model coverage

`storage_schedule::run_storage_recovery_campaign` composes schedules, the
storage-safety oracle, participant progress, and a cohort-bound receipt.
Positive local-only and peer-repair campaigns pass; unnecessary remote repair,
missed available repair, and quorum-intersection storage loss fail; an
incomplete final drain reports `incomplete`. Ten schedule/receipt/campaign cases
pass with `cargo test -p chaoscontrol-smr --lib` (38 total in the crate).

## Verification

```sh
nix build .#checks.x86_64-linux.local-multi-hypervisor-kvm-smoke --no-link -L \
  --option builders '' --max-jobs 2 --cores 2
```

Before the repair: exit 1 with `passed=0` on three attempts. After: exit 0 with
`status=recorded … runs=2 passed=2` on the first attempt.
