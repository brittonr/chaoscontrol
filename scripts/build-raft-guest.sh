#!/usr/bin/env bash
# Build the Raft guest binary and package as initrd.
# Output: guest/initrd-raft.gz
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

TARGET="x86_64-unknown-linux-musl"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
BINARY="${TARGET_DIR}/${TARGET}/release/chaoscontrol-raft-guest"
OUTPUT="guest/initrd-raft.gz"

echo "==> Building chaoscontrol-raft-guest for ${TARGET} ..."
cargo build --release --target "$TARGET" -p chaoscontrol-raft-guest

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

SIZE=$(stat -c %s "$BINARY" 2>/dev/null || stat -f %z "$BINARY")
echo "    Binary size: ${SIZE} bytes"

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

mkdir -p "$TMPDIR"/{dev,proc,sys/kernel/debug}
cp "$BINARY" "$TMPDIR/init"
chmod +x "$TMPDIR/init"

mkdir -p guest
(cd "$TMPDIR" && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > "$OUTPUT"

INITRD_SIZE=$(stat -c %s "$OUTPUT" 2>/dev/null || stat -f %z "$OUTPUT")
echo "==> Built ${OUTPUT} (${INITRD_SIZE} bytes)"
