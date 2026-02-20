#!/usr/bin/env bash
# Build the SDK-instrumented guest binary and package it as an initrd.
#
# Output: guest/initrd-sdk.gz
#
# The binary is statically linked (musl) so the initrd needs no shared
# libraries.  It runs as PID 1 inside the ChaosControl VM.
#
# Usage:
#   nix develop --command bash -c "scripts/build-guest.sh"

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

TARGET="x86_64-unknown-linux-musl"
# Respect CARGO_TARGET_DIR if set (e.g. by nix develop)
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
BINARY="${TARGET_DIR}/${TARGET}/release/chaoscontrol-guest"
OUTPUT="guest/initrd-sdk.gz"

echo "==> Building chaoscontrol-guest for ${TARGET} ..."
cargo build --release --target "$TARGET" -p chaoscontrol-guest

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

SIZE=$(stat -c %s "$BINARY" 2>/dev/null || stat -f %z "$BINARY")
echo "    Binary size: ${SIZE} bytes"
echo "    Linked: $(file "$BINARY" | grep -o 'statically linked' || echo 'dynamically linked')"

# Create minimal initrd with the binary as /init
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

mkdir -p "$TMPDIR"/{dev,proc,sys/kernel/debug}
cp "$BINARY" "$TMPDIR/init"
chmod +x "$TMPDIR/init"

# Create initrd.gz
mkdir -p guest
(cd "$TMPDIR" && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > "$OUTPUT"

INITRD_SIZE=$(stat -c %s "$OUTPUT" 2>/dev/null || stat -f %z "$OUTPUT")
echo "==> Built ${OUTPUT} (${INITRD_SIZE} bytes)"
