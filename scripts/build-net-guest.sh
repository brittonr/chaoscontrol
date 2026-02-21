#!/usr/bin/env bash
# Build the networking demo guest and package it as an initrd.
#
# Output: guest/initrd-net.gz
#
# Usage:
#   nix develop --command bash -c "scripts/build-net-guest.sh"

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

TARGET="x86_64-unknown-linux-musl"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
BINARY="${TARGET_DIR}/${TARGET}/release/chaoscontrol-net-guest"
OUTPUT="guest/initrd-net.gz"

echo "==> Building chaoscontrol-net-guest for ${TARGET} ..."
cargo build --release --target "$TARGET" -p chaoscontrol-net-guest

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

SIZE=$(stat -c %s "$BINARY" 2>/dev/null || stat -f %z "$BINARY")
echo "    Binary size: ${SIZE} bytes"

# Create minimal initrd
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

mkdir -p "$TMPDIR"/{dev,proc,sys,sys/kernel/debug}
cp "$BINARY" "$TMPDIR/init"
chmod +x "$TMPDIR/init"

mkdir -p guest
(cd "$TMPDIR" && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > "$OUTPUT"

INITRD_SIZE=$(stat -c %s "$OUTPUT" 2>/dev/null || stat -f %z "$OUTPUT")
echo "==> Built ${OUTPUT} (${INITRD_SIZE} bytes)"
