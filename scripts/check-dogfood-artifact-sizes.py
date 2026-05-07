#!/usr/bin/env python3
"""Guard committed dogfood evidence against oversized blobs.

This check is intentionally repository-native: it scans the source tree that Nix
sees, so it catches tracked/staged dogfood evidence files before they become new
large Git blobs. Raw runtime logs/checkpoints are ignored by policy, but if one is
force-added or otherwise enters the flake source, this guard fails it too.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

DEFAULT_MAX_BYTES = 50 * 1024 * 1024
DEFAULT_ROOT = "dogfood-results"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(DEFAULT_ROOT),
        help=f"evidence root to scan (default: {DEFAULT_ROOT})",
    )
    parser.add_argument(
        "--max-bytes",
        type=int,
        default=DEFAULT_MAX_BYTES,
        help=f"maximum allowed file size in bytes (default: {DEFAULT_MAX_BYTES})",
    )
    return parser.parse_args()


def relative(path: Path) -> str:
    try:
        return str(path.relative_to(Path.cwd()))
    except ValueError:
        return str(path)


def main() -> int:
    args = parse_args()
    if args.max_bytes <= 0:
        print("--max-bytes must be positive", file=sys.stderr)
        return 2

    root = args.root
    if not root.exists():
        print(f"dogfood artifact size guard: {relative(root)} absent; nothing to scan")
        return 0
    if not root.is_dir():
        print(f"dogfood artifact size guard: {relative(root)} is not a directory", file=sys.stderr)
        return 2

    oversized: list[tuple[Path, int]] = []
    scanned = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        scanned += 1
        size = path.stat().st_size
        if size > args.max_bytes:
            oversized.append((path, size))

    if oversized:
        print(
            f"dogfood artifact size guard failed: {len(oversized)} file(s) exceed "
            f"{args.max_bytes} bytes",
            file=sys.stderr,
        )
        for path, size in oversized:
            print(f"  {relative(path)}: {size} bytes", file=sys.stderr)
        print(
            "Use chunked snapshot evidence (<snapshot>.chunks.json + .partNN), "
            "artifact summaries, or external storage instead of committing large blobs.",
            file=sys.stderr,
        )
        return 1

    print(
        f"dogfood artifact size guard ok: scanned {scanned} file(s), "
        f"max allowed {args.max_bytes} bytes"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
