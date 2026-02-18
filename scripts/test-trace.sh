#!/usr/bin/env bash
# Live test of the eBPF tracing harness.
#
# Usage: nix develop --command bash scripts/test-trace.sh
#
# Prerequisites:
#   - cargo build --release (VMM + trace binaries)
#   - sudo NOPASSWD for chaoscontrol-trace binary
#   - /dev/kvm accessible

set -euo pipefail

TRACE="${CARGO_TARGET_DIR:-target}/release/chaoscontrol-trace"
BOOT="${CARGO_TARGET_DIR:-target}/release/boot"
KERNEL="/nix/store/dslmcyd0p4aq5hnvf7dnrr89jb8h90y1-linux-6.18.10-dev/vmlinux"
OUTDIR="/tmp/chaoscontrol-trace-test"

# Use CARGO_TARGET_DIR if set (for custom target dirs like ~/.cargo-target)
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    TRACE="$CARGO_TARGET_DIR/release/chaoscontrol-trace"
    BOOT="$CARGO_TARGET_DIR/release/boot"
fi

mkdir -p "$OUTDIR"

echo "=== Building release binaries ==="
cargo build --release -p chaoscontrol-vmm --bin boot 2>&1 | tail -3
cargo build --release -p chaoscontrol-trace 2>&1 | tail -3

echo ""
echo "=== Run 1: Boot VM with tracing ==="
# Start VMM in background
RUST_LOG=warn $BOOT "$KERNEL" &
VMM_PID=$!
echo "VMM PID: $VMM_PID"

# Small delay for KVM to initialize
sleep 0.2

# Attach tracer
echo "Attaching tracer..."
sudo "$TRACE" live --pid "$VMM_PID" --quiet --output "$OUTDIR/run1.json" 2>&1 &
TRACE_PID=$!

# Wait for VMM (it will HLT or crash since we have no proper initrd)
wait $VMM_PID 2>/dev/null || true
sleep 0.5
kill $TRACE_PID 2>/dev/null || true
wait $TRACE_PID 2>/dev/null || true

echo ""
echo "=== Run 1 Summary ==="
$TRACE summary --trace "$OUTDIR/run1.json"

echo ""
echo "=== Run 2: Identical boot for determinism check ==="
RUST_LOG=warn $BOOT "$KERNEL" &
VMM_PID=$!
echo "VMM PID: $VMM_PID"
sleep 0.2

sudo "$TRACE" live --pid "$VMM_PID" --quiet --output "$OUTDIR/run2.json" 2>&1 &
TRACE_PID=$!

wait $VMM_PID 2>/dev/null || true
sleep 0.5
kill $TRACE_PID 2>/dev/null || true
wait $TRACE_PID 2>/dev/null || true

echo ""
echo "=== Run 2 Summary ==="
$TRACE summary --trace "$OUTDIR/run2.json"

echo ""
echo "=== Determinism Verification ==="
$TRACE verify --trace-a "$OUTDIR/run1.json" --trace-b "$OUTDIR/run2.json"
RESULT=$?

echo ""
if [ $RESULT -eq 0 ]; then
    echo "✅ DETERMINISTIC — both runs produced identical KVM event sequences"
else
    echo "❌ NON-DETERMINISTIC — runs diverged (exit code $RESULT)"
    echo ""
    echo "Try filtered comparison (PIO only):"
    $TRACE verify --trace-a "$OUTDIR/run1.json" --trace-b "$OUTDIR/run2.json" --filter pio || true
fi

exit $RESULT
