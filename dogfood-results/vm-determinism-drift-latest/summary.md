# VM determinism drift packaged run

- Command: `nix run .#vm-determinism-drift`
- Result: PASS
- Receipt: `dogfood-results/vm-determinism-drift-latest/receipt.json`
- Kernel artifact: `/nix/store/96hxkvhlf1ifzvkrl5xpkigf3g2jv1m6-chaoscontrol-vmlinux/vmlinux`
- Initrd artifact: `/nix/store/mswqqg7ikfls4y7f3pi2gn181rgkzls7-chaoscontrol-initrd-rust-workload`
- Kernel CRC32: `crc32:decbd023`
- Initrd CRC32: `crc32:e587186e`

## Cases

| Case | Runs | Passed | Mismatches | Dlog structural match |
| --- | ---: | --- | ---: | --- |
| `single-vm-1vcpu` | 5 | yes | 0 | yes |
| `single-vm-2vcpu` | 5 | yes | 0 | yes |
| `controller-3vm-1vcpu` | 5 | yes | 0 | yes |
| `controller-3vm-2vcpu` | 5 | yes | 0 | yes |

Raw dlogs were generated under `dogfood-results/vm-determinism-drift-latest/dlogs/` as local debug evidence and are intentionally not tracked.
