#!/usr/bin/env bash
# trace-boot.sh — Run a bpftrace script while booting the VM
#
# Usage: sudo ./trace-boot.sh [script]
#   Scripts:
#     pit        — PIT calibration I/O and IRQ trace (default)
#     profile    — Exit reason + I/O port + MSR histogram
#     determinism — Verify PIT suppression + IRQ injection
#
# Example:
#   sudo ./trace-boot.sh pit
#   (in another terminal: cargo run --release --bin boot -- <kernel> <initrd>)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

SCRIPT_NAME="${1:-pit}"

case "$SCRIPT_NAME" in
    pit)         BT_FILE="$SCRIPT_DIR/kvm-pit-calibration.bt" ;;
    profile)     BT_FILE="$SCRIPT_DIR/kvm-exit-profile.bt" ;;
    determinism) BT_FILE="$SCRIPT_DIR/kvm-determinism-check.bt" ;;
    *)
        echo "Unknown script: $SCRIPT_NAME"
        echo "Available: pit, profile, determinism"
        exit 1
        ;;
esac

if [[ $EUID -ne 0 ]]; then
    echo "Error: must run with sudo"
    echo "Usage: sudo $0 $SCRIPT_NAME"
    exit 1
fi

echo "=== ChaosControl KVM Tracer ==="
echo "Script: $SCRIPT_NAME ($BT_FILE)"
echo ""
echo "Start the VM in another terminal:"
echo "  cd $REPO_DIR"
echo "  nix develop --command bash -c 'RUST_LOG=error cargo run --release --bin boot -- <kernel> <initrd>'"
echo ""
echo "Press Ctrl-C to stop tracing."
echo ""

exec bpftrace "$BT_FILE"
