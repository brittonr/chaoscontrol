# VM Determinism Drift Gate — 2026-05-09

Receipt: `receipt.json`
Receipt SHA-256: `c0b1cda9710f11d137887e08fa37ada36902dd5c569add07732ad48d7c12e367`

Result: `failed` (exit `1`) across 10 runs per case.

This is bounded drift evidence for selected VM/controller configurations only. It is not a universal hypervisor/device/timing determinism proof. Raw dlog files were treated as local debug artifacts; the committed receipt and summary preserve the mismatch classes and first-difference excerpts.

| Case | Runs | Status | Fingerprint mismatches | dlog structural match | dlog mismatches |
| --- | ---: | --- | ---: | --- | ---: |
| `single-vm-1vcpu` | 10 | `failed` | 1 | `false` | 1 |
| `single-vm-2vcpu` | 10 | `failed` | 4 | `false` | 4 |
| `controller-3vm-1vcpu` | 10 | `failed` | 0 | `false` | 25 |
| `controller-3vm-2vcpu` | 10 | `failed` | 0 | `false` | 27 |

## Command

```bash
RUSTC_WRAPPER= CARGO_TARGET_DIR=target cargo run -p chaoscontrol-vmm --bin determinism_stress -- /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux /nix/store/iwnqg98k0vy4kk80cn76a0kwpdajk434-chaoscontrol-initrd-rust-workload 10 --receipt dogfood-results/vm-determinism-drift-2026-05-09/receipt.json --dlog-dir dogfood-results/vm-determinism-drift-2026-05-09/dlogs
```
