# VM Determinism A/B — single-vCPU clock profile — 2026-05-10

Receipts:
- Baseline TSC: `baseline-receipt.json` (`0f307bfb893f738a5e61158316a6779b44295c92aeadc5222fc5ba3487b37457`, 54,956 bytes)
- Appended jiffies/notsc: `jiffies-receipt.json` (`9c77ea4b04b38f14104bfd7c419f85860e88cd78871e356edd63d87851e7e913`, 58,623 bytes)

Result: the narrow A/B did **not** promote `clocksource=jiffies notsc` for single-vCPU. Baseline TSC passed 5/5; the jiffies/notsc variant failed with the same PIT/TSC calibration class plus a PCI-config first dlog divergence.

This is bounded evidence for the pinned kernel/initrd and `single-vm-1vcpu` case only. It is not a universal VM determinism claim. Raw dlogs and terminal logs were treated as local debug artifacts; the committed receipts preserve the machine-readable mismatch classes and first-difference excerpts.

| Profile | Runs | Exit | Status | Divergence classes | Fingerprint mismatches | dlog mismatches |
| --- | ---: | ---: | --- | --- | ---: | ---: |
| `tsc` | 5 | 0 | `passed` | none | 0 | 0 |
| `jiffies` (`clocksource=jiffies notsc` appended) | 5 | 1 | `failed` | `pit-tsc-calibration`, `pci-config-access` | 1 | 1 |

## Commands

```bash
nix build .#kcov-vmlinux .#initrd-rust-workload --no-link --print-out-paths

RUSTC_WRAPPER= CARGO_TARGET_DIR=target cargo run -p chaoscontrol-vmm --bin determinism_stress -- \
  /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux \
  /nix/store/30z2dm8jnpbrxiw30bbyq4sgcqam3avq-chaoscontrol-initrd-rust-workload \
  5 --case single-vm-1vcpu --single-clock-profile tsc \
  --receipt dogfood-results/vm-determinism-ab-single-clock-2026-05-10/baseline-receipt.json \
  --dlog-dir dogfood-results/vm-determinism-ab-single-clock-2026-05-10/baseline-dlogs

RUSTC_WRAPPER= CARGO_TARGET_DIR=target cargo run -p chaoscontrol-vmm --bin determinism_stress -- \
  /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux \
  /nix/store/30z2dm8jnpbrxiw30bbyq4sgcqam3avq-chaoscontrol-initrd-rust-workload \
  5 --case single-vm-1vcpu --single-clock-profile jiffies \
  --receipt dogfood-results/vm-determinism-ab-single-clock-2026-05-10/jiffies-receipt.json \
  --dlog-dir dogfood-results/vm-determinism-ab-single-clock-2026-05-10/jiffies-dlogs
```
