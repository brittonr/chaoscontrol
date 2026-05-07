# Verification

## Evidence captured

- Accepted VM campaign transcript: `evidence/vm-campaign-kcov-20260507T165920Z/transcript.log.gz`
- Accepted VM campaign output: `evidence/vm-campaign-kcov-20260507T165920Z/run/`
- Classification receipt: `evidence/vm-campaign-kcov-20260507T165920Z/run/evidence-classification.json`
- Human summary: `evidence/latest-vm-campaign-attempt.md`
- File manifest: `evidence/latest-vm-campaign-files.txt`

## Accepted VM campaign result

`timeout 3600s nix run .#explore-rust-workload -- evidence/vm-campaign-kcov-20260507T165920Z/run` completed with exit code 0 after building the KCOV kernel, initrd, and package rail.

Report highlights from `run/report.txt`:

- Exploration rounds: 5
- Total branches explored: 20
- Unique edges found: 25
- Bugs discovered: 0
- Assertion coverage: 5/5 exercised, 5 passed, 0 failed
- Wall-clock time: 50m 43s

## Evidence classification

The accepted run's `evidence-classification.json` uses schema `chaoscontrol.vm_campaign.classification.v1`, classifies the result as `bounded-vm-campaign`, and preserves the replay boundary: campaign output may contain VM execution evidence, but standalone replay proof still requires replay/minimization artifacts.

## Source fix needed for validation

The initial VM attempt reached `/init` but failed because the VM rail used `mkChaosKernel { }` while the guest expected KCOV (`kcov: open failed (errno=2) — kernel lacks CONFIG_KCOV?`). The validation fix changes both `rust-workload-sim` and `explore-rust-workload` to use `mkChaosKernel { kcov = true; }`.

## Commands

Captured in `evidence/vm-campaign-kcov-20260507T165920Z/transcript.log.gz`:

```text
nix build .#kcov-vmlinux .#initrd-rust-workload .#packages.x86_64-linux.default --no-link -L
timeout 3600s nix run .#explore-rust-workload -- openspec/changes/archive/2026-05-07-add-rust-workload-harness-vm-validation/evidence/vm-campaign-kcov-20260507T165920Z/run
```

Additional closeout validation is captured in the commit transcript/status before archive.

## Closeout validation

Before archive:

```text
python ~/.hermes/skills/agentkit-port/openspec/scripts/openspec_helper.py verify add-rust-workload-harness-vm-validation --json
# warning only before final task closure: tasks incomplete {'done': 2, 'todo': 2, 'in_progress': 0}
openspec validate add-rust-workload-harness-vm-validation --strict
# Change 'add-rust-workload-harness-vm-validation' is valid
git diff --check
# no output
```
