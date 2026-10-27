# Directional Fault Admitted Cohort

Repo-owned admitted artifacts and a bounded redb campaign.

- Source revision: `3c134aa`
- Host binary: `/nix/store/37d4gv9fmvka387201s4sygfid02c877-chaoscontrol-0.1.0/bin/chaoscontrol-explore`
- Guest kernel: `/nix/store/7kiw7p0cyqchvfcizgy0vjzlv8101k98-chaoscontrol-vmlinux/vmlinux`
- Guest initrd: `/nix/store/q284nvk77i483x1wga1zzpq41v2rif6f-chaoscontrol-initrd-redb`
- Disk image: `/nix/store/8i68yqrfslag4nxlkn331sq1n93bh8x5-redb-disk-image`
- Campaign: `run --vms 1 --rounds 3 --branches 1 --ticks 1000 --seed 42 --mode fault-schedule`
- Result: 134 cumulative edges, 27 registered assertions, 17 exercised and passing, 0 failing, 0 bugs.
- Known gaps: single seed, one VM, three rounds, no bug found, no replay attempt, no coverage growth after round 1.
- Host binary hash, kernel hash, initrd hash, and disk-image hash are recorded in `runtime-artifacts.json`.
- Receipt: `receipt.json` (known-gap because no bug and no replay attempt).

## Non-claims

- Does not prove that a directional fault was applied in this campaign.
- Does not prove CPU-throttle, custom-effect, or pause-control behavior.
- Does not prove universal determinism, host equivalence, workload correctness, or release eligibility.
