# VM determinism hide-tsc evidence — 2026-05-10

- git rev under test: `6898a38c0c16db54c58ac0308320a157b5e9e17f` plus working-tree change adding `--single-clock-profile hide-tsc`.
- command: `RUSTC_WRAPPER= CARGO_TARGET_DIR=target cargo run -p chaoscontrol-vmm --bin determinism_stress -- /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux /nix/store/iwnqg98k0vy4kk80cn76a0kwpdajk434-chaoscontrol-initrd-rust-workload 5 --case single-vm-1vcpu --single-clock-profile hide-tsc --receipt dogfood-results/vm-determinism-hide-tsc-2026-05-10/hide-tsc-receipt.json --dlog-dir dogfood-results/vm-determinism-hide-tsc-2026-05-10/dlogs`
- profile behavior: hides guest CPUID TSC and appends `clocksource=jiffies notsc` after the default TSC clock arguments.
- result: `single-vm-1vcpu` passed `5/5`.
- reference fingerprint: exits `70000`, virtual_tsc `70000000`.
- serial mismatches: `0`; dlog structural match: `True`; dlog mismatches: `0`; divergence classes: `[]`.
- receipt: `hide-tsc-receipt.json` (raw dlogs were generated only for local inspection).

## Interpretation

The lower-level CPUID TSC hiding path is wired into the stress harness and still produces deterministic single-vCPU evidence under the jiffies/notsc profile. This does not by itself fix the broader multi-vCPU drift seam; it gives us a narrow VM-side switch to isolate time-source effects in follow-up A/B runs.
