# Validation evidence

Date: 2026-08-08

## Privileged live-VM evidence

Host: `britton-desktop` (kernel `6.18.41`, kvm_amd). Managed-host change
`3c3c7958` in onix-core granted brittonr NOPASSWD access to
`/home/brittonr/.cargo-target/release/ebpf-trace-evidence-selftest` alongside
the existing `chaoscontrol-trace` rule. The host provides tracefs+debugfs
(mounted), `/dev/kvm` (0666), BTF `/sys/kernel/btf/vmlinux`, and all 11
required KVM tracepoint `format` files under root.

Repeated privileged smoke runs against a live QEMU KVM guest (dev `vmlinux`
from this change's `result-blk-kernel`, `-accel kvm -cpu host -smp 2 -m 256M`,
`-append console=ttyS0`, `-no-shutdown`):

```text
ebpf trace evidence fixtures ok; privileged smoke not requested
ebpf privileged smoke complete: pid=2590937 accepted_records=2234 ...
ebpf privileged smoke complete: pid=4004585 accepted_records=2541 ...
```

Each run reports exit status 0 with real `kvm_exit`-class and IRQ-line
traffic at the traced TGID, producer/userspace accounting reconciled to
completeness, bounded final drain, detach, and cleanup completed, and a
stable target binding (pidfd + process-start + executable identity).

An earlier `panic=-1` reboot-loop guest produced `maximum_polls reached before
stop` (exit 1). That is the designed bounded-shutdown behavior: an unbounded
producer flood must not be misreported as a clean capture. The passing lane
uses a calm, alive guest within the poll budget.

## Commands

These checks passed:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .
cargo run -q -p chaoscontrol-trace --bin ebpf-trace-evidence-selftest
nix build -L --no-link \
  .#checks.x86_64-linux.ebpf-trace-evidence \
  .#checks.x86_64-linux.evidence-contracts \
  .#checks.x86_64-linux.tests \
  .#checks.x86_64-linux.clippy
nix run path:/home/brittonr/git/OnixResearch/cairn#cairn -- validate --root .
nix run path:/home/brittonr/git/OnixResearch/cairn#cairn -- gate proposal|design|tasks harden-ebpf-trace-evidence --root . --policy <generated-policy>
```

The Nix client could not connect to `ssh-ng://root@10.10.10.1`. Nix completed
each derivation on the local builder.

## Evidence boundary

This evidence reports bounded host agreement for the exact profile and smoke
lane. It does not prove VM determinism, kernel correctness, BPF safety,
security, production readiness, or release eligibility.