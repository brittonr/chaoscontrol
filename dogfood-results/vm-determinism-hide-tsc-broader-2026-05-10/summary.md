# VM determinism hide-tsc broader matrix — 2026-05-10

- git rev under test: `967c133903e20f3f44e916ad7275c72ca985fa48` plus working-tree change propagating `--single-clock-profile hide-tsc` to 2-vCPU and controller VM configs.
- command: `RUSTC_WRAPPER= CARGO_TARGET_DIR=target cargo run -p chaoscontrol-vmm --bin determinism_stress -- /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux /nix/store/iwnqg98k0vy4kk80cn76a0kwpdajk434-chaoscontrol-initrd-rust-workload 5 --single-clock-profile hide-tsc --receipt dogfood-results/vm-determinism-hide-tsc-broader-2026-05-10/receipt.json --dlog-dir dogfood-results/vm-determinism-hide-tsc-broader-2026-05-10/dlogs`
- profile behavior: all single/controller VM configs hide guest CPUID TSC and append `clocksource=jiffies notsc`.
- receipt: `receipt.json` (raw dlogs were generated for structural comparison and removed from committed evidence).

| case | runs | passed | serial/fingerprint mismatches | dlog structural match | dlog mismatches | divergence classes |
| --- | ---: | --- | ---: | --- | ---: | --- |
| `single-vm-1vcpu` | 5 | True | 0 | True | 0 | `[]` |
| `single-vm-2vcpu` | 5 | True | 0 | True | 0 | `[]` |
| `controller-3vm-1vcpu` | 5 | True | 0 | True | 0 | `[]` |
| `controller-3vm-2vcpu` | 5 | True | 0 | True | 0 | `[]` |

## Interpretation

The hide-tsc+jiffies profile now covers the broader matrix (`single-vm-1vcpu`, `single-vm-2vcpu`, `controller-3vm-1vcpu`, `controller-3vm-2vcpu`) and passed 5/5 for each case with no fingerprint or dlog structural mismatches. This promotes the time-source hypothesis from single-case isolation to a broader DST VM confidence improvement, while still remaining a named A/B profile rather than a default VM determinism claim.
